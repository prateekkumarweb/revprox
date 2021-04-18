#[macro_use]
mod macros;

use anyhow::Context;
use handler::Handler;
use hyper::{
    server::conn::{AddrIncoming, AddrStream},
    service::{make_service_fn, service_fn},
    Server,
};
use std::{fs::File, io::BufReader, net::SocketAddr, sync::Arc};
use structopt::StructOpt;
use tokio_rustls::rustls::{
    internal::pemfile::{certs, pkcs8_private_keys},
    NoClientAuth, ServerConfig,
};
use tracing::info;

mod handler;
mod opt;
mod settings;
mod tls;
mod tssh;
mod tunnel;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let opt = opt::Opt::from_args();
    let settings = settings::Settings::from_config_file(opt.config);
    let handler = Handler::new(settings.servers(), opt.tls);
    let handler = Arc::new(handler);

    let addr = SocketAddr::from(([127, 0, 0, 1], opt.port));

    let mut incoming = AddrIncoming::bind(&addr)?;
    incoming.set_nodelay(true);

    if opt.tls {
        let cert = certs(&mut BufReader::new(File::open("./cert.pem").unwrap())).unwrap();
        let mut keys =
            pkcs8_private_keys(&mut BufReader::new(File::open("./key.pem").unwrap())).unwrap();

        let mut server_config = ServerConfig::new(NoClientAuth::new());
        server_config.set_single_cert(cert, keys.remove(0)).unwrap();

        info!("Starting https server on port {}", opt.port);
        create_server!(tls: handler, incoming, server_config);
    } else {
        tssh::main().await?;
        info!("Starting http server on port {}", opt.port);
        create_server!(handler, incoming);
    };

    Ok(())
}
