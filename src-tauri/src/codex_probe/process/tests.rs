use super::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn sleeper() -> Command {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 60"]);
        command
    };
    #[cfg(unix)]
    let mut command = { let mut command = Command::new("sleep"); command.arg("60"); command };
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    command
}

#[test]
fn dropping_lifetime_terminates_the_owned_child() {
    let (mut child, lifetime) = spawn(&mut sleeper()).unwrap();
    drop(lifetime);
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("probe survived its lifetime owner");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "subprocess helper for abrupt_parent_exit_terminates_the_probe"]
fn lifetime_parent() {
    let Some(marker) = std::env::var_os("ASB_PROBE_LIFETIME_TEST") else { return; };
    let (mut child, _lifetime) = spawn(&mut sleeper()).unwrap();
    std::fs::write(marker, child.id().to_string()).unwrap();
    child.wait().unwrap();
}

#[cfg(windows)]
#[test]
fn abrupt_parent_exit_terminates_the_probe() {
    use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("child-pid");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "codex_probe::process::tests::lifetime_parent", "--ignored"])
        .env("ASB_PROBE_LIFETIME_TEST", &marker)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::process_control::suppress_window(&mut command);
    let mut parent = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let pid: u32 = loop {
        if let Some(pid) = std::fs::read_to_string(&marker).ok().and_then(|s| s.parse().ok()) { break pid; }
        if Instant::now() >= deadline {
            let _ = parent.kill(); let _ = parent.wait();
            panic!("parent helper did not start a probe");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    parent.kill().unwrap();
    parent.wait().unwrap();
    assert!(!handle.is_null());
    let status = unsafe { WaitForSingleObject(handle, 5_000) };
    unsafe { CloseHandle(handle); }
    assert_eq!(status, 0, "probe survived abrupt termination of its parent");
}
