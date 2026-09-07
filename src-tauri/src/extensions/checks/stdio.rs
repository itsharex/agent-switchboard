//! The stdio MCP check: spawn, JSON-RPC negotiation, capped line
//! reading, and process-tree termination.

use super::{
    CheckDeadline, CheckError, CheckErrorKind, McpCheckOutcome, ProbeOutcome, DISCOVERY_DEADLINE,
    LEGACY_REQUESTED_PROTOCOL_VERSION, MAX_OUTPUT_BYTES, MAX_PAGES, MODERN_PROTOCOL_VERSION,
    SUPPORTED_PROTOCOL_VERSIONS,
};
use super::{MAX_LINE_BYTES, RECEIVE_POLL};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, SyncSender};
use std::time::Duration;

pub(super) struct RpcConnection {
    send: Box<dyn FnMut(&serde_json::Value) -> Result<(), CheckError>>,
    receive: Box<dyn FnMut(Duration) -> Result<Option<ReaderMessage>, CheckError>>,
    bytes_read: u64,
}

enum ReaderMessage {
    Line(String),
    Closed,
    Error,
}

impl RpcConnection {
    fn call(
        &mut self,
        request: &serde_json::Value,
        deadline: &CheckDeadline,
    ) -> Result<serde_json::Value, CheckError> {
        deadline.remaining()?;
        (self.send)(request)?;
        loop {
            let wait = deadline.remaining()?.min(RECEIVE_POLL);
            match (self.receive)(wait)? {
                Some(ReaderMessage::Line(line)) => {
                    self.bytes_read = self
                        .bytes_read
                        .checked_add(line.len() as u64)
                        .ok_or_else(CheckError::output_limit)?;
                    if self.bytes_read > MAX_OUTPUT_BYTES {
                        return Err(CheckError::output_limit());
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if value.get("id").is_some_and(|id| *id == request["id"]) {
                            return Ok(value);
                        }
                    }
                }
                Some(ReaderMessage::Closed) => return Err(CheckError::closed()),
                Some(ReaderMessage::Error) => {
                    return Err(CheckError::transport("服务输出无法读取"));
                }
                None => {}
            }
        }
    }
}

pub(super) fn check_stdio(
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    deadline: &CheckDeadline,
) -> ProbeOutcome {
    if let Err(error) = deadline.remaining() {
        return probe_for_error(None, &error);
    }
    let mut builder = Command::new(command);
    builder
        .args(args)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        builder.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match builder.spawn() {
        Ok(child) => child,
        Err(_) => return ProbeOutcome::failed("spawn", "无法启动检测命令"),
    };
    drive_stdio_check(&mut child, deadline)
}

pub(super) fn drive_stdio_check(child: &mut Child, deadline: &CheckDeadline) -> ProbeOutcome {
    let Some(stdin) = child.stdin.take() else {
        terminate_process_tree(child);
        return ProbeOutcome::failed("spawn", "检测命令未提供标准输入");
    };
    let Some(stdout) = child.stdout.take() else {
        terminate_process_tree(child);
        return ProbeOutcome::failed("spawn", "检测命令未提供标准输出");
    };
    let (sender, receiver) = mpsc::sync_channel::<ReaderMessage>(32);
    let reader = std::thread::spawn(move || read_stdio_lines(stdout, sender));
    let mut stdin = stdin;
    let mut connection = RpcConnection {
        send: Box::new(move |value| {
            let mut line =
                serde_json::to_string(value).map_err(|_| CheckError::protocol("请求无法编码"))?;
            line.push('\n');
            stdin
                .write_all(line.as_bytes())
                .map_err(|_| CheckError::transport("无法写入检测命令"))?;
            stdin
                .flush()
                .map_err(|_| CheckError::transport("无法写入检测命令"))
        }),
        receive: Box::new(move |wait| match receiver.recv_timeout(wait) {
            Ok(message) => Ok(Some(message)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(Some(ReaderMessage::Closed)),
        }),
        bytes_read: 0,
    };
    let probe = negotiate_stdio(&mut connection, deadline);
    // Releasing stdin and the receiver unblocks the bounded reader before
    // waiting for a long-running MCP process to terminate.
    drop(connection);
    terminate_process_tree(child);
    let _ = reader.join();
    probe
}

fn read_stdio_lines(stdout: std::process::ChildStdout, sender: SyncSender<ReaderMessage>) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_line_capped(&mut reader, MAX_LINE_BYTES) {
            Ok(Some(bytes)) => match String::from_utf8(bytes) {
                Ok(line) => {
                    if sender.send(ReaderMessage::Line(line)).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = sender.send(ReaderMessage::Error);
                    break;
                }
            },
            Ok(None) => break,
            Err(_) => {
                let _ = sender.send(ReaderMessage::Error);
                break;
            }
        }
    }
    let _ = sender.send(ReaderMessage::Closed);
}

