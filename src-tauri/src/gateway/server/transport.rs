//! Async upstream transport behind the synchronous protocol conversion layer.

use super::route::{upstream_headers, UpstreamClient, UpstreamResponse};
use super::ActiveRoute;
use crate::provider_diagnostics::{network_diagnostic, ProviderDiagnostic, ProviderFailureKind};
use bytes::Bytes;
use std::sync::mpsc;
use std::time::Duration;
use tiny_http::Header;
use tokio::sync::{mpsc as tokio_mpsc, watch};

#[derive(Clone, Copy)]
struct Timeouts {
    headers: Duration,
    first_byte: Duration,
    idle: Duration,
    total: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            headers: Duration::from_secs(20),
            first_byte: Duration::from_secs(30),
            idle: Duration::from_secs(60),
            total: Duration::from_secs(600),
        }
    }
}

pub(crate) fn send_upstream_request(
    client: &UpstreamClient,
    route: &ActiveRoute,
    url: &str,
    method: reqwest::Method,
    body: Vec<u8>,
    incoming: Option<&[Header]>,
    anthropic_version: Option<&str>,
    anthropic_beta: Option<&str>,
) -> Result<UpstreamResponse, ProviderDiagnostic> {
    let request = client
        .client
        .request(method, url)
        .headers(upstream_headers(
            route,
            anthropic_version,
            anthropic_beta,
            incoming,
        ))
        .body(body);
    let timeouts = Timeouts::default();
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let endpoint = url.to_string();
    client.runtime.spawn(async move {
        produce_response(request, endpoint, ready_sender, timeouts).await;
    });
    ready_receiver
        .recv_timeout(timeouts.headers + timeouts.first_byte)
        .map_err(|_| timeout_diagnostic(url, "上游服务未在首包期限内响应"))?
}

async fn produce_response(
    request: reqwest::RequestBuilder,
    endpoint: String,
    ready_sender: mpsc::SyncSender<Result<UpstreamResponse, ProviderDiagnostic>>,
    timeouts: Timeouts,
) {
    let response = match tokio::time::timeout(timeouts.headers, request.send()).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => return send_failure(ready_sender, network_diagnostic(&endpoint, &error)),
        Err(_) => {
            return send_failure(
                ready_sender,
                timeout_diagnostic(&endpoint, "上游服务响应头超时"),
            )
        }
    };
    let status = response.status();
    let headers = response.headers().clone();
    let url = response.url().clone();
    let (body_sender, body_receiver) = tokio_mpsc::channel(2);
    let (cancellation, mut cancellation_receiver) = watch::channel(false);
    let mut response = response;
    let deadline = tokio::time::Instant::now() + timeouts.total;
    let mut first_cancellation = cancellation_receiver.clone();
    let first = match next_chunk(
        &mut response,
        &mut first_cancellation,
        deadline,
        timeouts.first_byte,
    )
    .await
    {
        ChunkResult::Data(bytes) => Some(bytes),
        ChunkResult::End => None,
        ChunkResult::Cancelled => return,
        ChunkResult::Timeout => {
            return send_failure(
                ready_sender,
                timeout_diagnostic(&endpoint, "上游服务首包超时"),
            )
        }
        ChunkResult::Network(error) => {
            return send_failure(ready_sender, network_diagnostic(&endpoint, &error))
        }
    };
    let upstream = UpstreamResponse::new(status, headers, url, body_receiver, cancellation);
    if ready_sender.send(Ok(upstream)).is_err() {
        return;
    }
    if let Some(first) = first {
        if !send_chunk(&body_sender, first, &mut cancellation_receiver).await {
            return;
        }
    }
    relay_chunks(
        response,
        body_sender,
        cancellation_receiver,
        deadline,
        timeouts.idle,
    )
    .await;
}

fn send_failure(
    sender: mpsc::SyncSender<Result<UpstreamResponse, ProviderDiagnostic>>,
    failure: ProviderDiagnostic,
) {
    let _ = sender.send(Err(failure));
}

async fn relay_chunks(
    mut response: reqwest::Response,
    sender: tokio_mpsc::Sender<Result<Bytes, super::route::StreamError>>,
    mut cancellation: watch::Receiver<bool>,
    deadline: tokio::time::Instant,
    idle_timeout: Duration,
) {
    loop {
        match next_chunk(&mut response, &mut cancellation, deadline, idle_timeout).await {
            ChunkResult::Data(bytes) => {
                if !send_chunk(&sender, bytes, &mut cancellation).await {
                    return;
                }
            }
            ChunkResult::End | ChunkResult::Cancelled => return,
            ChunkResult::Timeout => {
                let _ = sender
                    .send(Err(super::route::StreamError::timeout(
                        "上游响应流空闲或总期限超时",
                    )))
                    .await;
                return;
            }
            ChunkResult::Network(error) => {
                let _ = sender
                    .send(Err(super::route::StreamError::network(error)))
                    .await;
                return;
            }
        }
    }
}

