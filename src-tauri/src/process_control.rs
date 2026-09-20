//! Child-process mechanics shared by every local probe: Windows console
//! suppression and whole-process-tree termination.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
#[cfg(windows)]
use std::process::Stdio;

/// Suppresses the console flash a detached child would otherwise show on
/// Windows; other platforms keep their spawn defaults.
pub(crate) fn suppress_window(command: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Kills one child together with any process it spawned: `kill` only reaches
/// the direct child, so Windows walks the tree with `taskkill /T`.
pub(crate) fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let mut command = Command::new("taskkill");
        command
            .args(["/PID", &pid, "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        suppress_window(&mut command);
        let _ = command.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
