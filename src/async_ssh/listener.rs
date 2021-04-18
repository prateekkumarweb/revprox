use ssh2::{Listener, Session};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{self, unix::AsyncFd};

use super::channel::AsyncChannel;

pub struct AsyncListener {
    inner: Listener,
    session: Arc<AsyncFd<Session>>,
}

impl AsyncListener {
    pub fn new(listener: Listener, session: Arc<AsyncFd<Session>>) -> Self {
        Self {
            inner: listener,
            session,
        }
    }

    pub async fn accept(&mut self) -> io::Result<AsyncChannel> {
        let channel = loop {
            let session = self.session.clone();
            let mut guard = session.readable().await?;
            match guard.try_io(|_| self.inner.accept().map_err(Into::into)) {
                Ok(channel) => break channel,
                Err(_would_block) => {}
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }?;

        Ok(AsyncChannel::new(channel, self.session.clone()))
    }
}