pub(super) fn read_line_capped<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }
        if let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            if line.len().saturating_add(newline) > max_bytes {
                reader.consume(newline + 1);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "line exceeds cap",
                ));
            }
            line.extend_from_slice(&buffer[..newline]);
            reader.consume(newline + 1);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
        let length = buffer.len();
        if line.len().saturating_add(length) > max_bytes {
            reader.consume(length);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "line exceeds cap",
            ));
        }
        line.extend_from_slice(buffer);
        reader.consume(length);
    }
}

pub(super) fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(super) fn negotiate_stdio(
    connection: &mut RpcConnection,
    deadline: &CheckDeadline,
) -> ProbeOutcome {
    let discovery_deadline = deadline.limited(DISCOVERY_DEADLINE);
    let discovery = rpc_request(1, "server/discover", None, true);
    match connection.call(&discovery, &discovery_deadline) {
        Ok(response) => match modern_supported(&response) {
            Ok(true) => return list_stdio(connection, MODERN_PROTOCOL_VERSION, true, deadline),
            Ok(false) => {}
            Err(error) => return probe_for_error(None, &error),
        },
        Err(error)
            if matches!(error.kind, CheckErrorKind::TimedOut)
                && !deadline.cancelled()
                && deadline.remaining().is_ok() => {}
        Err(error) => return probe_for_error(None, &error),
    }
    negotiate_legacy_stdio(connection, deadline)
}

pub(super) fn negotiate_legacy_stdio(
    connection: &mut RpcConnection,
    deadline: &CheckDeadline,
) -> ProbeOutcome {
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "initialize",
        "params": {
            "protocolVersion": LEGACY_REQUESTED_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "agent-switchboard", "version": env!("CARGO_PKG_VERSION")},
        }
    });
    let response = match connection.call(&initialize, deadline) {
        Ok(response) => response,
        Err(error) => return probe_for_error(None, &error),
    };
    let Some(result) = response.get("result") else {
        return ProbeOutcome::failed("protocol", "服务端拒绝 initialize 请求");
    };
    let Some(protocol_version) = result
        .get("protocolVersion")
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return ProbeOutcome::failed("protocol", "服务端未返回协议版本");
    };
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version.as_str())
        || protocol_version == MODERN_PROTOCOL_VERSION
    {
        return ProbeOutcome {
            outcome: McpCheckOutcome::Failed {
                classification: "version".to_string(),
                error: format!("服务端协议版本 {protocol_version} 不在本检测支持范围内"),
            },
            protocol_version: Some(protocol_version),
            truncated: false,
        };
    }
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    if let Err(error) = deadline
        .remaining()
        .and_then(|_| (connection.send)(&notification))
    {
        return ProbeOutcome::partial(Some(protocol_version), error.message, false);
    }
    list_stdio(connection, &protocol_version, false, deadline)
}

pub(super) fn list_stdio(
    connection: &mut RpcConnection,
    protocol_version: &str,
    modern: bool,
    deadline: &CheckDeadline,
) -> ProbeOutcome {
    list_directories(
        protocol_version,
        modern,
        deadline,
        |request, request_deadline| connection.call(request, request_deadline),
    )
}

pub(super) fn modern_supported(response: &serde_json::Value) -> Result<bool, CheckError> {
    if let Some(result) = response.get("result") {
        let Some(versions) = result
            .get("supportedVersions")
            .and_then(|value| value.as_array())
        else {
            return Err(CheckError::protocol("server/discover 未返回支持的协议版本"));
        };
        return Ok(versions
            .iter()
            .filter_map(|value| value.as_str())
            .any(|version| version == MODERN_PROTOCOL_VERSION));
    }
    if response_error_code(response) == Some(-32601) {
        return Ok(false);
    }
    Err(CheckError::protocol("服务端拒绝 server/discover 请求"))
}

