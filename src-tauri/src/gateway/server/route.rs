use super::super::ActiveRoute;
use crate::gateway::http::Request;
use asb_core::contracts::UpstreamProtocol;
use bytes::Bytes;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_TYPE,
};
use std::io::{self, Read};
use std::time::Duration;
use tiny_http::{Header, Method};
use tokio::sync::{mpsc as tokio_mpsc, watch};

const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct UpstreamClient {
    pub(super) client: reqwest::Client,
    pub(super) runtime: tokio::runtime::Handle,
}

impl UpstreamClient {
    pub(crate) fn new(runtime: tokio::runtime::Handle) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
                .build()?,
            runtime,
        })
    }
}

pub(crate) struct UpstreamResponse {
    status: reqwest::StatusCode,
    headers: HeaderMap,
    url: reqwest::Url,
    body: UpstreamBody,
}

impl UpstreamResponse {
    pub(super) fn new(
        status: reqwest::StatusCode,
        headers: HeaderMap,
        url: reqwest::Url,
        receiver: tokio_mpsc::Receiver<Result<Bytes, StreamError>>,
        cancellation: watch::Sender<bool>,
    ) -> Self {
        Self {
            status,
            headers,
            url,
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
pub(super) struct StreamError {
    kind: io::ErrorKind,
    message: String,
}

impl StreamError {
    pub(super) fn timeout(message: &str) -> Self {
        Self {
            kind: io::ErrorKind::TimedOut,
            message: message.to_string(),
        }
    }

    pub(super) fn network(error: reqwest::Error) -> Self {
        Self {
            kind: io::ErrorKind::ConnectionAborted,
            message: error.to_string(),
        }
    }

    fn into_io(self) -> io::Error {
        io::Error::new(self.kind, self.message)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CodexOperation {
    Responses,
    Compact,
    Models,
    ChatCompletions,
    AlphaSearch,
    ImageGeneration,
    ImageEdit,
}

impl CodexOperation {
    pub(crate) fn method(self) -> Method {
        match self {
            Self::Models => Method::Get,
            Self::Responses
            | Self::Compact
            | Self::ChatCompletions
            | Self::AlphaSearch
            | Self::ImageGeneration
            | Self::ImageEdit => Method::Post,
        }
    }

    pub(crate) fn upstream_path(self) -> &'static str {
        match self {
            Self::Responses => "/responses",
            Self::Compact => "/responses/compact",
            Self::Models => "/models",
            Self::ChatCompletions => "/chat/completions",
            Self::AlphaSearch => "/alpha/search",
            Self::ImageGeneration => "/images/generations",
            Self::ImageEdit => "/images/edits",
        }
    }

    pub(crate) fn is_responses(self) -> bool {
        self == Self::Responses
    }

    pub(crate) fn is_compact(self) -> bool {
        self == Self::Compact
    }

    pub(crate) fn requires_json(self) -> bool {
        self != Self::Models
    }
}

pub(crate) struct CodexRequestPath<'a> {
    pub(crate) capability: &'a str,
    pub(crate) operation: CodexOperation,
}

pub(super) fn upstream_headers(
    route: &ActiveRoute,
    anthropic_version: Option<&str>,
    anthropic_beta: Option<&str>,
    incoming: Option<&[Header]>,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    // Conversion and diagnostics consume response bytes locally. Asking for
    // identity avoids an avoidable decoder in compliant upstreams; the shared
    // response decoder still handles providers that send compression anyway.
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    match route.upstream_protocol.authentication_scheme() {
        asb_core::AuthenticationScheme::Bearer => {
            if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", route.api_key)) {
                headers.insert(AUTHORIZATION, value);
            }
        }
        asb_core::AuthenticationScheme::XApiKey => {
            if let Ok(value) = HeaderValue::from_str(&route.api_key) {
                headers.insert(HeaderName::from_static("x-api-key"), value);
            }
        }
    }
    if route.upstream_protocol == UpstreamProtocol::AnthropicMessages {
        let version = anthropic_version.unwrap_or("2023-06-01");
        if let Ok(value) = HeaderValue::from_str(version) {
            headers.insert(HeaderName::from_static("anthropic-version"), value);
        }
        if let Some(beta) = anthropic_beta.and_then(|value| HeaderValue::from_str(value).ok()) {
            headers.insert(HeaderName::from_static("anthropic-beta"), beta);
        }
    }
    if route.app == asb_core::contracts::AppKind::Codex
        && route.upstream_protocol == UpstreamProtocol::Responses
    {
        if let Some(incoming) = incoming {
            append_native_responses_headers(&mut headers, incoming);
        }
    }
    headers
}

/// The only HTTP exit to a third-party Codex upstream. It owns regenerated
/// authentication and filtered protocol headers; callers retain only their
/// request conversion and response presentation responsibilities.
/// Native Responses vendors may require Codex beta, client-version, or
/// session headers. The gateway preserves them only after replacing every
/// credential, connection, routing, and tracing header with local state.
fn append_native_responses_headers(target: &mut HeaderMap, incoming: &[Header]) {
    for header in incoming {
        let name = header.field.as_str().as_str();
        if !is_native_responses_header(name) {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_bytes(header.value.as_bytes()) else {
            continue;
        };
        target.append(name, value);
    }
}

fn is_native_responses_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "openai-beta"
            | "originator"
            | "session_id"
            | "thread_id"
            | "turn_id"
            | "user-agent"
            | "x-client-request-id"
            | "x-codex-beta-features"
            | "x-codex-installation-id"
            | "x-codex-parent-thread-id"
            | "x-codex-routing-hint"
            | "x-codex-turn-metadata"
            | "x-codex-turn-state"
            | "x-codex-window-id"
            | "x-openai-internal-codex-responses-lite"
            | "x-openai-subagent"
            | "x-responsesapi-include-timing-metrics"
            | "x-stainless-arch"
            | "x-stainless-lang"
            | "x-stainless-os"
            | "x-stainless-package-version"
            | "x-stainless-retry-count"
            | "x-stainless-runtime"
            | "x-stainless-runtime-version"
            | "x-stainless-timeout"
    )
}

pub(crate) fn upstream_url(
    route: &ActiveRoute,
    gateway_base: &str,
    request_url: &str,
) -> Result<String, String> {
    let url =
        asb_core::endpoint::upstream_endpoint(&route.upstream_base_url, route.upstream_protocol)?;
    with_request_query(url, gateway_base, request_url)
}

pub(crate) fn upstream_compact_url(
    route: &ActiveRoute,
    gateway_base: &str,
    request_url: &str,
) -> Result<String, String> {
    if route.upstream_protocol != UpstreamProtocol::Responses {
        return Err("当前第三方上游不支持原生 Responses compact".to_string());
    }
    let url = asb_core::endpoint::compact_endpoint(&route.upstream_base_url)?;
    with_request_query(url, gateway_base, request_url)
}

pub(crate) fn upstream_codex_operation_url(
    route: &ActiveRoute,
    gateway_base: &str,
    operation: CodexOperation,
    request_url: &str,
) -> Result<String, String> {
    if route.upstream_protocol == UpstreamProtocol::AnthropicMessages {
        return Err("所选第三方档案不支持此 Codex 操作".to_string());
    }
    let url = reqwest::Url::parse(&asb_core::endpoint::codex_endpoint(
        &route.upstream_base_url,
        operation.upstream_path(),
    )?)
    .map_err(|_| "供应商上游请求地址无效".to_string())?;
    with_request_query(url.to_string(), gateway_base, request_url)
}

fn validate_upstream_url(url: &str, gateway_base: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(&url).map_err(|_| "供应商上游请求地址无效".to_string())?;
    let gateway =
        reqwest::Url::parse(gateway_base).map_err(|_| "本机协议网关地址无效".to_string())?;
    if parsed.origin() == gateway.origin() {
        return Err("供应商服务地址不能指向本机协议网关自身".to_string());
    }
    Ok(url.to_string())
}

/// Copies only the request target's query onto a fixed, already-resolved
/// upstream endpoint. The path and authority always come from the route's
/// validated endpoint; an empty component, fragment, or control character is
/// rejected instead of being interpreted as a second forwarding target.
fn with_request_query(
    url: String,
    gateway_base: &str,
    request_url: &str,
) -> Result<String, String> {
    let mut parsed = reqwest::Url::parse(&url).map_err(|_| "供应商上游请求地址无效".to_string())?;
    let Some((_, query)) = request_url.split_once('?') else {
        return validate_upstream_url(parsed.as_str(), gateway_base);
    };
    if query.is_empty()
        || query.contains('#')
        || query.chars().any(char::is_control)
        || query
            .split('&')
            .any(|part| part.split('=').next().is_none_or(str::is_empty))
    {
        return Err("Codex 操作查询参数无效".to_string());
    }
    parsed.set_query(Some(query));
    validate_upstream_url(parsed.as_str(), gateway_base)
}

pub(crate) fn client_protocol(path: &str) -> Option<UpstreamProtocol> {
    let path = path.split('?').next().unwrap_or(path);
    if codex_request(path).is_some() {
        return Some(UpstreamProtocol::Responses);
    }
    match path {
        "/v1/messages" | "/messages" => Some(UpstreamProtocol::AnthropicMessages),
        _ => None,
    }
}

pub(crate) fn codex_request(path: &str) -> Option<CodexRequestPath<'_>> {
    let path = path.split('?').next()?;
    let (token, operation) = path.strip_prefix("/codex/")?.split_once('/')?;
    let secret = token.strip_prefix("asb_codex_")?;
    if secret.len() != 64 || !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let operation = match operation {
        "v1/responses" => CodexOperation::Responses,
        "v1/responses/compact" => CodexOperation::Compact,
        "v1/models" => CodexOperation::Models,
        "v1/chat/completions" => CodexOperation::ChatCompletions,
        "v1/alpha/search" => CodexOperation::AlphaSearch,
        "v1/images/generations" => CodexOperation::ImageGeneration,
        "v1/images/edits" => CodexOperation::ImageEdit,
        _ => return None,
    };
    Some(CodexRequestPath {
        capability: token,
        operation,
    })
}

