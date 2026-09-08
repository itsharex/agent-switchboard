use super::*;

#[test]
fn loopback_gateway_streams_a_converted_first_event_before_upstream_finishes() {
    let upstream = TcpListener::bind(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = format!(
        "http://{}",
        upstream.local_addr().expect("upstream address")
    );
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (release_sender, release_receiver) = mpsc::channel();
    let (upstream_first_sender, upstream_first_receiver) = mpsc::channel();
    let upstream_worker =
        serve_streaming_upstream(upstream, upstream_first_sender, release_receiver);

    let directory = tempfile::tempdir().expect("temporary state");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = sandbox_profile(
        &state,
        AppKind::Codex,
        "streaming chat sandbox",
        upstream_url,
        upstream_key,
        UpstreamProtocol::ChatCompletions,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(
            profile,
            default_client_settings(AppKind::Codex),
        ))
        .expect("project route");
    gateway
        .commit(&projection, || Ok(()))
        .expect("commit route");

    let gateway_url = codex_endpoint(&projection);
    let local_token = projection_token(&projection);
    let (first_sender, first_receiver) = mpsc::channel();
    let client_worker = start_stream_client(gateway_url, local_token, first_sender);

    upstream_first_receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("upstream must flush its first chunk");
    let first = first_receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("gateway must emit before upstream release");
    assert!(first.contains("response.created"));
    assert!(!first.contains("streamed answer"));
    release_sender.send(()).expect("release upstream stream");
    let rest = client_worker.join().expect("client worker");
    assert!(rest.contains("response.output_text.delta"), "{rest}");
    assert!(rest.contains("streamed answer"), "{rest}");
    assert!(rest.contains("response.completed"), "{rest}");
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

fn serve_streaming_upstream(
    upstream: TcpListener,
    upstream_first_sender: mpsc::Sender<()>,
    release_receiver: mpsc::Receiver<()>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (mut stream, _) = upstream.accept().expect("upstream receive");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set read timeout");
        read_stream_request(&mut stream);
        let first = br#"data: {"id":"chat_stream","object":"chat.completion.chunk","created":0,"model":"sandbox-model","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

"#
            .to_vec();
        let rest = br#"data: {"id":"chat_stream","object":"chat.completion.chunk","created":0,"model":"sandbox-model","choices":[{"index":0,"delta":{"content":"streamed answer"},"finish_reason":null}]}

data: {"id":"chat_stream","object":"chat.completion.chunk","created":0,"model":"sandbox-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#
            .to_vec();
        stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .expect("write headers");
        write!(stream, "{:x}\r\n", first.len()).expect("write first chunk length");
        stream.write_all(&first).expect("write first chunk");
        stream.write_all(b"\r\n").expect("finish first chunk");
        stream.flush().expect("flush first chunk");
        upstream_first_sender
            .send(())
            .expect("report upstream first chunk");
        release_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("test stream release");
        write!(stream, "{:x}\r\n", rest.len()).expect("write remaining chunk length");
        stream.write_all(&rest).expect("write remaining chunk");
        stream.write_all(b"\r\n0\r\n\r\n").expect("finish response");
        stream.flush().expect("flush response");
    })
}

fn read_stream_request(stream: &mut TcpStream) {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let count = stream.read(&mut buffer).expect("read upstream request");
        assert!(count > 0, "request ended before headers");
        request.extend_from_slice(&buffer[..count]);
    }
    let headers_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("request headers end");
    let content_length = std::str::from_utf8(&request[..headers_end])
        .expect("request headers UTF-8")
        .lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("Content-Length")
                    .then(|| value.trim().parse::<usize>().expect("content length"))
            })
        })
        .expect("request content length");
    let request_end = headers_end + 4 + content_length;
    while request.len() < request_end {
        let count = stream
            .read(&mut buffer)
            .expect("read upstream request body");
        assert!(count > 0, "request ended before body");
        request.extend_from_slice(&buffer[..count]);
    }
}

fn start_stream_client(
    gateway_url: String,
    local_token: String,
    first_sender: mpsc::Sender<String>,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let client = Client::builder().no_proxy().build().expect("client");
        let mut response = client
            .post(gateway_url)
            .header("Authorization", format!("Bearer {local_token}"))
            .header(CONTENT_TYPE, "application/json")
            .body(
                serde_json::to_vec(&json!({
                    "model": "sandbox-model",
                    "input": "stream through gateway",
                    "max_output_tokens": 32,
                    "stream": true,
                }))
                .expect("request json"),
            )
            .send()
            .expect("gateway request");
        assert!(response.status().is_success());
        let mut first = [0_u8; 4096];
        let count = response.read(&mut first).expect("first stream bytes");
        first_sender
            .send(String::from_utf8(first[..count].to_vec()).expect("first utf8"))
            .expect("report first frame");
        let mut rest = String::new();
        response
            .read_to_string(&mut rest)
            .expect("remaining stream");
        rest
    })
}
