use futures::{ready, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use ssh2::{Channel, Listener, Session, Stream};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::io::{self, unix::AsyncFd};
use tracing::{error, info};

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

    async fn try_write<R>(&self, cb: impl Fn(&Session) -> Result<R, ssh2::Error>) -> io::Result<R> {
        loop {
            let mut guard = self.inner.writable().await?;
            match guard.try_io(|inner| cb(inner.get_ref()).map_err(Into::into)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    async fn try_write_mut<R>(
        &mut self,
        cb: impl Fn(&mut Session) -> Result<R, ssh2::Error>,
    ) -> io::Result<R> {
        loop {
            let mut guard = self.inner.writable_mut().await?;
            match guard.try_io(|inner| cb(inner.get_mut()).map_err(Into::into)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn handshake(&mut self) -> io::Result<()> {
        self.try_write_mut(|session| session.handshake()).await
    }

    pub async fn userauth_pubkey_file(&self) -> io::Result<()> {
        self.try_write(|session| {
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
        let (listener, port) = self
            .try_write(|session| session.channel_forward_listen(remote_port, host, queue_maxsize))
            .await?;
        Ok((AsyncListener::new(listener, &self.inner), port))
    }
}

pub struct AsyncListener<'a> {
    inner: Listener,
    session: &'a AsyncFd<Session>,
}

impl<'a> AsyncListener<'a> {
    pub fn new(listener: Listener, session: &'a AsyncFd<Session>) -> Self {
        Self {
            inner: listener,
            session,
        }
    }

    pub async fn accept(&mut self) -> io::Result<AsyncChannel<'_>> {
        let channel = loop {
            let mut guard = self.session.readable().await?;
            match guard.try_io(|_| self.inner.accept().map_err(Into::into)) {
                Ok(channel) => break channel,
                Err(_would_block) => {}
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }?;

        Ok(AsyncChannel::new(channel, self.session))
    }
}

pub struct AsyncChannel<'a> {
    inner: Channel,
    session: &'a AsyncFd<Session>,
}

impl<'a> AsyncChannel<'a> {
    pub fn new(channel: Channel, session: &'a AsyncFd<Session>) -> Self {
        Self {
            inner: channel,
            session,
        }
    }

    pub fn stream(&self, stream_id: i32) -> AsyncStream {
        AsyncStream::new(self.inner.stream(stream_id), self.session)
    }
}

impl AsyncRead for AsyncChannel<'_> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.stream(0)).poll_read(cx, buf)
    }
}

impl AsyncWrite for AsyncChannel<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream(0)).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream(0)).poll_flush(cx)
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.session.poll_write_ready(cx))?;
            match guard.try_io(|_| self.inner.close().map_err(Into::into)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }
}

pub struct AsyncStream<'a> {
    inner: Stream,
    session: &'a AsyncFd<Session>,
}

impl<'a> AsyncStream<'a> {
    pub fn new(stream: Stream, session: &'a AsyncFd<Session>) -> Self {
        Self {
            inner: stream,
            session,
        }
    }
}

impl AsyncRead for AsyncStream<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let mut guard = ready!(self.session.poll_read_ready(cx))?;
            match guard.try_io(|_| self.inner.read(buf).map_err(Into::into)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsyncWrite for AsyncStream<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let mut guard = ready!(self.session.poll_write_ready(cx))?;
            match guard.try_io(|_| self.inner.write(buf).map_err(Into::into)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.session.poll_write_ready(cx))?;
            match guard.try_io(|_| self.inner.flush().map_err(Into::into)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

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
