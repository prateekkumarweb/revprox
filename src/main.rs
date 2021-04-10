use anyhow::Context;
use handler::Handler;
use hyper::{
    server::conn::AddrIncoming,
    service::{make_service_fn, service_fn},
    Server,
};
use std::{fs::File, io::BufReader, net::SocketAddr, sync::Arc};
use structopt::StructOpt;
use tokio_rustls::rustls::{
    internal::pemfile::{certs, pkcs8_private_keys},
    NoClientAuth, ServerConfig,
};

mod handler;
mod opt;
mod settings;
mod tls;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = opt::Opt::from_args();
    let settings = settings::Settings::from_config_file(opt.config);
    let handler = Handler::new(settings.servers());
    let handler = Arc::new(handler);

    let cert = certs(&mut BufReader::new(File::open("./cert.pem").unwrap())).unwrap();
    let mut keys =
        pkcs8_private_keys(&mut BufReader::new(File::open("./key.pem").unwrap())).unwrap();

    let mut server_config = ServerConfig::new(NoClientAuth::new());
    server_config.set_single_cert(cert, keys.remove(0)).unwrap();

    let addr = SocketAddr::from(([127, 0, 0, 1], opt.port));

    let mut incoming = AddrIncoming::bind(&addr)?;
    incoming.set_nodelay(true);

    let make_service = make_service_fn(|conn: &tls::TlsStream| {
        let handler = handler.clone();
        let addr = conn.remote_addr();
        async move {
            Ok::<_, anyhow::Error>(service_fn(move |req| {
                let handle_future = handler.clone().handle_client(addr, req);
                async { handle_future.await.context("Failed to handle client") }
            }))
        }
    });

    let server =
        Server::builder(tls::TlsAcceptor::new(server_config, incoming)).serve(make_service);

    println!("Listening on port {}", opt.port);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
