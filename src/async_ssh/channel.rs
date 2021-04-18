use futures::ready;
use ssh2::{Channel, Session, Stream};
use std::{
    io::{Read, Write},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::io::{self, unix::AsyncFd, AsyncRead, AsyncWrite, ReadBuf};

pub struct AsyncChannel {
    inner: Channel,
    session: Arc<AsyncFd<Session>>,
}

impl AsyncChannel {
    pub fn new(channel: Channel, session: Arc<AsyncFd<Session>>) -> Self {
        Self {
            inner: channel,
            session,
        }
    }

    fn stream(&self, stream_id: i32) -> AsyncStream {
        AsyncStream::new(self.inner.stream(stream_id), self.session.clone())
    }
}

impl AsyncRead for AsyncChannel {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.stream(0)).poll_read(cx, buf)
    }
}

impl AsyncWrite for AsyncChannel {
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

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        loop {
            let session = self.session.clone();
            let mut guard = ready!(session.poll_write_ready(cx))?;
            match guard.try_io(|_| self.inner.close().map_err(Into::into)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }
}

struct AsyncStream {
    inner: Stream,
    session: Arc<AsyncFd<Session>>,
}

impl AsyncStream {
    fn new(stream: Stream, session: Arc<AsyncFd<Session>>) -> Self {
        Self {
            inner: stream,
            session,
        }
    }
}

impl AsyncRead for AsyncStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        loop {
            let session = self.session.clone();
            let mut guard = ready!(session.poll_read_ready(cx))?;
            let unfilled: &mut [u8] = buf.initialize_unfilled();
            match guard.try_io(|_| self.inner.read(unfilled).map_err(Into::into)) {
                Ok(result) => return Poll::Ready(result.map(|n| buf.advance(n))),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsyncWrite for AsyncStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            let session = self.session.clone();
            let mut guard = ready!(session.poll_write_ready(cx))?;
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
            let session = self.session.clone();
            let mut guard = ready!(session.poll_write_ready(cx))?;
            match guard.try_io(|_| self.inner.flush().map_err(Into::into)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.poll_flush(cx)
    }
}
