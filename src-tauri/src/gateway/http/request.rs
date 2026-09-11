//! Synchronous gateway handlers over Hyper's cancellable network boundary.
use super::{empty_body, Body};
use bytes::Bytes;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use std::io::{self, Read};
use std::sync::{atomic::AtomicBool, Arc};
use tiny_http::{Header, Method, ReadWrite, Response};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

pub(crate) struct Request {
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) headers: Vec<Header>,
    pub(super) body: Vec<u8>,
    pub(super) reply: Option<oneshot::Sender<hyper::Response<Body>>>,
    pub(super) upgrade: Option<hyper::upgrade::OnUpgrade>,
    pub(super) runtime: tokio::runtime::Handle,
    pub(super) stop: Arc<AtomicBool>,
}

impl Request {
    pub(crate) fn method(&self) -> &Method {
        &self.method
    }
    pub(crate) fn url(&self) -> &str {
        &self.path
    }
    pub(crate) fn headers(&self) -> &[Header] {
        &self.headers
    }
    pub(crate) fn take_body(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.body)
    }

    pub(crate) fn stream_response_with_headers(
        mut self,
        status: u16,
        headers: &[Header],
    ) -> io::Result<BodyWriter> {
        let (sender, receiver) = mpsc::channel(2);
        let body = StreamBody::new(ReceiverStream::new(receiver)).boxed_unsync();
        let mut builder = hyper::Response::builder().status(status);
        for header in headers {
            builder = builder.header(header.field.to_string(), header.value.to_string());
        }
        let response = builder.body(body).map_err(|_| closed())?;
        self.reply
            .take()
            .ok_or_else(closed)?
            .send(response)
            .map_err(|_| closed())?;
        Ok(BodyWriter(sender))
    }

    pub(crate) fn respond<R: Read>(mut self, response: Response<R>) -> io::Result<()> {
        let (sender, receiver) = mpsc::channel(2);
        let body = StreamBody::new(ReceiverStream::new(receiver)).boxed_unsync();
        let outgoing = response_head(&response, body)?;
        self.reply
            .take()
            .ok_or_else(closed)?
            .send(outgoing)
            .map_err(|_| closed())?;
        let mut reader = response.into_reader();
        let mut buffer = vec![0; 16 * 1024];
        loop {
            let size = match reader.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(size) => size,
                Err(error) => {
                    let _ =
                        sender.blocking_send(Err(io::Error::new(error.kind(), "上游响应流中断")));
                    return Err(error);
                }
            };
            sender
                .blocking_send(Ok(Frame::data(Bytes::copy_from_slice(&buffer[..size]))))
                .map_err(|_| closed())?;
        }
    }

    pub(crate) fn upgrade<R: Read>(
        mut self,
        protocol: &str,
        response: Response<R>,
    ) -> io::Result<Box<dyn ReadWrite + Send>> {
        self.open_upgrade(protocol, &response)
            .map(|stream| Box::new(stream) as Box<dyn ReadWrite + Send>)
    }

    fn open_upgrade<R: Read>(
        &mut self,
        protocol: &str,
        response: &Response<R>,
    ) -> io::Result<tokio_util::io::SyncIoBridge<super::upgrade::ControlledIo>> {
        let mut outgoing = response_head(response, empty_body())?;
        outgoing.headers_mut().insert(
            hyper::header::CONNECTION,
            hyper::header::HeaderValue::from_static("Upgrade"),
        );
        outgoing.headers_mut().insert(
            hyper::header::UPGRADE,
            protocol.parse().map_err(|_| closed())?,
        );
        self.reply
            .take()
            .ok_or_else(closed)?
            .send(outgoing)
            .map_err(|_| closed())?;
        let pending = self.upgrade.take().ok_or_else(closed)?;
        let stream = self
            .runtime
            .block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(5), pending).await
            })
            .map_err(|_| closed())?
            .map_err(|_| closed())?;
        let io = self
            .runtime
            .block_on(async { super::upgrade::ControlledIo::new(stream, Arc::clone(&self.stop)) });
        Ok(tokio_util::io::SyncIoBridge::new_with_handle(
            io,
            self.runtime.clone(),
        ))
    }
}

pub(crate) struct BodyWriter(mpsc::Sender<Result<Frame<Bytes>, io::Error>>);
impl io::Write for BodyWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .blocking_send(Ok(Frame::data(Bytes::copy_from_slice(bytes))))
            .map_err(|_| closed())?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn response_head<R: Read>(response: &Response<R>, body: Body) -> io::Result<hyper::Response<Body>> {
    let mut builder = hyper::Response::builder().status(response.status_code().0);
    for header in response.headers() {
        builder = builder.header(header.field.as_str().as_str(), header.value.as_str());
    }
    builder
        .body(body)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "响应头无效"))
}

fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "客户端连接已关闭")
}