enum ChunkResult {
    Data(Bytes),
    End,
    Cancelled,
    Timeout,
    Network(reqwest::Error),
}

async fn next_chunk(
    response: &mut reqwest::Response,
    cancellation: &mut watch::Receiver<bool>,
    deadline: tokio::time::Instant,
    idle_timeout: Duration,
) -> ChunkResult {
    let timeout = deadline
        .saturating_duration_since(tokio::time::Instant::now())
        .min(idle_timeout);
    if timeout.is_zero() {
        return ChunkResult::Timeout;
    }
    tokio::select! {
        result = tokio::time::timeout(timeout, response.chunk()) => match result {
            Ok(Ok(Some(bytes))) => ChunkResult::Data(bytes), Ok(Ok(None)) => ChunkResult::End,
            Ok(Err(error)) => ChunkResult::Network(error), Err(_) => ChunkResult::Timeout,
        },
        _ = cancellation.changed() => ChunkResult::Cancelled,
    }
}

async fn send_chunk(
    sender: &tokio_mpsc::Sender<Result<Bytes, super::route::StreamError>>,
    bytes: Bytes,
    cancellation: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        result = sender.send(Ok(bytes)) => result.is_ok(),
        _ = cancellation.changed() => false,
    }
}

fn timeout_diagnostic(endpoint: &str, message: &str) -> ProviderDiagnostic {
    ProviderDiagnostic::new(ProviderFailureKind::Timeout, endpoint, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn short_timeouts() -> Timeouts {
        Timeouts {
            headers: Duration::from_millis(200),
            first_byte: Duration::from_millis(80),
            idle: Duration::from_millis(80),
            total: Duration::from_secs(1),
        }
    }

    fn start_response(
        request: reqwest::RequestBuilder,
        timeouts: Timeouts,
    ) -> (
        tokio::runtime::Runtime,
        mpsc::Receiver<Result<UpstreamResponse, ProviderDiagnostic>>,
    ) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        runtime.spawn(produce_response(
            request,
            "http://fixture".to_string(),
            sender,
            timeouts,
        ));
        (runtime, receiver)
    }

    fn accept_and_write(bytes: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0_u8; 2048]);
            stream.write_all(bytes).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(250));
        });
        (endpoint, worker)
    }

    fn accept_first_chunk_and_wait_for_close() -> (String, thread::JoinHandle<bool>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0_u8; 2048]);
            stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nd\r\ndata: first\n\n\r\n").unwrap();
            stream.flush().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            matches!(stream.read(&mut [0_u8; 1]), Ok(0))
        });
        (endpoint, worker)
    }

    #[test]
    fn headers_without_a_body_fail_the_first_byte_deadline() {
        let (endpoint, worker) = accept_and_write(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 9\r\n\r\n",
        );
        let (runtime, receiver) = start_response(
            reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(endpoint),
            short_timeouts(),
        );
        let error = match receiver.recv_timeout(Duration::from_secs(1)).unwrap() {
            Ok(_) => panic!("response without a body should time out"),
            Err(error) => error,
        };
        assert_eq!(error.kind, ProviderFailureKind::Timeout);
        worker.join().unwrap();
        drop(runtime);
    }

    #[test]
    fn stalled_stream_reports_an_idle_timeout_to_the_reader() {
        let (endpoint, worker) = accept_and_write(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\nd\r\ndata: first\n\n\r\n",
        );
        let (runtime, receiver) = start_response(
            reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(endpoint),
            short_timeouts(),
        );
        let mut response = receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        let mut first = [0_u8; 16];
        assert!(response.read(&mut first).unwrap() > 0);
        let error = response.read(&mut first).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        worker.join().unwrap();
        drop(runtime);
    }

    #[test]
    fn dropping_the_downstream_reader_closes_the_upstream_socket() {
        let (endpoint, worker) = accept_first_chunk_and_wait_for_close();
        let (runtime, receiver) = start_response(
            reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(endpoint),
            short_timeouts(),
        );
        drop(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
        );
        assert!(worker.join().unwrap());
        drop(runtime);
    }
}
