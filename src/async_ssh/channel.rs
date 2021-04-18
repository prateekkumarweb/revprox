use futures::{ready, AsyncRead, AsyncWrite};
use ssh2::{Channel, Session, Stream};
use std::{
    io::{Read, Write},
    pin::Pin,
    task::Poll,
};
use tokio::io::{self, unix::AsyncFd};

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

    fn stream(&self, stream_id: i32) -> AsyncStream {
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

struct AsyncStream<'a> {
    inner: Stream,
    session: &'a AsyncFd<Session>,
}

impl<'a> AsyncStream<'a> {
    fn new(stream: Stream, session: &'a AsyncFd<Session>) -> Self {
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
