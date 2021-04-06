use hyper::{
    header::{self, HeaderValue},
    http::uri,
    service::{make_service_fn, service_fn},
    Body, Client, Request, Response, Server, StatusCode, Uri,
};
use lazy_static::lazy_static;
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};
use structopt::StructOpt;

mod opt;
mod settings;

lazy_static! {
    static ref CONNECTION_HEADERS: Vec<&'static str> = vec![
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

async fn handle_client(
    mut req: Request<Body>,
    servers_map: Arc<HashMap<String, String>>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    dbg!(&req);

    let client = Client::new();
    let host = req.headers().get(header::HOST);

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

    let is_upgrade_websocket_request =
        req.headers().get(header::UPGRADE) == Some(&HeaderValue::from_bytes(b"websocket").unwrap());

    let mut new_req_builder = Request::builder()
        .method(req.method())
        .uri(uri_builder.build()?)
        .version(req.version());

    for (key, value) in req.headers() {
        new_req_builder = new_req_builder.header(key, value);
    }

    // Strip hop by hop headers
    if let Some(connection_headers) = req.headers().get(header::CONNECTION) {
        let connection_headers = connection_headers
            .to_str()
            .unwrap()
            .split(",")
            .map(|v| v.trim())
            .collect::<Vec<_>>();

        for header in connection_headers {
            new_req_builder.headers_mut().unwrap().remove(header);
        }
    }
    for header in CONNECTION_HEADERS.iter() {
        new_req_builder.headers_mut().unwrap().remove(*header);
    }

    if is_upgrade_websocket_request {
        new_req_builder = new_req_builder
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket");
    }

    let body = hyper::body::to_bytes(req.body_mut()).await?;
    let new_req = new_req_builder.body(hyper::Body::from(body))?;

    dbg!(&new_req);

    let res = client.request(new_req).await?;

    dbg!(&res);

    Ok(if res.status() == StatusCode::SWITCHING_PROTOCOLS {
        handle_ws(req, res).await?
    } else {
        res
    })
}

async fn handle_ws(
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

#[tokio::main]
async fn main() {
    let opt = opt::Opt::from_args();
    let settings = settings::Settings::from_config_file(opt.config);
    let servers_map = Arc::new(settings.servers());

    let make_svc = make_service_fn(move |_conn| {
        let servers_map = servers_map.clone();
        async {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle_client(req, servers_map.clone())
            }))
        }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], opt.port));
    let server = Server::bind(&addr).serve(make_svc);

    println!("Listening on port {}", opt.port);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}
