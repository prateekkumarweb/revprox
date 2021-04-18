use futures::{AsyncReadExt, AsyncWriteExt};
use session::AsyncSession;
use std::net::TcpStream;
use tracing::{error, info};

mod channel;
mod listener;
mod session;

pub async fn main() -> anyhow::Result<()> {
    let tcp = TcpStream::connect("10.108.79.149:22")?;
    let mut sess = AsyncSession::new(tcp)?;
    sess.handshake().await?;

    sess.userauth_pubkey_file().await?;

    assert!(sess.authenticated());

    let (mut listener, port) = sess
        .channel_forward_listen(8081, Some("127.0.0.1"), None)
        .await?;

    println!("Port {}", port);

    loop {
        match listener.accept().await {
            Ok(mut channel) => {
                let mut buf = vec![0; 64];
                channel.read(&mut buf).await?;
                info!("channel receive {:?}", std::str::from_utf8(&buf));
                if buf.starts_with(b"GET / HTTP/1.1\r\n") {
                    channel.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await?;
                } else {
                    channel
                        .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                        .await?;
                }
            }
            Err(err) => {
                error!("accept failed, error: {:?}", err);
            }
        }
    }
}
