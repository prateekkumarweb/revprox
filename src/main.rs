use handler::Handler;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Server,
};
use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use structopt::StructOpt;

mod handler;
mod opt;
mod settings;

#[tokio::main]
async fn main() {
    let opt = opt::Opt::from_args();
    let settings = settings::Settings::from_config_file(opt.config);
    let handler = Handler::new(settings.servers());
    let handler = Arc::new(handler);

    let make_service = make_service_fn(|conn: &AddrStream| {
        let handler = handler.clone();
        let addr = conn.remote_addr();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let handler = handler.clone();
                handler.handle_client(addr, req)
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
