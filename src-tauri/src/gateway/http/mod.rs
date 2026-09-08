//! HTTP/1 listener with bounded handlers and cancellable WebSocket upgrades.
mod request;
mod upgrade;
use super::{BoundListener, GatewayInner, InflightGuard};
use bytes::Bytes;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full};
use hyper::{body::Incoming, service::service_fn};
use hyper_util::rt::TokioIo;
pub(crate) use request::Request;
use std::{
    convert::Infallible,
    io,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::oneshot;

type Body = UnsyncBoxBody<Bytes, io::Error>;
fn empty_body() -> Body {
    Full::new(Bytes::new())
        .map_err(|never| match never {})
        .boxed_unsync()
}
fn rejection(status: u16) -> hyper::Response<Body> {
    let mut response = hyper::Response::new(empty_body());
    *response.status_mut() = hyper::StatusCode::from_u16(status).unwrap();
    response
}

pub(super) fn serve(
    listener: BoundListener,
    inner: Arc<GatewayInner>,
    client: Arc<reqwest::blocking::Client>,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(48)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(format!("无法启动网关运行时：{error}")));
            return;
        }
    };
    runtime.block_on(async {
        let socket = match listener
            .take_socket()
            .and_then(tokio::net::TcpListener::from_std)
        {
            Ok(socket) => socket,
            Err(error) => {
                let _ = ready.send(Err(format!("无法接管网关监听：{error}")));
                return;
            }
        };
        let stop = listener.stop_signal();
        let connections = Arc::new(AtomicUsize::new(0));
        let _ = ready.send(Ok(()));
        while !stop.load(Ordering::Acquire) && !inner.stopping.load(Ordering::Acquire) {
            let accepted =
                tokio::time::timeout(std::time::Duration::from_millis(100), socket.accept()).await;
            let Ok(Ok((stream, _))) = accepted else {
                continue;
            };
            if connections
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    (count < 48).then_some(count + 1)
                })
                .is_err()
            {
                continue;
            }
            let (inner, client, stop, connections) = (
                inner.clone(),
                client.clone(),
                stop.clone(),
                connections.clone(),
            );
            tokio::spawn(async move {
                let _connection = ConnectionGuard(connections);
                let service = service_fn(move |request| {
                    dispatch(request, inner.clone(), client.clone(), stop.clone())
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .with_upgrades()
                    .await;
            });
        }
        stop.store(true, Ordering::Release);
    });
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
}

struct ConnectionGuard(Arc<AtomicUsize>);
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

async fn dispatch(
    mut incoming: hyper::Request<Incoming>,
    inner: Arc<GatewayInner>,
    client: Arc<reqwest::blocking::Client>,
    stop: Arc<AtomicBool>,
) -> Result<hyper::Response<Body>, Infallible> {
    let websocket = incoming
        .headers()
        .get("upgrade")
        .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let guard = if websocket {
        None
    } else {
        InflightGuard::try_acquire(inner.clone())
    };
    if inner.maintenance.load(Ordering::Acquire)
        || inner.stopping.load(Ordering::Acquire)
        || (!websocket && guard.is_none())
    {
        return Ok(rejection(503));
    }
    let pending = websocket.then(|| hyper::upgrade::on(&mut incoming));
    let (parts, body) = incoming.into_parts();
    let bytes = match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        http_body_util::Limited::new(body, super::server::MAX_REQUEST_BYTES as usize).collect(),
    )
    .await
    {
        Ok(Ok(body)) => body.to_bytes().to_vec(),
        Ok(Err(_)) => return Ok(rejection(413)),
        Err(_) => return Ok(rejection(408)),
    };
    let (sender, receiver) = oneshot::channel();
    let request = Request {
        method: parts
            .method
            .as_str()
            .parse()
            .expect("Hyper validated an ASCII HTTP method"),
        path: parts.uri.to_string(),
        headers: parts
            .headers
            .iter()
            .filter_map(|(key, value)| {
                tiny_http::Header::from_bytes(key.as_str(), value.as_bytes()).ok()
            })
            .collect(),
        body: bytes,
        reply: Some(sender),
        upgrade: pending,
        runtime: tokio::runtime::Handle::current(),
        stop: stop.clone(),
    };
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        if websocket {
            super::server::websocket::handle(request, inner, client, stop);
        } else {
            super::server::handle(request, inner, client);
        }
    });
    Ok(receiver.await.unwrap_or_else(|_| rejection(500)))
}