pub(super) fn list_directories(
    protocol_version: &str,
    modern: bool,
    deadline: &CheckDeadline,
    mut call: impl FnMut(&serde_json::Value, &CheckDeadline) -> Result<serde_json::Value, CheckError>,
) -> ProbeOutcome {
    let mut counts = (0u64, 0u64, 0u64);
    for (index, method) in ["tools/list", "resources/list", "prompts/list"]
        .into_iter()
        .enumerate()
    {
        let mut cursor: Option<String> = None;
        for page in 0..MAX_PAGES {
            let request = rpc_request(
                100 + (index as u64 * MAX_PAGES as u64) + page as u64,
                method,
                cursor.as_deref(),
                modern,
            );
            let response = match call(&request, deadline) {
                Ok(response) => response,
                Err(error) if matches!(error.kind, CheckErrorKind::Cancelled) => {
                    return ProbeOutcome::cancelled(Some(protocol_version.to_string()))
                }
                Err(error) => {
                    return ProbeOutcome::partial(
                        Some(protocol_version.to_string()),
                        error.message,
                        matches!(error.kind, CheckErrorKind::OutputLimit),
                    )
                }
            };
            let Some(result) = response.get("result") else {
                if response_error_code(&response) == Some(-32601) {
                    break;
                }
                return ProbeOutcome::partial(
                    Some(protocol_version.to_string()),
                    "服务端拒绝目录请求",
                    false,
                );
            };
            let items = result
                .get(match method {
                    "tools/list" => "tools",
                    "resources/list" => "resources",
                    _ => "prompts",
                })
                .and_then(|value| value.as_array())
                .map(|items| items.len() as u64)
                .unwrap_or(0);
            counts = match index {
                0 => (counts.0 + items, counts.1, counts.2),
                1 => (counts.0, counts.1 + items, counts.2),
                _ => (counts.0, counts.1, counts.2 + items),
            };
            cursor = result
                .get("nextCursor")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
            if page + 1 == MAX_PAGES {
                return ProbeOutcome::partial(
                    Some(protocol_version.to_string()),
                    "目录分页超过上限，结果已截断",
                    true,
                );
            }
        }
    }
    ProbeOutcome {
        outcome: McpCheckOutcome::Passed {
            tools: counts.0,
            resources: counts.1,
            prompts: counts.2,
        },
        protocol_version: Some(protocol_version.to_string()),
        truncated: false,
    }
}

pub(super) fn rpc_request(
    id: u64,
    method: &str,
    cursor: Option<&str>,
    modern: bool,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    if let Some(cursor) = cursor {
        params.insert(
            "cursor".to_string(),
            serde_json::Value::String(cursor.to_string()),
        );
    }
    if modern {
        params.insert(
            "_meta".to_string(),
            serde_json::json!({
                "io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientInfo": {
                    "name": "agent-switchboard",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "io.modelcontextprotocol/clientCapabilities": {},
            }),
        );
    }
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

pub(super) fn response_error_code(response: &serde_json::Value) -> Option<i64> {
    response
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(|code| code.as_i64())
}

pub(super) fn probe_for_error(
    protocol_version: Option<String>,
    error: &CheckError,
) -> ProbeOutcome {
    match error.kind {
        CheckErrorKind::Cancelled => ProbeOutcome::cancelled(protocol_version),
        CheckErrorKind::TimedOut => ProbeOutcome {
            outcome: McpCheckOutcome::Failed {
                classification: "timeout".to_string(),
                error: error.message.clone(),
            },
            protocol_version,
            truncated: false,
        },
        CheckErrorKind::OutputLimit => ProbeOutcome {
            outcome: McpCheckOutcome::Failed {
                classification: "output".to_string(),
                error: error.message.clone(),
            },
            protocol_version,
            truncated: true,
        },
        CheckErrorKind::Transport => ProbeOutcome {
            outcome: McpCheckOutcome::Failed {
                classification: "network".to_string(),
                error: error.message.clone(),
            },
            protocol_version,
            truncated: false,
        },
        CheckErrorKind::Closed | CheckErrorKind::Protocol => ProbeOutcome {
            outcome: McpCheckOutcome::Failed {
                classification: "protocol".to_string(),
                error: error.message.clone(),
            },
            protocol_version,
            truncated: false,
        },
    }
}
