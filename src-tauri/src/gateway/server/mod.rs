//! Loopback HTTP boundary. It authenticates a client capability before any
//! body is forwarded, keeps upstream credentials local, and emits only
//! gateway-owned diagnostics.

mod respond;
mod route;
mod websocket;

use super::metrics::RequestSpan;
use super::transform::{convert_request, ConvertedRequest, ReasoningTransport};
use super::{constant_time_equal, ActiveRoute, BoundListener, GatewayInner};
use asb_core::contracts::{AppKind, UpstreamProtocol};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
#[cfg(test)]
use tiny_http::Server;
use tiny_http::{Method, Request};

use route::{json_content_type, request_header};

pub(super) use respond::{read_limited, respond_error, respond_upstream, ReadLimitError};
pub(super) use route::{client_protocol, client_token, upstream_headers, upstream_url};

pub(super) const MAX_REQUEST_BYTES: u64 = 8 * 1024 * 1024;
pub(super) const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CONCURRENT_REQUESTS: usize = 8;

/// How long a request may wait for a worker to become idle before the busy
/// answer. Long enough to absorb thread-park timing, far below any client
/// timeout that matters.
const DISPATCH_RETRY_WINDOW: Duration = Duration::from_millis(250);

/// The client behind a served path. Chat Completions is upstream-only here,
/// so it has no client and its traffic is not counted as provider traffic.
fn span_app(protocol: UpstreamProtocol) -> Option<AppKind> {
    match protocol {
        UpstreamProtocol::Responses => Some(AppKind::Codex),
        UpstreamProtocol::AnthropicMessages => Some(AppKind::Claude),
        UpstreamProtocol::ChatCompletions => None,
    }
}

