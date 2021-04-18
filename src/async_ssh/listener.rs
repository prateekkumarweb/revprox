use ssh2::{Listener, Session};
use std::time::Duration;
use tokio::io::{self, unix::AsyncFd};

use super::channel::AsyncChannel;

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
