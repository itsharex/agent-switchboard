//! Identifies the process holding a listener port, when the platform reports
//! it reliably. Strictly read-only: the gateway never terminates other
//! processes, and an unidentified holder is reported as unknown.

/// A probe result for the serialized failure report.
pub(crate) struct GatewayProcessInfo {
    pub(crate) pid: u32,
    pub(crate) name: Option<String>,
}

/// Finds one process with a TCP listener on `port`. Returns `None` when the
/// platform probe is unavailable or its output cannot be trusted.
pub(super) fn find_listener(port: u16) -> Option<GatewayProcessInfo> {
    find_listener_os(port)
}

#[cfg(target_os = "windows")]
fn find_listener_os(port: u16) -> Option<GatewayProcessInfo> {
    let output = std::process::Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let local_suffix = format!(":{port}");
    let mut pid = None;
    for line in text.lines() {
        // `Proto  Local  Foreign  State  PID` for every TCP row.
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() != 5
            || !columns[0].eq_ignore_ascii_case("tcp")
            || !columns[3].eq_ignore_ascii_case("LISTENING")
            || !columns[1].ends_with(&local_suffix)
        {
            continue;
        }
        match columns[4].parse::<u32>() {
            Ok(found) => {
                pid = Some(found);
                break;
            }
            Err(_) => continue,
        }
    }
    let pid = pid?;
    Some(GatewayProcessInfo {
        pid,
        name: windows_process_name(pid),
    })
}

#[cfg(target_os = "windows")]
fn windows_process_name(pid: u32) -> Option<String> {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|line| !line.trim().is_empty())?;
    let name = line.trim().strip_prefix('"')?;
    let end = name.find('"')?;
    let name = &name[..end];
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(not(target_os = "windows"))]
fn find_listener_os(_port: u16) -> Option<GatewayProcessInfo> {
    None
}
