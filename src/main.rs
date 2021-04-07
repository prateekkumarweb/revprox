use hyper::{
    header::{self, HeaderValue},
    http::uri,
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Client, HeaderMap, Request, Response, Server, StatusCode, Uri,
};
use lazy_static::lazy_static;
use std::{
    collections::HashMap,
    convert::{Infallible, TryInto},
    net::SocketAddr,
    sync::Arc,
};
use structopt::StructOpt;

mod opt;
mod settings;

lazy_static! {
    static ref HOP_HEADERS: Vec<&'static str> = vec![
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade"
    ];
}

#[tokio::main]
async fn main() {
    let opt = opt::Opt::from_args();
    let settings = settings::Settings::from_config_file(opt.config);
    let servers_map = Arc::new(settings.servers());

    let make_service = make_service_fn(|conn: &AddrStream| {
        let servers_map = servers_map.clone();
        let addr = conn.remote_addr();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle_client(req, addr, servers_map.clone())
            }))
        }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], opt.port));
    let server = Server::bind(&addr).serve(make_service);

    println!("Listening on port {}", opt.port);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}

async fn handle_client(
    mut req: Request<Body>,
    addr: SocketAddr,
    servers_map: Arc<HashMap<String, String>>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    dbg!(&req);

    let client = Client::new();
    let host = req.headers().get(header::HOST);

    // TODO: Decide if host or authority from uri is to be used
    let uri: Uri = host
        .and_then(|host| host.to_str().ok())
        .and_then(|host| servers_map.get(host))
        .and_then(|uri| uri.parse().ok())
        .unwrap_or("http://127.0.0.1:8000/".parse()?);

    let uri_parts = uri.into_parts();
    let uri_builder = Uri::builder()
        .scheme(uri_parts.scheme.unwrap_or(uri::Scheme::HTTP))
        .authority(uri_parts.authority.unwrap())
        .path_and_query((req.uri().path_and_query().unwrap()).clone());

    let mut new_req_builder = Request::builder()
        .method(req.method())
        .uri(uri_builder.build()?)
        .version(req.version());

    for (key, value) in req.headers() {
        new_req_builder = new_req_builder.header(key, value);
    }

    let new_headers_mut = new_req_builder.headers_mut().unwrap();

    strip_connection_and_hop_headers(new_headers_mut);

    let upgrade_type = find_upgrade_type(req.headers());
    if let Some(upgrade) = upgrade_type {
        new_headers_mut.insert(header::CONNECTION, "Upgrade".try_into().unwrap());
        new_headers_mut.insert(header::UPGRADE, upgrade.try_into().unwrap());
    }

    insert_forwarded_headers(new_headers_mut, addr);

    let body = hyper::body::to_bytes(req.body_mut()).await?;
    let new_req = new_req_builder.body(hyper::Body::from(body))?;

    dbg!(&new_req);

    let mut res = client.request(new_req).await?;

    dbg!(&res);

    Ok(if res.status() == StatusCode::SWITCHING_PROTOCOLS {
        handle_upgrade(req, res).await?
    } else {
        strip_connection_and_hop_headers(res.headers_mut());
        res
    })
}

async fn handle_upgrade(
    mut req: Request<Body>,
    mut res: Response<Body>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    match hyper::upgrade::on(&mut res).await {
        Ok(upgraded_server) => {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(&mut req).await {
                    Ok(upgraded_client) => {
                        let (mut server_reader, mut server_writer) =
                            tokio::io::split(upgraded_server);
                        let (mut client_reader, mut client_writer) =
                            tokio::io::split(upgraded_client);
                        let server_to_client =
                            tokio::io::copy(&mut server_reader, &mut client_writer);
                        let client_to_server =
                            tokio::io::copy(&mut client_reader, &mut server_writer);
                        tokio::try_join!(server_to_client, client_to_server).unwrap();
                    }
                    Err(e) => eprintln!("upgrade error: {}", e),
                };
            });
        }
        Err(e) => eprintln!("upgrade error: {}", e),
    }

    Ok(res)
}

fn find_upgrade_type(headers: &HeaderMap<HeaderValue>) -> Option<&[u8]> {
    let is_upgrade = headers
        .get(header::CONNECTION)
        .and_then(|val| val.to_str().ok())
        .map(|val| val.to_lowercase().split(",").any(|v| v.trim() == "upgrade"))
        .unwrap_or(false);
    if is_upgrade {
        headers.get(header::UPGRADE).map(|v| v.as_bytes())
    } else {
        None
    }
}

fn strip_connection_and_hop_headers(headers_mut: &mut HeaderMap<HeaderValue>) {
    // Strip connection headers
    if let Some(connection_headers) = headers_mut.get(header::CONNECTION) {
        let connection_headers = connection_headers
            .to_str()
            .unwrap()
            .split(",")
            .map(|v| v.trim().to_owned())
            .collect::<Vec<_>>();

        for header in connection_headers {
            headers_mut.remove(header);
        }
    }

    for header in HOP_HEADERS.iter() {
        headers_mut.remove(*header);
    }
}

fn insert_forwarded_headers(headers_mut: &mut HeaderMap<HeaderValue>, addr: SocketAddr) {
    let client_ip = addr.ip();

    if headers_mut.contains_key("X-Forwarded-For") {
        let prior_ips = headers_mut.get_mut("X-Forwarded-For").unwrap();
        let mut all_ips = vec![];
        all_ips.extend_from_slice(prior_ips.as_bytes());
        all_ips.extend_from_slice(format!(", {}", client_ip).as_bytes());
        *prior_ips = HeaderValue::from_bytes(&all_ips).unwrap();
    } else {
        headers_mut.insert(
            "X-Forwarded-For",
            HeaderValue::from_str(&format!("{}", client_ip)).unwrap(),
        );
    }

    if !headers_mut.contains_key("X-Forwarded-Proto") {
        // TODO: How to find the protocol used by client?
        headers_mut.insert(
            "X-Forwarded-Proto",
            HeaderValue::from_bytes(b"https").unwrap(),
        );
    }
}
