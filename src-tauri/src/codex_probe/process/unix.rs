use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

/// Closing the sole writer (including OS cleanup after a crash) wakes a
/// guardian which kills the process group. No PID polling or parent exit
/// handler is required. The guardian only calls async-signal-safe libc after fork.
pub(crate) struct Lifetime(OwnedFd);
impl Drop for Lifetime {
    fn drop(&mut self) {
        // shutdown wakes the guardian even if another thread forked while
        // this descriptor existed; the CLOEXEC copies cannot extend life.
        unsafe { libc::shutdown(self.0.as_raw_fd(), libc::SHUT_RDWR); }
    }
}

pub(crate) fn spawn(command: &mut Command) -> io::Result<(Child, Lifetime)> {
    let (reader, writer) = UnixStream::pair()?;
    let read_fd = reader.as_raw_fd();
    let write_fd = writer.as_raw_fd();
    let max_fd = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) }.max(1024) as i32;
    // SAFETY: the child hook uses only async-signal-safe libc operations;
    // it does not allocate, log, or acquire locks after fork.
    unsafe {
        command.pre_exec(move || {
            if libc::setpgid(0, 0) != 0 { return Err(io::Error::last_os_error()); }
            let group = libc::getpid();
            libc::close(write_fd);
            let guardian = libc::fork();
            if guardian < 0 { return Err(io::Error::last_os_error()); }
            if guardian == 0 { guard(group, read_fd, max_fd); }
            libc::close(read_fd);
            Ok(())
        });
    }
    let child = command.spawn()?;
    drop(reader);
    use std::os::fd::IntoRawFd;
    let writer = unsafe { OwnedFd::from_raw_fd(writer.into_raw_fd()) };
    Ok((child, Lifetime(writer)))
}

unsafe fn guard(group: i32, read_fd: i32, max_fd: i32) -> ! {
    // Separate the guardian from the group it owns, and close inherited
    // stdout/stderr plus std's exec-status pipe before waiting for EOF.
    if libc::setsid() < 0 { libc::kill(-group, libc::SIGKILL); libc::_exit(1); }
    for fd in 0..max_fd { if fd != read_fd { libc::close(fd); } }
    let mut byte = 0u8;
    loop {
        let read = libc::read(read_fd, &mut byte as *mut _ as *mut _, 1);
        if read == 0 { break; }
        if read < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) { break; }
    }
    libc::kill(-group, libc::SIGKILL);
    libc::_exit(0);
}
