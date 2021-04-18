use ssh2::Session;
use std::{net::TcpStream, path::Path};
use tokio::io::{self, unix::AsyncFd};

use super::listener::AsyncListener;

pub struct AsyncSession {
    inner: AsyncFd<Session>,
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

    pub async fn handshake(&mut self) -> io::Result<()> {
        try_write_mut(&mut self.inner, |session| session.handshake()).await
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

    pub async fn channel_forward_listen(
        &self,
        remote_port: u16,
        host: Option<&str>,
        queue_maxsize: Option<u32>,
    ) -> io::Result<(AsyncListener<'_>, u16)> {
        let (listener, port) = try_write(&self.inner, |session| {
            session.channel_forward_listen(remote_port, host, queue_maxsize)
        })
        .await?;
        Ok((AsyncListener::new(listener, &self.inner), port))
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

async fn try_write_mut<R>(
    session: &mut AsyncFd<Session>,
    cb: impl Fn(&mut Session) -> Result<R, ssh2::Error>,
) -> io::Result<R> {
    loop {
        let mut guard = session.writable_mut().await?;
        match guard.try_io(|inner| cb(inner.get_mut()).map_err(Into::into)) {
            Ok(result) => return result,
            Err(_would_block) => continue,
        }
    }
}