pub(crate) fn request_capability(request: &Request, protocol: UpstreamProtocol) -> Option<String> {
    if protocol == UpstreamProtocol::Responses {
        return codex_request(request.url()).map(|path| path.capability.to_string());
    }
    let mut token = None;
    for header in request.headers() {
        let candidate = if header.field.equiv("Authorization") {
            header
                .value
                .as_str()
                .strip_prefix("Bearer ")
                .or_else(|| header.value.as_str().strip_prefix("bearer "))
        } else {
            None
        };
        if let Some(candidate) = candidate {
            if candidate.is_empty() || token.replace(candidate.to_string()).is_some() {
                return None;
            }
        }
    }
    token
}

pub(super) fn request_header(request: &Request, name: &'static str) -> Option<String> {
    let mut value = None;
    for header in request.headers() {
        if header.field.equiv(name) {
            if value.replace(header.value.as_str().to_string()).is_some() {
                return None;
            }
        }
    }
    value
}

pub(super) fn json_content_type(request: &Request) -> bool {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Content-Type"))
        .is_none_or(|header| {
            header
                .value
                .as_str()
                .to_ascii_lowercase()
                .starts_with("application/json")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITY: &str =
        "asb_codex_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn codex_paths_are_a_closed_typed_operation_set() {
        let cases = [
            ("responses", CodexOperation::Responses, Method::Post),
            ("responses/compact", CodexOperation::Compact, Method::Post),
            ("models", CodexOperation::Models, Method::Get),
            (
                "chat/completions",
                CodexOperation::ChatCompletions,
                Method::Post,
            ),
            ("alpha/search", CodexOperation::AlphaSearch, Method::Post),
            (
                "images/generations",
                CodexOperation::ImageGeneration,
                Method::Post,
            ),
            ("images/edits", CodexOperation::ImageEdit, Method::Post),
        ];
        for (path, expected, method) in cases {
            let url = format!("/codex/{CAPABILITY}/v1/{path}?trace=fixture");
            let request = codex_request(&url).expect("known Codex operation");
            assert_eq!(request.capability, CAPABILITY);
            assert_eq!(request.operation, expected);
            assert_eq!(request.operation.method(), method);
        }
        assert!(codex_request(&format!("/codex/{CAPABILITY}/v1/files")).is_none());
        assert!(codex_request(&format!("/codex/{CAPABILITY}/v1/v1/responses")).is_none());
        assert!(codex_request("/codex/asb_local_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/v1/responses").is_none());
    }

    #[test]
    fn native_responses_headers_keep_protocol_facts_and_drop_credentials() {
        let incoming = [
            Header::from_bytes("openai-beta", "responses=v1").unwrap(),
            Header::from_bytes("user-agent", "codex/1.0").unwrap(),
            Header::from_bytes("session_id", "session-fixture").unwrap(),
            Header::from_bytes("x-stainless-lang", "rust").unwrap(),
            Header::from_bytes("Authorization", "Bearer official-token").unwrap(),
            Header::from_bytes("chatgpt-account-id", "official-account").unwrap(),
            Header::from_bytes("Cookie", "official_cookie=1").unwrap(),
            Header::from_bytes("x-request-id", "client-trace").unwrap(),
            Header::from_bytes("Content-Encoding", "zstd").unwrap(),
        ];
        let mut forwarded = HeaderMap::new();
        append_native_responses_headers(&mut forwarded, &incoming);
        assert_eq!(forwarded["openai-beta"], "responses=v1");
        assert_eq!(forwarded["user-agent"], "codex/1.0");
        assert_eq!(forwarded["session_id"], "session-fixture");
        assert_eq!(forwarded["x-stainless-lang"], "rust");
        for name in [
            "authorization",
            "chatgpt-account-id",
            "cookie",
            "x-request-id",
            "content-encoding",
        ] {
            assert!(
                !forwarded.contains_key(name),
                "{name} must not reach third-party upstreams"
            );
        }
    }

    #[test]
    fn request_query_cannot_replace_the_fixed_upstream_target() {
        let upstream = "https://vendor.example/api/v1/responses".to_string();
        let gateway = "http://127.0.0.1:47821";
        let forwarded = with_request_query(
            upstream.clone(),
            gateway,
            "/codex/asb_codex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/v1/responses?trace=a%20b&mode=test",
        )
        .expect("valid query");
        assert_eq!(
            forwarded,
            "https://vendor.example/api/v1/responses?trace=a%20b&mode=test"
        );

        for request_url in [
            "/codex/token/v1/responses?",
            "/codex/token/v1/responses?trace=1#other",
            "/codex/token/v1/responses?=value",
            "/codex/token/v1/responses?trace=1&",
            "/codex/token/v1/responses?trace=\nvalue",
        ] {
            assert!(
                with_request_query(upstream.clone(), gateway, request_url).is_err(),
                "unexpectedly accepted {request_url:?}"
            );
        }
    }
}
