use std::{convert::Infallible, net::SocketAddr};

use hyper::{
    header,
    service::{make_service_fn, service_fn},
    Body, Client, Request, Response, Server,
};

async fn handle_client(
    req: Request<Body>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new();
    println!("Host: {:?}", req.headers().get(header::HOST));
    let host = req.headers().get(header::HOST);
    let uri = match host {
        Some(host) => match host.to_str() {
            Ok("a.localhost:9000") => "http://127.0.0.1:8000/".parse()?,
            Ok("b.localhost:9000") => "http://127.0.0.1:8001/".parse()?,
            _ => "http://127.0.0.1:8000/".parse()?,
        },
        None => "http://127.0.0.1:8000/".parse()?,
    };
    let mut new_req = Request::from(req);
    *new_req.uri_mut() = uri;
    let resp = client.request(new_req).await?;
    Ok(resp)
}

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 9000));
    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_client)) });
    let server = Server::bind(&addr).serve(make_svc);

    println!("Listening on port 9000");
    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}
