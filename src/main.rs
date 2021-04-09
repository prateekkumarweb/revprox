use futures_util::stream::Stream;
use handler::Handler;
use hyper::{
    service::{make_service_fn, service_fn},
    Server,
};
use std::{
    convert::Infallible,
    fs::File,
    io::BufReader,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{
    rustls::{
        internal::pemfile::{certs, pkcs8_private_keys},
        NoClientAuth, ServerConfig,
    },
    server::TlsStream,
    TlsAcceptor,
};

mod handler;
mod opt;
mod settings;

#[tokio::main]
async fn main() {
    let opt = opt::Opt::from_args();
    let settings = settings::Settings::from_config_file(opt.config);
    let handler = Handler::new(settings.servers());
    let handler = Arc::new(handler);

    let cert = certs(&mut BufReader::new(File::open("./cert.pem").unwrap())).unwrap();
    let mut keys =
        pkcs8_private_keys(&mut BufReader::new(File::open("./key.pem").unwrap())).unwrap();

    let mut server_config = ServerConfig::new(NoClientAuth::new());
    server_config.set_single_cert(cert, keys.remove(0)).unwrap();
    let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

    let addr = SocketAddr::from(([127, 0, 0, 1], opt.port));
    let tcp = TcpListener::bind(&addr).await.unwrap();

    let incoming_tls_stream = async_stream::stream! {
        loop {
            let (socket, _) = tcp.accept().await.unwrap();
            let stream = tls_acceptor.accept(socket);
            yield stream.await;
        }
    };

    let make_service = make_service_fn(|conn: &TlsStream<TcpStream>| {
        let handler = handler.clone();
        let addr = conn.get_ref().0.peer_addr().unwrap();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let handler = handler.clone();
                handler.handle_client(addr, req)
            }))
        }
    });

    let server = Server::builder(HyperAcceptor {
        acceptor: Box::pin(incoming_tls_stream),
    })
    .serve(make_service);

    println!("Listening on port {}", opt.port);

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}

struct HyperAcceptor<'a> {
    acceptor: Pin<Box<dyn Stream<Item = Result<TlsStream<TcpStream>, std::io::Error>> + 'a>>,
}

impl hyper::server::accept::Accept for HyperAcceptor<'_> {
    type Conn = TlsStream<TcpStream>;
    type Error = std::io::Error;

    fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        Pin::new(&mut self.acceptor).poll_next(cx)
    }
}
