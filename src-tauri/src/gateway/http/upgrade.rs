use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use std::{
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Polling the stop signal also wakes an otherwise idle upgraded connection.
pub(super) struct ControlledIo {
    stream: TokioIo<Upgraded>,
    stop: Arc<AtomicBool>,
    tick: tokio::time::Interval,
}

impl ControlledIo {
    pub(super) fn new(stream: Upgraded, stop: Arc<AtomicBool>) -> Self {
        Self {
            stream: TokioIo::new(stream),
            stop,
            tick: tokio::time::interval(std::time::Duration::from_millis(100)),
        }
    }
    fn cancelled(&mut self, cx: &mut Context<'_>) -> bool {
        while self.tick.poll_tick(cx).is_ready() {}
        self.stop.load(Ordering::Acquire)
    }
}

impl AsyncRead for ControlledIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.cancelled(cx) {
            return Poll::Ready(Err(io::ErrorKind::Interrupted.into()));
        }
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl AsyncWrite for ControlledIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.cancelled(cx) {
            return Poll::Ready(Err(io::ErrorKind::Interrupted.into()));
        }
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
