//! Real Codex app-server protocol against an isolated gateway and fake upstream.
use super::*;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};

const TOOL_NAME: &str = "asb_fixture_echo";
const TOOL_NAMESPACE: &str = "asb_fixture";
const TOOL_OUTPUT: &str = "asb-fixture-tool-result";

#[test]
#[ignore = "requires ASB_TEST_CODEX_BIN pointing to the installed Codex CLI"]
fn actual_codex_manual_compaction_survives_client_restart_and_continues() {
    verify_compaction(false);
}

#[test]
#[ignore = "requires ASB_TEST_CODEX_BIN pointing to the installed Codex CLI"]
fn actual_codex_automatic_compaction_survives_client_restart_and_continues() {
    verify_compaction(true);
}

#[test]
#[ignore = "requires ASB_TEST_CODEX_BIN pointing to the installed Codex CLI"]
fn actual_codex_tool_search_loads_a_deferred_tool_through_the_chat_gateway() {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "tool search CLI",
        endpoint(&upstream),
        "fixture-vendor-key".into(),
        CodexUpstream::ChatCompletions,
    );
    file.parameters.settings.insert(
        "web_search".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("disabled".into()),
        },
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    let (home, workdir) = super::codex_cli::prepare_client(
        directory.path(),
        &projection,
        &gateway,
        "fixture-vendor-key",
    );
    let (seen_sender, seen_receiver) = mpsc::channel();
    let worker = thread::spawn(move || tool_search_upstream(upstream, seen_sender));
    let mut client = AppServer::start(&home);
    let started = client.call(
        2,
        "thread/start",
        json!({"cwd":workdir,"model":"sandbox-model","approvalPolicy":"never","sandbox":"read-only",
            "dynamicTools":[{"type":"namespace","name":TOOL_NAMESPACE,
                "description":"Isolated deferred test tools.","tools":[{
                    "type":"function","name":TOOL_NAME,
                    "description":"Return the fixed isolated test value without side effects.",
                    "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
                    "deferLoading":true}]}]}),
    );
    let id = started["thread"]["id"].as_str().unwrap().to_string();
    client.turn(
        3,
        &id,
        "Use the deferred sandbox tool and return its result",
    );
    assert_eq!(client.tool_calls, 1);
    assert_eq!(
        client.tool_namespaces,
        vec![Some(TOOL_NAMESPACE.to_string())],
        "the deferred dynamic tool callback must retain its namespace"
    );
    drop(client);
    worker.join().unwrap();
    let observed: Vec<_> = seen_receiver.try_iter().collect();
    assert_eq!(
        observed.len(),
        3,
        "search, loaded tool, and result requests"
    );
    assert_chat_tool_search_roundtrip(&observed);
    gateway.shutdown();
}

