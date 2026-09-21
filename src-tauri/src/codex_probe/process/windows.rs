use std::io;
use std::os::windows::{io::AsRawHandle, process::CommandExt};
use std::process::{Child, Command};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};

pub(crate) struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
}

/// The child cannot execute or spawn descendants until it belongs to the
/// job. The non-inherited job handle belongs only to the application; OS
/// handle cleanup on a crash terminates the entire probe tree.
pub(crate) fn spawn(command: &mut Command) -> io::Result<(Child, Handle)> {
    let job = create_job()?;
    command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    let mut child = command.spawn()?;
    let attached = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) };
    let result = if attached == 0 { Err(io::Error::last_os_error()) }
        else { resume(child.id()) };
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok((child, job))
}

fn create_job() -> io::Result<Handle> {
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() { return Err(io::Error::last_os_error()); }
    let job = Handle(handle);
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let result = unsafe { SetInformationJobObject(job.0, JobObjectExtendedLimitInformation,
        &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32) };
    if result == 0 { return Err(io::Error::last_os_error()); }
    Ok(job)
}

fn resume(pid: u32) -> io::Result<()> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE { return Err(io::Error::last_os_error()); }
    let snapshot = Handle(snapshot);
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of_val(&entry) as u32;
    let mut present = unsafe { Thread32First(snapshot.0, &mut entry) };
    while present != 0 {
        if entry.th32OwnerProcessID == pid {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() { return Err(io::Error::last_os_error()); }
            let thread = Handle(thread);
            if unsafe { ResumeThread(thread.0) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            return Ok(());
        }
        present = unsafe { Thread32Next(snapshot.0, &mut entry) };
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "找不到检测进程主线程"))
}