pub(super) fn serve(
    listener: BoundListener,
    inner: Arc<GatewayInner>,
    ready: SyncSender<Result<(), String>>,
) {
    let server = listener.server();
    let stop = listener.stop_signal();
    let client = match Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()
    {
        Ok(client) => Arc::new(client),
        Err(_) => {
            listener.stop();
            let _ = ready.send(Err("无法初始化本机协议网关 HTTP 客户端".to_string()));
            return;
        }
    };
    let (sender, workers) = match start_workers(&inner, &client, &stop) {
        Ok(workers) => workers,
        Err(()) => {
            listener.stop();
            let _ = ready.send(Err("无法启动本机协议网关请求工作线程".to_string()));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    while !inner.stopping.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
        let request = match server.recv() {
            Ok(request) => request,
            Err(_) if inner.stopping.load(Ordering::Acquire) || stop.load(Ordering::Acquire) => {
                break;
            }
            Err(_) => continue,
        };
        // A port change drains through this gate: new work is refused while
        // the accepted work drains to zero, so the swap never cuts a live
        // request short.
        if inner.maintenance.load(Ordering::Acquire) {
            let protocol = client_protocol(request.url());
            respond_error(
                request,
                protocol,
                503,
                "本机协议网关正在修改监听端口，请稍后重试",
            );
            continue;
        }
        dispatch(request, &inner, &sender, &stop);
    }
    drop(sender);
    for worker in workers {
        let _ = worker.join();
    }
}

/// Starts fixed rendezvous workers before the listener is announced ready.
fn start_workers(
    inner: &Arc<GatewayInner>,
    client: &Arc<Client>,
    stop: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<(mpsc::SyncSender<Request>, Vec<thread::JoinHandle<()>>), ()> {
    // A rendezvous channel accepts only while a worker is idle, keeping the
    // actual in-flight count at the declared bound without a buffered batch.
    let (sender, receiver) = mpsc::sync_channel(0);
    let receiver = Arc::new(Mutex::new(receiver));
    let mut workers = Vec::with_capacity(MAX_CONCURRENT_REQUESTS);
    for index in 0..MAX_CONCURRENT_REQUESTS {
        let receiver = Arc::clone(&receiver);
        let inner = Arc::clone(inner);
        let client = Arc::clone(client);
        let stop = Arc::clone(stop);
        let worker = thread::Builder::new()
            .name(format!("asb-gateway-worker-{index}"))
            .spawn(move || loop {
                let request = match receiver.lock() {
                    Ok(receiver) => receiver.recv(),
                    Err(_) => return,
                };
                match request {
                    Ok(request) if !stop.load(Ordering::Acquire) => {
                        handle(request, Arc::clone(&inner), Arc::clone(&client))
                    }
                    Ok(_) | Err(_) => return,
                }
            })
            .map_err(|_| ())?;
        workers.push(worker);
    }
    Ok((sender, workers))
}

/// Hands one request to an idle worker. The rendezvous channel only accepts
/// a request while a worker is parked in `recv`, so a request can arrive in
/// the gap before the first worker parks or while workers swap between
/// requests. A short retry window keeps those timing gaps from turning into
/// spurious 503 answers; a genuinely saturated gateway still answers 503
/// once the window closes.
fn dispatch(
    request: Request,
    inner: &Arc<GatewayInner>,
    sender: &mpsc::SyncSender<Request>,
    stop: &Arc<std::sync::atomic::AtomicBool>,
) {
    inner.inflight.fetch_add(1, Ordering::AcqRel);
    let deadline = std::time::Instant::now() + DISPATCH_RETRY_WINDOW;
    let mut request = request;
    loop {
        match sender.try_send(request) {
            Ok(()) => return,
            Err(TrySendError::Full(returned)) => {
                request = returned;
                if std::time::Instant::now() >= deadline
                    || inner.stopping.load(Ordering::Acquire)
                    || stop.load(Ordering::Acquire)
                    || inner.maintenance.load(Ordering::Acquire)
                {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(TrySendError::Disconnected(_)) => {
                inner.inflight.fetch_sub(1, Ordering::AcqRel);
                return;
            }
        }
    }
    inner.inflight.fetch_sub(1, Ordering::AcqRel);
    let protocol = client_protocol(request.url());
    if let Some(protocol) = protocol {
        if let Some(app) = span_app(protocol) {
            RequestSpan::start(Arc::clone(&inner.metrics), app, protocol).finish(Some(503), 0);
        }
    }
    respond_error(
        request,
        protocol,
        503,
        "本机协议网关当前请求过多，请稍后重试",
    );
}

fn handle(request: Request, inner: Arc<GatewayInner>, client: Arc<Client>) {
    let _inflight = super::InflightGuard(Arc::clone(&inner));
    if websocket::is_upgrade_request(&request) {
        websocket::handle(request, inner, client);
        return;
    }
    let target_protocol = client_protocol(request.url());
    if request.method() != &Method::Post {
        respond_error(
            request,
            target_protocol,
            405,
            "本机协议网关仅接受 POST 请求",
        );
        return;
    }
    let Some(protocol) = target_protocol else {
        respond_error(request, None, 404, "本机协议网关不处理该路径");
        return;
    };
    // Provider traffic starts at a path naming a client the gateway serves;
    // every rejection from here on is recorded with the span.
    let Some(app) = span_app(protocol) else {
        respond_error(
            request,
            Some(protocol),
            404,
            "本机协议网关不接收 Chat Completions 客户端请求",
        );
        return;
    };
    let mut span = RequestSpan::start(Arc::clone(&inner.metrics), app, protocol);
    if !json_content_type(&request) {
        span.finish(Some(415), 0);
        respond_error(
            request,
            Some(protocol),
            415,
            "请求必须使用 application/json",
        );
        return;
    }
    let Some(token) = client_token(&request) else {
        span.finish(Some(401), 0);
        respond_error(request, Some(protocol), 401, "本机协议网关凭据无效");
        return;
    };
    let route = match inner.routes.read() {
        Ok(routes) => routes
            .get(&app)
            .filter(|route| constant_time_equal(route.client_token.as_bytes(), token.as_bytes()))
            .cloned(),
        Err(_) => None,
    };
    let Some(route) = route else {
        span.finish(Some(401), 0);
        respond_error(request, Some(protocol), 401, "本机协议网关凭据无效");
        return;
    };
    span.bind_route(&route.profile_id, route.upstream_protocol);
    forward_request(request, span, protocol, route, inner, client);
}

fn forward_request(
    mut request: Request,
    mut span: RequestSpan,
    protocol: UpstreamProtocol,
    route: ActiveRoute,
    inner: Arc<GatewayInner>,
    client: Arc<Client>,
) {
    let body = match read_limited(request.as_reader(), MAX_REQUEST_BYTES) {
        Ok(body) => body,
        Err(ReadLimitError::TooLarge) => {
            span.finish(Some(413), 0);
            respond_error(request, Some(protocol), 413, "请求体超过本机协议网关限制");
            return;
        }
        Err(ReadLimitError::Io) => {
            span.finish(Some(400), 0);
            respond_error(request, Some(protocol), 400, "无法读取请求体");
            return;
        }
    };
    span.note_request_bytes(body.len() as u64);
    let reasoning_transport = ReasoningTransport::from_client_token(&route.client_token);
    let converted = match convert_request(
        protocol,
        route.upstream_protocol,
        &body,
        route.max_output_tokens,
        Some(&reasoning_transport),
    ) {
        Ok(converted) => converted,
        Err(error) => {
            span.finish(Some(422), 0);
            respond_error(
                request,
                Some(protocol),
                422,
                &format!("无法转换请求：{error}"),
            );
            return;
        }
    };
    if converted.body.len() as u64 > MAX_REQUEST_BYTES {
        span.finish(Some(413), 0);
        respond_error(
            request,
            Some(protocol),
            413,
            "转换后的请求体超过本机协议网关限制",
        );
        return;
    }
    send_upstream(request, span, protocol, route, inner, client, converted);
}

fn send_upstream(
    request: Request,
    span: RequestSpan,
    protocol: UpstreamProtocol,
    route: ActiveRoute,
    inner: Arc<GatewayInner>,
    client: Arc<Client>,
    converted: ConvertedRequest,
) {
    let url = match upstream_url(&route, &inner.configured_base_url()) {
        Ok(url) => url,
        Err(message) => {
            span.finish(Some(502), 0);
            respond_error(request, Some(protocol), 502, &message);
            return;
        }
    };
    let headers = upstream_headers(
        &route,
        request_header(&request, "anthropic-version").as_deref(),
        request_header(&request, "anthropic-beta").as_deref(),
    );
    let upstream = match client
        .post(url)
        .headers(headers)
        .body(converted.body)
        .send()
    {
        Ok(response) => response,
        Err(_) => {
            span.finish(Some(502), 0);
            respond_error(request, Some(protocol), 502, "无法连接上游服务");
            return;
        }
    };
    respond_upstream(
        request,
        span,
        protocol,
        &route,
        route.upstream_protocol,
        converted.stream,
        upstream,
    );
}

#[cfg(test)]
mod live_nvidia_tests;
#[cfg(test)]
mod tests;