fn verify_compaction(automatic: bool) {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "compact CLI",
        endpoint(&upstream),
        "fixture-vendor-key".into(),
        CodexUpstream::Responses,
    );
    file.parameters.settings.insert(
        "web_search".into(),
        SettingValue::Explicit {
            value: ConfigValue::Str("disabled".into()),
        },
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    let (home, workdir) = super::codex_cli::prepare_client(
        directory.path(),
        &projection,
        &gateway,
        "fixture-vendor-key",
    );
    if automatic {
        let config = home.join("config.toml");
        let mut document = fs::read_to_string(&config)
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        document["model_auto_compact_token_limit"] = toml_edit::value(100_000);
        fs::write(config, document.to_string()).unwrap();
    }
    let before = fs::read(home.join("auth.json")).unwrap();
    let (seen_sender, seen_receiver) = mpsc::channel();
    let worker = thread::spawn(move || upstream_loop(upstream, seen_sender, automatic));
    let mut client = AppServer::start(&home);
    let started = client.call(2, "thread/start", json!({"cwd":workdir,"model":"sandbox-model","approvalPolicy":"never","sandbox":"read-only",
        "dynamicTools":[{"type":"function","name":TOOL_NAME,"description":"Return the fixed isolated test value without side effects.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},"deferLoading":false}]}));
    let id = started["thread"]["id"].as_str().unwrap().to_string();
    client.turn(3, &id, "Return exactly: sandbox answer");
    client.turn(4, &id, "Continue with the same task");
    if !automatic {
        client.call(5, "thread/compact/start", json!({"threadId":id}));
    }
    client.wait("completed contextCompaction item", |message| {
        message["method"] == "item/completed"
            && message["params"]["item"]["type"] == "contextCompaction"
    });
    drop(client);
    let mut resumed = AppServer::start(&home);
    resumed.call(6, "thread/resume", json!({"threadId":id,"cwd":workdir}));
    resumed.turn(7, &id, "Continue after compaction and restart");
    assert_eq!(resumed.tool_calls, 1);
    let observed: Vec<_> = seen_receiver.try_iter().collect();
    assert_compacted_history(&observed);
    drop(resumed);
    assert_eq!(fs::read(home.join("auth.json")).unwrap(), before);
    worker.join().unwrap();
    gateway.shutdown();
}

fn assert_compacted_history(observed: &[Value]) {
    assert_eq!(
        observed.len(),
        5,
        "two turns, one compaction, resumed tool call and final response"
    );
    assert_eq!(
        observed
            .iter()
            .filter(|body| body["stream"] == false)
            .count(),
        1
    );
    assert!(observed
        .last()
        .unwrap()
        .to_string()
        .contains("Gateway summary: preserve the task and run tests."));
    assert!(!observed
        .last()
        .unwrap()
        .to_string()
        .contains("asb-compaction-v1."));
    let outputs: Vec<_> = observed.last().unwrap()["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .collect();
    assert!(
        outputs.iter().any(|item| {
            item["call_id"] == "call_sandbox_readonly"
                && item["output"].to_string().contains(TOOL_OUTPUT)
        }),
        "actual tool outputs: {outputs:?}"
    );
    let tool_output = &outputs
        .iter()
        .find(|item| item["call_id"] == "call_sandbox_readonly")
        .unwrap()["output"];
    eprintln!("isolated dynamic tool result: {tool_output}");
    assert_eq!(tool_output, &json!(TOOL_OUTPUT));
}

struct AppServer {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: mpsc::Receiver<Value>,
    pending: VecDeque<Value>,
    tool_calls: usize,
    tool_namespaces: Vec<Option<String>>,
}

impl AppServer {
    fn start(home: &std::path::Path) -> Self {
        let mut child = super::codex_cli::isolated_codex(home)
            .args(["app-server", "--listen", "stdio://"])
            .current_dir(home.parent().unwrap().join("work"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str(&line) {
                    if sender.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        let mut server = Self {
            child,
            input,
            output: receiver,
            pending: VecDeque::new(),
            tool_calls: 0,
            tool_namespaces: Vec::new(),
        };
        server.call(1, "initialize", json!({"clientInfo":{"name":"asb_isolated_test","version":"1"},"capabilities":{"experimentalApi":true}}));
        server.send(json!({"method":"initialized"}));
        server
    }
    fn send(&mut self, value: Value) {
        writeln!(self.input, "{value}").unwrap();
        self.input.flush().unwrap();
    }
    fn call(&mut self, id: u32, method: &str, params: Value) -> Value {
        self.send(json!({"id":id,"method":method,"params":params}));
        let reply = self.wait(method, |message| message["id"] == id);
        assert!(
            reply.get("error").is_none(),
            "app-server {method} failed: {reply}"
        );
        reply["result"].clone()
    }
    fn wait(&mut self, expected: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.pending.iter().position(&predicate) {
            return self.pending.remove(index).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let value = self.output.recv_timeout(remaining).unwrap_or_else(|error| {
                let seen: Vec<_> = self
                    .pending
                    .iter()
                    .map(|v| (&v["method"], &v["params"]["item"]["type"]))
                    .collect();
                panic!("Codex {expected} deadline: {error}; observed methods: {seen:?}")
            });
            assert!(
                value["method"] != "error",
                "Codex reported failure: {value}"
            );
            if value["method"] == "item/tool/call" {
                assert_eq!(value["params"]["tool"], TOOL_NAME);
                self.tool_namespaces
                    .push(value["params"]["namespace"].as_str().map(str::to_string));
                assert_eq!(value["params"]["arguments"], json!({}));
                self.send(json!({"id":value["id"],"result":{"success":true,
                    "contentItems":[{"type":"inputText","text":TOOL_OUTPUT}]}}));
                self.tool_calls += 1;
                continue;
            }
            if predicate(&value) {
                return value;
            }
            self.pending.push_back(value);
        }
    }
    fn turn(&mut self, sequence: u32, id: &str, text: &str) {
        self.call(
            sequence,
            "turn/start",
            json!({"threadId":id,"input":[{"type":"text","text":text}]}),
        );
        let done = self.wait("turn/completed", |message| {
            message["method"] == "turn/completed"
        });
        assert_eq!(done["params"]["turn"]["status"], "completed", "{done}");
    }
}
impl Drop for AppServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn upstream_loop(upstream: Server, seen: mpsc::Sender<Value>, automatic: bool) {
    for index in 0..5 {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(60))
            .unwrap()
            .unwrap();
        assert_eq!(request.url(), "/v1/responses");
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        let body: Value = serde_json::from_str(&body).unwrap();
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str());
        assert_eq!(authorization, Some("Bearer fixture-vendor-key"));
        assert!(!body.to_string().contains("fixture-official-access"));
        let stream = body["stream"] == true;
        let tool = (index == 3).then(|| dynamic_tool_call(&body));
        seen.send(body).unwrap();
        let text = if stream {
            "sandbox answer"
        } else {
            "Gateway summary: preserve the task and run tests."
        };
        // The first completed turn crosses the configured limit. Later usage stays
        // below it, so the actual CLI must compact once without a manual command.
        let input_tokens = if automatic && index == 0 {
            200_000
        } else {
            200
        };
        let mut response = json!({"id":format!("resp_{index}"),"object":"response","status":"completed","model":"sandbox-model",
            "output":[{"id":format!("msg_{index}"),"type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":text,"annotations":[]}]}],
            "usage":{"input_tokens":input_tokens,"output_tokens":10,"total_tokens":input_tokens + 10},"error":null});
        if let Some(tool) = tool {
            response["output"] = json!([tool]);
        }
        let output = if stream {
            response_events(&response)
        } else {
            response.to_string()
        };
        request
            .respond(
                Response::from_string(output).with_header(content_type(if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                })),
            )
            .unwrap();
    }
}

fn tool_search_upstream(upstream: Server, seen: mpsc::Sender<Value>) {
    for index in 0..3 {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(60))
            .unwrap()
            .unwrap();
        assert_eq!(request.url(), "/v1/chat/completions");
        let mut raw = String::new();
        request.as_reader().read_to_string(&mut raw).unwrap();
        let body: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str()),
            Some("Bearer fixture-vendor-key")
        );
        assert!(!raw.contains("fixture-official-access"));
        let response = match index {
            0 => chat_tool_call_stream("chat_search", "call_search", "tool_search"),
            1 => chat_tool_call_stream(
                "chat_dynamic",
                "call_dynamic",
                &loaded_chat_tool_name(&body),
            ),
            2 => chat_text_stream("chat_done", "sandbox answer"),
            _ => unreachable!(),
        };
        seen.send(body).unwrap();
        request
            .respond(
                Response::from_string(response)
                    .with_header(content_type("text/event-stream; charset=utf-8")),
            )
            .unwrap();
    }
}

fn loaded_chat_tool_name(body: &Value) -> String {
    body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
        .find(|name| *name != "tool_search")
        .map(str::to_string)
        .expect("tool search output must register the deferred dynamic tool")
}

fn chat_tool_call_stream(response_id: &str, call_id: &str, name: &str) -> String {
    let arguments = if name == "tool_search" {
        json!({"query":"sandbox deferred tool"}).to_string()
    } else {
        "{}".to_string()
    };
    [
        json!({"id":response_id,"object":"chat.completion.chunk","created":0,"model":"sandbox-model",
            "choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}),
        json!({"id":response_id,"object":"chat.completion.chunk","created":0,"model":"sandbox-model",
            "choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":call_id,"type":"function",
                "function":{"name":name,"arguments":arguments}}]},"finish_reason":null}]}),
        json!({"id":response_id,"object":"chat.completion.chunk","created":0,"model":"sandbox-model",
            "choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
    ]
    .iter()
    .map(|event| format!("data: {event}\n\n"))
    .chain(std::iter::once("data: [DONE]\n\n".to_string()))
    .collect()
}

