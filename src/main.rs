use hyper::{
    header,
    http::uri,
    service::{make_service_fn, service_fn},
    Body, Client, Request, Response, Server, Uri,
};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};
use structopt::StructOpt;

mod opt;
mod settings;

async fn handle_client(
    req: Request<Body>,
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

    let mut new_req = Request::from(req);

    *new_req.uri_mut() = uri_builder.build()?;

    dbg!(&new_req);

    let resp = client.request(new_req).await?;

    dbg!(&resp);

    Ok(resp)
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

    let addr = SocketAddr::from(([127, 0, 0, 1], 9000));
    let server = Server::bind(&addr).serve(make_svc);

    println!("Listening on port 9000");

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}
