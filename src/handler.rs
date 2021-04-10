use hyper::{
    header::{self, HeaderValue},
    http::uri,
    Body, Client, HeaderMap, Request, Response, StatusCode, Uri,
};
use lazy_static::lazy_static;
use std::{collections::HashMap, convert::TryInto, net::SocketAddr, sync::Arc};

pub struct Handler {
    servers_map: HashMap<String, String>,
}

impl Handler {
    pub fn new(servers_map: HashMap<String, String>) -> Self {
        Self { servers_map }
    }

    pub async fn handle_client(
        self: Arc<Self>,
        addr: SocketAddr,
        mut req: Request<Body>,
    ) -> anyhow::Result<Response<Body>> {
        dbg!(&req);

        let client = Client::new();
        let host = req.headers().get(header::HOST);

        // TODO: Decide if host or authority from uri is to be used
        let uri: Uri = host
            .and_then(|host| host.to_str().ok())
            .and_then(|host| self.servers_map.get(host))
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
            new_headers_mut.insert(header::CONNECTION, header::UPGRADE.into());
            new_headers_mut.insert(header::UPGRADE, upgrade.try_into().unwrap());
        }

        insert_forwarded_headers(new_headers_mut, addr);

        // TODO: This creates a copy of req body in memory. any way to avoid it?
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
}

async fn handle_upgrade(
    mut req: Request<Body>,
    mut res: Response<Body>,
) -> anyhow::Result<Response<Body>> {
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
    const X_FORWARDED_FOR: &'static str = "X-Forwarded-For";
    const X_FORWARDED_PROTO: &'static str = "X-Forwarded-Proto";

    let client_ip = addr.ip();

    if headers_mut.contains_key(X_FORWARDED_FOR) {
        let prior_ips = headers_mut.get_mut(X_FORWARDED_FOR).unwrap();
        let mut all_ips = vec![];
        all_ips.extend_from_slice(prior_ips.as_bytes());
        all_ips.extend_from_slice(format!(", {}", client_ip).as_bytes());
        *prior_ips = HeaderValue::from_bytes(&all_ips).unwrap();
    } else {
        headers_mut.insert(
            X_FORWARDED_FOR,
            HeaderValue::from_str(&format!("{}", client_ip)).unwrap(),
        );
    }

    if !headers_mut.contains_key(X_FORWARDED_PROTO) {
        // TODO: How to find the protocol used by client?
        headers_mut.insert(X_FORWARDED_PROTO, HeaderValue::from_static("https"));
    }
}
