//! Bounded upstream response transport; protocol routing remains in the parent module.

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use std::io::{self, Read};
use tokio::sync::{mpsc as tokio_mpsc, watch};

pub(crate) struct UpstreamResponse {
    status: reqwest::StatusCode,
    headers: HeaderMap,
    url: reqwest::Url,
    initial_body_received: bool,
    body: UpstreamBody,
}

impl UpstreamResponse {
    pub(in crate::gateway::server) fn new(
        status: reqwest::StatusCode,
        headers: HeaderMap,
        url: reqwest::Url,
        initial_body_received: bool,
        receiver: tokio_mpsc::Receiver<Result<Bytes, StreamError>>,
        cancellation: watch::Sender<bool>,
    ) -> Self {
        Self {
            status,
            headers,
            url,
            initial_body_received,
            body: UpstreamBody {
                receiver,
                cancellation,
                pending: Bytes::new(),
                offset: 0,
            },
        }
    }
    pub(crate) fn status(&self) -> reqwest::StatusCode {
        self.status
    }
    pub(crate) fn headers(&self) -> &HeaderMap {
        &self.headers
    }
    pub(crate) fn url(&self) -> &reqwest::Url {
        &self.url
    }

    /// Whether the upstream produced any body bytes before the response was
    /// handed to the synchronous gateway. An empty 2xx SSE response is a
    /// transport failure for Claude and must not become a successful empty
    /// stream.
    pub(crate) fn initial_body_received(&self) -> bool {
        self.initial_body_received
    }

    /// Replaces a response body that was buffered for semantic inspection.
    /// The response metadata and cancellation boundary remain unchanged, so
    /// the normal responder can consume the buffered response exactly once.
    pub(crate) fn with_json_body(mut self, body: Vec<u8>) -> Self {
        self.headers.remove("content-encoding");
        self.headers.remove("content-length");
        self.headers
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        self.with_buffered_body(body)
    }

    pub(crate) fn with_buffered_body(self, body: Vec<u8>) -> Self {
        let status = self.status;
        let headers = self.headers.clone();
        let url = self.url.clone();
        let initial_body_received = self.initial_body_received;
        let (sender, receiver) = tokio_mpsc::channel(1);
        let (cancellation, _) = watch::channel(false);
        if !body.is_empty() {
            let _ = sender.try_send(Ok(Bytes::from(body)));
        }
        drop(sender);
        Self::new(
            status,
            headers,
            url,
            initial_body_received,
            receiver,
            cancellation,
        )
    }
}

impl Read for UpstreamResponse {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.body.read(buffer)
    }
}

pub(super) struct UpstreamBody {
    receiver: tokio_mpsc::Receiver<Result<Bytes, StreamError>>,
    cancellation: watch::Sender<bool>,
    pending: Bytes,
    offset: usize,
}

impl Read for UpstreamBody {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.offset < self.pending.len() {
                let count = output.len().min(self.pending.len() - self.offset);
                output[..count].copy_from_slice(&self.pending[self.offset..self.offset + count]);
                self.offset += count;
                return Ok(count);
            }
            match self.receiver.blocking_recv() {
                Some(Ok(bytes)) => {
                    self.pending = bytes;
                    self.offset = 0;
                }
                Some(Err(error)) => return Err(error.into_io()),
                None => return Ok(0),
            }
        }
    }
}

impl Drop for UpstreamBody {
    fn drop(&mut self) {
        let _ = self.cancellation.send(true);
    }
}

#[derive(Debug)]
pub(crate) struct StreamError {
    kind: io::ErrorKind,
    message: String,
}

impl StreamError {
    pub(in crate::gateway::server) fn timeout(message: &str) -> Self {
        Self {
            kind: io::ErrorKind::TimedOut,
            message: message.to_string(),
        }
    }

    pub(in crate::gateway::server) fn network(error: reqwest::Error) -> Self {
        Self {
            kind: io::ErrorKind::ConnectionAborted,
            message: error.to_string(),
        }
    }

    fn into_io(self) -> io::Error {
        io::Error::new(self.kind, self.message)
    }
}
