macro_rules! create_service {
    ($handler:ident) => {
        make_service_fn(|conn: &AddrStream| {
            let handler = $handler.clone();
            let addr = conn.remote_addr();
            async move {
                Ok::<_, anyhow::Error>(service_fn(move |req| {
                    let handle_future = handler.clone().handle_client(addr, req);
                    async { handle_future.await.context("Failed to handle client") }
                }))
            }
        })
    };

    (tls: $handler:ident) => {
        make_service_fn(|conn: &tls::TlsStream| {
            let handler = $handler.clone();
            let addr = conn.remote_addr();
            async move {
                Ok::<_, anyhow::Error>(service_fn(move |req| {
                    let handle_future = handler.clone().handle_client(addr, req);
                    async { handle_future.await.context("Failed to handle client") }
                }))
            }
        })
    };
}

macro_rules! server_await {
    ($server:expr) => {
        if let Err(e) = $server.await {
            tracing::error!("Server error: {}", e);
        }
    };
}

macro_rules! create_server {
    ($handler:ident, $incoming:expr) => {
        let make_service = create_service!($handler);
        let server = Server::builder($incoming).serve(make_service);

        server_await!(server);
    };

    (tls: $handler:ident, $incoming:expr, $server_config:expr) => {
        let make_service = create_service!(tls: $handler);
        let server =
            Server::builder(tls::TlsAcceptor::new($server_config, $incoming)).serve(make_service);

        server_await!(server);
    };
}