fn chat_text_stream(response_id: &str, text: &str) -> String {
    [
        json!({"id":response_id,"object":"chat.completion.chunk","created":0,"model":"sandbox-model",
            "choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}),
        json!({"id":response_id,"object":"chat.completion.chunk","created":0,"model":"sandbox-model",
            "choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]}),
        json!({"id":response_id,"object":"chat.completion.chunk","created":0,"model":"sandbox-model",
            "choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
    ]
    .iter()
    .map(|event| format!("data: {event}\n\n"))
    .chain(std::iter::once("data: [DONE]\n\n".to_string()))
    .collect()
}

fn assert_chat_tool_search_roundtrip(observed: &[Value]) {
    let first = &observed[0];
    assert!(first["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|tool| tool.pointer("/function/name") == Some(&json!("tool_search"))));
    let second = &observed[1];
    assert!(second["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|message| { message["role"] == "tool" && message["tool_call_id"] == "call_search" }));
    assert_ne!(
        loaded_chat_tool_name(second),
        "tool_search",
        "tool search output must expose the deferred dynamic tool to Chat"
    );
    let third = &observed[2];
    assert!(third["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|message| {
            message["role"] == "tool"
                && message["tool_call_id"] == "call_dynamic"
                && message["content"].to_string().contains(TOOL_OUTPUT)
        }));
}

fn response_events(response: &Value) -> String {
    let item = &response["output"][0];
    let delta = if item["type"] == "function_call" {
        json!({"type":"response.function_call_arguments.delta","item_id":item["id"],"output_index":0,"delta":item["arguments"]})
    } else {
        json!({"type":"response.output_text.delta","item_id":item["id"],"output_index":0,"content_index":0,"delta":"sandbox answer"})
    };
    let mut added = item.clone();
    if item["type"] == "function_call" {
        added["arguments"] = json!("");
    }
    [json!({"type":"response.created","response":{"id":response["id"],"model":response["model"],"output":[]}}),
        json!({"type":"response.output_item.added","output_index":0,"item":added}),
        delta,
        json!({"type":"response.output_item.done","output_index":0,"item":item}),
        json!({"type":"response.completed","response":response})]
        .iter().map(|event| format!("data: {event}\n\n")).collect()
}

fn dynamic_tool_call(body: &Value) -> Value {
    let tools = body["tools"].as_array().expect("actual Codex tools");
    let mut candidates = Vec::new();
    for tool in tools {
        if tool["type"] == "namespace" {
            for function in tool["tools"].as_array().unwrap() {
                candidates.push((function, tool["name"].as_str()));
            }
        } else {
            candidates.push((tool, None));
        }
    }
    let (tool, namespace) = candidates
        .into_iter()
        .find(|(tool, _)| tool["name"] == TOOL_NAME)
        .expect("the resumed CLI must retain the registered dynamic tool");
    let mut call = json!({"type":"function_call","id":"fc_sandbox_readonly",
        "call_id":"call_sandbox_readonly","name":tool["name"],"arguments":"{}"});
    if let Some(namespace) = namespace {
        call["namespace"] = json!(namespace);
    }
    call
}
