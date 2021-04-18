#[macro_use]
mod macros;

use anyhow::Context;
use handler::Handler;
use hyper::{
    server::conn::{AddrIncoming, AddrStream},
    service::{make_service_fn, service_fn},
    Server,
};
use opt::Opt;
use std::{fs::File, io::BufReader, net::SocketAddr, sync::Arc};
use structopt::StructOpt;
use tokio_rustls::rustls::{
    internal::pemfile::{certs, pkcs8_private_keys},
    NoClientAuth, ServerConfig,
};
use tracing::info;
use tunnel::Tunnel;

mod async_ssh;
mod client;
mod handler;
mod opt;
mod server;
mod settings;
mod tls;
mod tunnel;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let opt = Opt::from_args();

    match opt {
        Opt::Client { .. } => {
            // let settings = settings::Settings::from_config_file(config);
            let mut tunnel = Tunnel::new("10.108.79.149:22", 8080, 8081).await.unwrap();
            tunnel.start_tunnel().await?;
        }
        Opt::Server { tls, port, config } => {
            let settings = settings::Settings::from_config_file(config);
            let handler = Handler::new(settings.servers(), tls);
            let handler = Arc::new(handler);

            let addr = SocketAddr::from(([127, 0, 0, 1], port));

            let mut incoming = AddrIncoming::bind(&addr)?;
            incoming.set_nodelay(true);

            if tls {
                let cert = certs(&mut BufReader::new(File::open("./cert.pem").unwrap())).unwrap();
                let mut keys =
                    pkcs8_private_keys(&mut BufReader::new(File::open("./key.pem").unwrap()))
                        .unwrap();

                let mut server_config = ServerConfig::new(NoClientAuth::new());
                server_config.set_single_cert(cert, keys.remove(0)).unwrap();

                info!("Starting https server on port {}", port);
                create_server!(tls: handler, incoming, server_config);
            } else {
                async_ssh::main().await?;
                info!("Starting http server on port {}", port);
                create_server!(handler, incoming);
            };
        }
    }

    Ok(())
}
