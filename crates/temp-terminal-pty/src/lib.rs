//! Windows ConPTY and JobObject process backend for temp-terminal.
//!
//! Ported directly from Go `tempterm/internal/pty/pty_windows.go`.
//! Uses Windows PseudoConsole (HPCON) APIs and JobObjects for process grouping
//! and clean termination on exit or close.

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, ResumeThread, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("pty: ConPTY creation failed: {0}")]
    CreatePseudoConsole(io::Error),
    #[error("pty: pipe creation failed: {0}")]
    CreatePipe(io::Error),
    #[error("pty: attribute list error: {0}")]
    AttributeList(io::Error),
    #[error("pty: process spawn failed: {0}")]
    Spawn(io::Error),
    #[error("pty: console is closed")]
    ConsoleClosed,
    #[error("pty: resize failed: {0}")]
    Resize(io::Error),
    #[error("pty: io error: {0}")]
    Io(#[from] io::Error),
}

/// Options to start a process in a pseudo-console.
/// Matches Go `pty.Options`.
#[derive(Debug, Clone, Default)]
pub struct PtyOptions {
    pub path: String,
    pub args: Vec<String>,
    pub dir: String,
    pub env: Vec<String>,
    pub cols: u16,
    pub rows: u16,
}

struct Inner {
    console: Mutex<Option<HPCON>>,
    process: Mutex<Option<HANDLE>>,
    job: Mutex<Option<HANDLE>>,
    pid: AtomicU32,
    exit_code: AtomicI32,
    exited: AtomicBool,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(h) = self.job.lock().unwrap().take() {
            unsafe { CloseHandle(h) };
        }
        if let Some(p) = self.process.lock().unwrap().take() {
            unsafe { CloseHandle(p) };
        }
    }
}

/// A running program attached to a Windows ConPTY pseudo-console and JobObject.
/// Matches Go `pty.Proc`.
#[derive(Clone)]
pub struct PtyProc {
    inner: Arc<Inner>,
    in_write: Arc<Mutex<Option<OwnedHandle>>>,
    out_read: Arc<Mutex<Option<OwnedHandle>>>,
}

impl PtyProc {
    /// Starts a program in a new pseudo-console.
    /// Corresponds to Go `pty.Start(o Options) (*Proc, error)`.
    pub fn start(options: PtyOptions) -> Result<Self, PtyError> {
        let cols = if options.cols == 0 { 120 } else { options.cols };
        let rows = if options.rows == 0 { 36 } else { options.rows };

        // 1. Create pipes for ConPTY:
        // in_read (console reads) <-> in_write (app writes)
        // out_read (app reads) <-> out_write (console writes)
        let mut in_read: HANDLE = INVALID_HANDLE_VALUE;
        let mut in_write: HANDLE = INVALID_HANDLE_VALUE;
        let mut out_read: HANDLE = INVALID_HANDLE_VALUE;
        let mut out_write: HANDLE = INVALID_HANDLE_VALUE;

        unsafe {
            if CreatePipe(&mut in_read, &mut in_write, std::ptr::null_mut(), 0) == FALSE {
                return Err(PtyError::CreatePipe(io::Error::last_os_error()));
            }
            if CreatePipe(&mut out_read, &mut out_write, std::ptr::null_mut(), 0) == FALSE {
                CloseHandle(in_read);
                CloseHandle(in_write);
                return Err(PtyError::CreatePipe(io::Error::last_os_error()));
            }
        }

        // 2. Create the PseudoConsole
        let size = COORD {
            X: cols.min(0x7fff) as i16,
            Y: rows.min(0x7fff) as i16,
        };

        let mut hpcon: HPCON = 0;
        let hr = unsafe { CreatePseudoConsole(size, in_read, out_write, 0, &mut hpcon) };

        // Console duplicates handles, close our copies of the console's ends
        unsafe {
            CloseHandle(in_read);
            CloseHandle(out_write);
        }

        if hr != 0 || hpcon == 0 {
            unsafe {
                CloseHandle(in_write);
                CloseHandle(out_read);
            }
            return Err(PtyError::CreatePseudoConsole(io::Error::from_raw_os_error(
                hr,
            )));
        }

        let inner = Arc::new(Inner {
            console: Mutex::new(Some(hpcon)),
            process: Mutex::new(None),
            job: Mutex::new(None),
            pid: AtomicU32::new(0),
            exit_code: AtomicI32::new(-1),
            exited: AtomicBool::new(false),
        });

        // 3. Spawn process
        let (proc_handle, job_handle, pid) = match spawn_process(hpcon, &options) {
            Ok(res) => res,
            Err(e) => {
                unsafe {
                    ClosePseudoConsole(hpcon);
                    CloseHandle(in_write);
                    CloseHandle(out_read);
                }
                return Err(e);
            }
        };

        *inner.process.lock().unwrap() = Some(proc_handle);
        *inner.job.lock().unwrap() = job_handle;
        inner.pid.store(pid, Ordering::SeqCst);

        let pty = Self {
            inner: Arc::clone(&inner),
            in_write: Arc::new(Mutex::new(Some(unsafe {
                OwnedHandle::from_raw_handle(in_write as RawHandle)
            }))),
            out_read: Arc::new(Mutex::new(Some(unsafe {
                OwnedHandle::from_raw_handle(out_read as RawHandle)
            }))),
        };

        // 4. Background thread to wait on process exit
        let wait_inner = Arc::clone(&inner);
        let wait_in = Arc::clone(&pty.in_write);
        let wait_out = Arc::clone(&pty.out_read);

        thread::Builder::new()
            .name(format!("conpty-wait-{}", pid))
            .spawn(move || {
                let process_handle = {
                    let guard = wait_inner.process.lock().unwrap();
                    guard.unwrap_or(INVALID_HANDLE_VALUE)
                };

                if process_handle != INVALID_HANDLE_VALUE {
                    unsafe {
                        WaitForSingleObject(process_handle, INFINITE);
                        let mut code: u32 = 0;
                        if GetExitCodeProcess(process_handle, &mut code) != FALSE {
                            wait_inner.exit_code.store(code as i32, Ordering::SeqCst);
                        }
                    }
                }

                wait_inner.exited.store(true, Ordering::SeqCst);

                // Close in pipe
                wait_in.lock().unwrap().take();

                // Close pseudo-console
                let console_opt = wait_inner.console.lock().unwrap().take();
                if let Some(c) = console_opt {
                    unsafe {
                        ClosePseudoConsole(c);
                    }
                }

                // Close out pipe
                wait_out.lock().unwrap().take();
            })
            .expect("spawn conpty wait thread");

        Ok(pty)
    }

