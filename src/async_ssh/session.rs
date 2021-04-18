use ssh2::Session;
use std::{net::TcpStream, path::Path, sync::Arc};
use tokio::io::{self, unix::AsyncFd};

use super::listener::AsyncListener;

pub struct AsyncSession {
    inner: AsyncFd<Session>,
}

pub struct HandshakenAsyncSession {
    inner: Arc<AsyncFd<Session>>,
}

impl AsyncSession {
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        let mut session = {
            let session = Session::new()?;
            session.set_blocking(false);
            session
        };

        session.set_tcp_stream(stream);

        Ok(Self {
            inner: AsyncFd::new(session)?,
        })
    }

    pub async fn handshake(mut self) -> io::Result<HandshakenAsyncSession> {
        loop {
            let mut guard = self.inner.writable_mut().await?;
            match guard.try_io(|inner| inner.get_mut().handshake().map_err(Into::into)) {
                Ok(result) => {
                    return result.map(|_| HandshakenAsyncSession {
                        inner: Arc::new(self.inner),
                    })
                }
                Err(_would_block) => continue,
            }
        }
    }
}

impl HandshakenAsyncSession {
    pub async fn channel_forward_listen(
        &self,
        remote_port: u16,
        host: Option<&str>,
        queue_maxsize: Option<u32>,
    ) -> io::Result<(AsyncListener, u16)> {
        let (listener, port) = try_write(&self.inner, |session| {
            session.channel_forward_listen(remote_port, host, queue_maxsize)
        })
        .await?;
        Ok((AsyncListener::new(listener, self.inner.clone()), port))
    }

    pub async fn userauth_pubkey_file(&self) -> io::Result<()> {
        try_write(&self.inner, |session| {
            session.userauth_pubkey_file(
                "ubuntu",
                None,
                Path::new("/var/snap/multipass/common/data/multipassd/ssh-keys/id_rsa"),
                None,
            )
        })
        .await
    }

    pub fn authenticated(&self) -> bool {
        self.inner.get_ref().authenticated()
    }
}

async fn try_write<R>(
    session: &AsyncFd<Session>,
    cb: impl Fn(&Session) -> Result<R, ssh2::Error>,
) -> io::Result<R> {
    loop {
        let mut guard = session.writable().await?;
        match guard.try_io(|inner| cb(inner.get_ref()).map_err(Into::into)) {
            Ok(result) => return result,
            Err(_would_block) => continue,
        }
    }
}
