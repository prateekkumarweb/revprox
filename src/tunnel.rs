use crate::async_ssh::{AsyncSession, HandshakenAsyncSession};
use std::net::TcpStream;
use tokio::io;
use tracing::{error, info};

pub struct Tunnel {
    client_port: u16,
    server_port: u16,
    session: HandshakenAsyncSession,
}

impl Tunnel {
    pub async fn new(ssh_server: &str, client_port: u16, server_port: u16) -> io::Result<Self> {
        let tcp = TcpStream::connect(ssh_server)?;
        let session = AsyncSession::new(tcp)?.handshake().await?;

        session.userauth_pubkey_file().await?;

        assert!(session.authenticated());

        Ok(Tunnel {
            client_port,
            server_port,
            session,
        })
    }

    pub async fn start_tunnel(&mut self) -> io::Result<Self> {
        let (mut listener, port) = self
            .session
            .channel_forward_listen(self.server_port, Some("127.0.0.1"), None)
            .await?;

        info!("Server Port {}", port);

        // FIXME: server accepts connection one by one
        loop {
            match listener.accept().await {
                Ok(channel) => {
                    info!("Accepted connection");
                    let client_socket =
                        tokio::net::TcpStream::connect(&format!("127.0.0.1:{}", self.client_port))
                            .await?;
                    tokio::spawn(async {
                        crate::utils::copy_duplex(channel, client_socket)
                            .await
                            .unwrap()
                    });
                    info!("Closed connection");
                }
                Err(err) => {
                    error!("accept failed, error: {:?}", err);
                }
            }
        }
    }
}