    pub fn pid(&self) -> u32 {
        self.inner.pid.load(Ordering::SeqCst)
    }

    pub fn is_alive(&self) -> bool {
        if self.inner.exited.load(Ordering::SeqCst) {
            return false;
        }
        let guard = self.inner.process.lock().unwrap();
        if let Some(p) = *guard {
            let mut code: u32 = 0;
            let res = unsafe { GetExitCodeProcess(p, &mut code) };
            if res != FALSE && code == 259 {
                // STILL_ACTIVE is 259
                return true;
            }
        }
        false
    }

    pub fn exit_code(&self) -> Option<i32> {
        if self.inner.exited.load(Ordering::SeqCst) {
            Some(self.inner.exit_code.load(Ordering::SeqCst))
        } else {
            None
        }
    }

    /// Resizes the pseudo-console.
    /// Matches Go `pty.Proc.Resize(cols, rows, cellW, cellH)`.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        let guard = self.inner.console.lock().unwrap();
        let Some(c) = *guard else {
            return Err(PtyError::ConsoleClosed);
        };

        let size = COORD {
            X: cols.clamp(1, 0x7fff) as i16,
            Y: rows.clamp(1, 0x7fff) as i16,
        };

        let hr = unsafe { ResizePseudoConsole(c, size) };
        if hr != 0 {
            Err(PtyError::Resize(io::Error::from_raw_os_error(hr)))
        } else {
            Ok(())
        }
    }

    /// Writes raw bytes to the program's standard input.
    pub fn write_in(&self, buf: &[u8]) -> io::Result<usize> {
        let guard = self.in_write.lock().unwrap();
        let Some(ref handle) = *guard else {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "pipe closed"));
        };

        let mut written: u32 = 0;
        let success = unsafe {
            WriteFile(
                handle.as_raw_handle() as HANDLE,
                buf.as_ptr(),
                buf.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };

        if success == FALSE {
            Err(io::Error::last_os_error())
        } else {
            Ok(written as usize)
        }
    }

    /// Reads raw output from the program's standard output.
    pub fn read_out(&self, buf: &mut [u8]) -> io::Result<usize> {
        let handle_raw = {
            let guard = self.out_read.lock().unwrap();
            let Some(ref handle) = *guard else {
                return Ok(0); // EOF
            };
            handle.as_raw_handle() as HANDLE
        };

        let mut bytes_read: u32 = 0;
        let success = unsafe {
            ReadFile(
                handle_raw,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };

        if success == FALSE {
            let err = io::Error::last_os_error();
            // ERROR_BROKEN_PIPE is 109
            if err.raw_os_error() == Some(109) {
                Ok(0) // Clean EOF
            } else {
                Err(err)
            }
        } else {
            Ok(bytes_read as usize)
        }
    }

    /// Kills the program and everything in its JobObject.
    /// Matches Go `pty.Proc.Kill()`.
    pub fn kill(&self) {
        let job = *self.inner.job.lock().unwrap();
        if let Some(j) = job {
            unsafe {
                if TerminateJobObject(j, 1) != FALSE {
                    return;
                }
            }
        }

        if !self.inner.exited.load(Ordering::SeqCst) {
            let proc = *self.inner.process.lock().unwrap();
            if let Some(p) = proc {
                unsafe {
                    TerminateProcess(p, 1);
                }
            }
        }
    }

    /// Gracefully stops the program and everything in its JobObject.
    /// Closes console input/hangup, waits up to `grace`, and kills on deadline.
    /// Matches Go `pty.Proc.Stop(grace)`.
    pub fn stop(&self, grace: Duration) {
        // Drop in_write handle to signal EOF
        self.in_write.lock().unwrap().take();

        let start = Instant::now();
        while start.elapsed() < grace {
            if self.inner.exited.load(Ordering::SeqCst) || self.job_active_count() == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }

        self.kill();

        // Close and discard job object
        if let Some(j) = self.inner.job.lock().unwrap().take() {
            unsafe {
                CloseHandle(j);
            }
        }
    }

    /// Queries the number of active processes in the JobObject.
    /// Matches Go `pty.Proc.jobActive()`.
    pub fn job_active_count(&self) -> usize {
        let guard = self.inner.job.lock().unwrap();
        let Some(j) = *guard else {
            return if self.is_alive() { 1 } else { 0 };
        };

        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        let res = unsafe {
            QueryInformationJobObject(
                j,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut _,
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };

        if res == FALSE {
            0
        } else {
            info.ActiveProcesses as usize
        }
    }

    /// Returns process IDs attached to the JobObject.
    pub fn job_pids(&self) -> Vec<u32> {
        let guard = self.inner.job.lock().unwrap();
        let Some(j) = *guard else {
            return if self.is_alive() {
                vec![self.inner.pid.load(Ordering::SeqCst)]
            } else {
                vec![]
            };
        };

        // Buffer for 256 PIDs
        let buffer_size = std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() + 256 * 8;
        let mut buffer = vec![0u8; buffer_size];
        let p_list = buffer.as_mut_ptr() as *mut JOBOBJECT_BASIC_PROCESS_ID_LIST;

        let res = unsafe {
            QueryInformationJobObject(
                j,
                JobObjectBasicProcessIdList,
                p_list as *mut _,
                buffer_size as u32,
                std::ptr::null_mut(),
            )
        };

        if res == FALSE {
            return vec![];
        }

        unsafe {
            let count = (*p_list).NumberOfProcessIdsInList as usize;
            let slice =
                std::slice::from_raw_parts((*p_list).ProcessIdList.as_ptr(), count.min(256));
            slice.iter().map(|&pid| pid as u32).collect()
        }
    }
}

// -----------------------------------------------------------------------------
// Helper routines
// -----------------------------------------------------------------------------

fn spawn_process(
    hpcon: HPCON,
    options: &PtyOptions,
) -> Result<(HANDLE, Option<HANDLE>, u32), PtyError> {
    unsafe {
        // 1. Initialize Thread Attribute List
        let mut size: usize = 0;
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
        let mut attr_list = vec![0u8; size];
        let lp_attr_list = attr_list.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;

        if InitializeProcThreadAttributeList(lp_attr_list, 1, 0, &mut size) == FALSE {
            return Err(PtyError::AttributeList(io::Error::last_os_error()));
        }

        struct AttrGuard(LPPROC_THREAD_ATTRIBUTE_LIST);
        impl Drop for AttrGuard {
            fn drop(&mut self) {
                unsafe {
                    DeleteProcThreadAttributeList(self.0);
                }
            }
        }
        let _guard = AttrGuard(lp_attr_list);

        if UpdateProcThreadAttribute(
            lp_attr_list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            hpcon as *mut _,
            std::mem::size_of::<HPCON>(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == FALSE
        {
            return Err(PtyError::AttributeList(io::Error::last_os_error()));
        }

        // 2. Prepare STARTUPINFOEX
        let mut si: STARTUPINFOEXW = std::mem::zeroed();
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        si.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        si.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
        si.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
        si.lpAttributeList = lp_attr_list;

        // 3. Command line encoding
        let cmd = command_line(&options.path, &options.args);
        let mut cmd_utf16: Vec<u16> = OsStr::new(&cmd).encode_wide().chain(Some(0)).collect();

        // 4. Working directory
        let dir_utf16: Option<Vec<u16>> = if !options.dir.is_empty() {
            Some(
                OsStr::new(&options.dir)
                    .encode_wide()
                    .chain(Some(0))
                    .collect(),
            )
        } else {
            None
        };
        let p_dir = dir_utf16
            .as_ref()
            .map(|v| v.as_ptr())
            .unwrap_or(std::ptr::null());

        // 5. Environment block (double NUL-terminated UTF-16)
        let env_block = build_environment_block(&options.env);
        let p_env = if !options.env.is_empty() {
            env_block.as_ptr() as *const _
        } else {
            std::ptr::null()
        };

        // 6. Spawn suspended
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED;

        let ok = CreateProcessW(
            std::ptr::null(),
            cmd_utf16.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            FALSE,
            flags,
            p_env,
            p_dir,
            &si.StartupInfo as *const _ as *mut _,
            &mut pi,
        );

        if ok == FALSE {
            return Err(PtyError::Spawn(io::Error::last_os_error()));
        }

        // 7. Put into JobObject
        let job = create_job_object(pi.hProcess);

        // 8. Resume main thread
        ResumeThread(pi.hThread);
        CloseHandle(pi.hThread);

        Ok((pi.hProcess, job, pi.dwProcessId))
    }
}

fn create_job_object(process: HANDLE) -> Option<HANDLE> {
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job.is_null() || job == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let res = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if res != FALSE {
            AssignProcessToJobObject(job, process);
        }

        Some(job)
    }
}

pub fn command_line(path: &str, args: &[String]) -> String {
    let mut parts = vec![escape_arg(path)];
    let rest = if !args.is_empty() {
        let first = &args[0];
        let prog_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        if first.eq_ignore_ascii_case(prog_name)
            || (is_cmd_name(path) && first.eq_ignore_ascii_case("cmd"))
            || (is_cmd_name(path) && first.eq_ignore_ascii_case("cmd.exe"))
            || first.eq_ignore_ascii_case(path)
        {
            &args[1..]
        } else {
            args
        }
    } else {
        &[]
    };

    if is_cmd_name(path) {
        for (i, a) in rest.iter().enumerate() {
            if a.eq_ignore_ascii_case("/c") || a.eq_ignore_ascii_case("/k") {
                let flags = &rest[..i];
                if !flags.iter().any(|f| f.eq_ignore_ascii_case("/s")) {
                    parts.push("/s".to_string());
                }
                for f in &rest[..=i] {
                    parts.push(escape_arg(f));
                }
                return format!("{} \"{}\"", parts.join(" "), rest[i + 1..].join(" "));
            }
        }
    }

    for a in rest {
        parts.push(escape_arg(a));
    }
    parts.join(" ")
}

fn is_cmd_name(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.eq_ignore_ascii_case("cmd.exe") || name.eq_ignore_ascii_case("cmd")
}

pub fn escape_arg(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    if !s.contains([' ', '\t', '\"']) {
        return s.to_string();
    }

    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in s.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes + 1));
                slashes = 0;
                out.push('"');
            }
            _ => {
                slashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(slashes));
    out.push('"');
    out
}

fn build_environment_block(env: &[String]) -> Vec<u16> {
    let mut block: Vec<u16> = Vec::new();
    for kv in env {
        for u in OsStr::new(kv).encode_wide() {
            block.push(u);
        }
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_line_with_program_name_in_args() {
        let cmd = command_line("cmd.exe", &["cmd".to_string(), "/k".to_string(), "agy".to_string()]);
        assert_eq!(cmd, "cmd.exe /s /k \"agy\"");
    }

    #[test]
    fn test_command_line_without_program_name_in_args() {
        let cmd = command_line("cmd.exe", &["/k".to_string(), "agy".to_string()]);
        assert_eq!(cmd, "cmd.exe /s /k \"agy\"");
    }

    #[test]
    fn test_command_line_powershell_args() {
        let cmd = command_line("powershell.exe", &["-NoLogo".to_string()]);
        assert_eq!(cmd, "powershell.exe -NoLogo");
    }
}

