//! Spawn a language-stage child with framed stdin/stdout.

use std::io;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
#[cfg(unix)]
use std::time::{Duration, Instant};

use pipe_core::{PipeError, PipeResult};

use crate::protocol::{write_frame, Frame};

pub struct ChildIo {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    read_buf: Vec<u8>,
}

impl ChildIo {
    pub fn write_frame(&mut self, typ: u8, payload: &[u8]) -> io::Result<()> {
        write_frame(&mut self.stdin, typ, payload)
    }

    pub fn read_frame(&mut self) -> io::Result<Frame> {
        loop {
            if let Some(frame) = self.take_buffered_frame()? {
                return Ok(frame);
            }
            fill_blocking(&mut self.stdout, &mut self.read_buf)?;
        }
    }

    /// Non-blocking frame read. `Ok(None)` means no complete frame yet.
    pub fn try_read_frame(&mut self) -> io::Result<Option<Frame>> {
        if let Some(frame) = self.take_buffered_frame()? {
            return Ok(Some(frame));
        }
        match self.stdout_has_data() {
            Ok(false) => return Ok(None),
            Ok(true) => {}
            Err(e) if is_closed_pipe(&e) => {}
            Err(e) => return Err(e),
        }
        match fill_available(&mut self.stdout, &mut self.read_buf) {
            Ok(()) => {}
            Err(e) if is_closed_pipe(&e) => {
                if let Some(frame) = self.take_buffered_frame()? {
                    return Ok(Some(frame));
                }
                return Err(e);
            }
            Err(e) => return Err(e),
        }
        self.take_buffered_frame()
    }

    /// True after the helper process has been reaped (or is immediately reapable).
    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// Last-chance read when the helper has already exited. Ignores PeekNamedPipe
    /// so a Windows broken pipe still yields a buffered EXIT frame.
    pub fn try_read_after_exit(&mut self) -> io::Result<Option<Frame>> {
        if let Some(frame) = self.take_buffered_frame()? {
            return Ok(Some(frame));
        }
        let mut tmp = [0u8; 4096];
        match std::io::Read::read(&mut self.stdout, &mut tmp) {
            Ok(0) => self.take_buffered_frame(),
            Ok(n) => {
                self.read_buf.extend_from_slice(&tmp[..n]);
                self.take_buffered_frame()
            }
            Err(e) if is_closed_pipe(&e) => self.take_buffered_frame(),
            Err(e) => Err(e),
        }
    }

    fn take_buffered_frame(&mut self) -> io::Result<Option<Frame>> {
        if self.read_buf.len() < 5 {
            return Ok(None);
        }
        let len = u32::from_le_bytes(self.read_buf[0..4].try_into().unwrap()) as usize;
        if len as u32 > crate::protocol::MAX_PAYLOAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stage payload exceeds 16 MiB",
            ));
        }
        let total = 5 + len;
        if self.read_buf.len() < total {
            return Ok(None);
        }
        let typ = self.read_buf[4];
        let payload = self.read_buf[5..total].to_vec();
        self.read_buf.drain(..total);
        Ok(Some(Frame { typ, payload }))
    }

    fn stdout_has_data(&self) -> io::Result<bool> {
        stdout_ready(&self.stdout)
    }

    pub fn terminate(mut self) {
        terminate_child(&mut self.child);
    }

    pub fn wait_exit(mut self) -> io::Result<std::process::ExitStatus> {
        drop(self.stdin);
        drop(self.stdout);
        self.child.wait()
    }
}

/// Windows `CREATE_NO_WINDOW`. Console-subsystem children of a
/// Windows-subsystem parent (`pipe-gui`) otherwise flash a cmd window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, Default)]
pub struct SpawnOpts {
    /// Ignored: piped protocol helpers always hide the console on Windows.
    /// Kept so existing callers compile.
    pub create_no_window: bool,
}

/// Do not allocate a console window for this child. No-op on Unix.
///
/// Stdin/stdout/stderr redirections still apply. A GUI child (notepad, etc.)
/// still shows its window. Process group and kill behavior are unchanged.
pub fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

/// Finish `cmd` with piped stdin/stdout and inherited stderr, then spawn.
pub fn spawn_piped(cmd: &mut Command, opts: SpawnOpts, kind: &str) -> PipeResult<ChildIo> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    apply_no_window(cmd);
    let _ = opts;

    let mut child = cmd
        .spawn()
        .map_err(|e| PipeError::stage(format!("could not start {kind}: {e}")))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| PipeError::stage(format!("{kind} stdin pipe missing")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PipeError::stage(format!("{kind} stdout pipe missing")))?;
    Ok(ChildIo {
        child,
        stdin,
        stdout,
        read_buf: Vec::new(),
    })
}

fn fill_blocking(stdout: &mut ChildStdout, buf: &mut Vec<u8>) -> io::Result<()> {
    let mut tmp = [0u8; 4096];
    let n = std::io::Read::read(stdout, &mut tmp)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "helper pipe closed",
        ));
    }
    buf.extend_from_slice(&tmp[..n]);
    Ok(())
}

fn fill_available(stdout: &mut ChildStdout, buf: &mut Vec<u8>) -> io::Result<()> {
    loop {
        match stdout_ready(stdout) {
            Ok(false) => return Ok(()),
            Ok(true) => {}
            Err(e) if is_closed_pipe(&e) => {
                return eof_or_keep_buf(buf);
            }
            Err(e) => return Err(e),
        }
        let mut tmp = [0u8; 4096];
        match std::io::Read::read(stdout, &mut tmp) {
            Ok(0) => return eof_or_keep_buf(buf),
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if is_closed_pipe(&e) => return eof_or_keep_buf(buf),
            Err(e) => return Err(e),
        }
    }
}

fn eof_or_keep_buf(buf: &[u8]) -> io::Result<()> {
    if buf.is_empty() {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "helper pipe closed",
        ))
    } else {
        // Last bytes arrived with hangup/broken-pipe. Caller parses EXIT.
        Ok(())
    }
}

fn is_closed_pipe(err: &io::Error) -> bool {
    if matches!(
        err.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
    ) {
        return true;
    }
    #[cfg(windows)]
    {
        // ERROR_BROKEN_PIPE 109, ERROR_NO_DATA 232, ERROR_PIPE_NOT_CONNECTED 233.
        matches!(err.raw_os_error(), Some(109 | 232 | 233))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn stdout_ready(stdout: &ChildStdout) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = stdout.as_raw_fd();
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, 0) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(rc > 0 && pfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let handle = stdout.as_raw_handle();
        let mut avail: u32 = 0;
        let ok = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                handle as _,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut avail,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            // Writer closed: PeekNamedPipe fails instead of avail=0. Treat as
            // readable so Read can drain the last EXIT/HELLO_OK frame.
            if is_closed_pipe(&err) {
                return Ok(true);
            }
            return Err(err);
        }
        Ok(avail > 0)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = stdout;
        Ok(true)
    }
}

pub fn protocol_io_error(child: &mut ChildIo, err: io::Error, kind: &str) -> PipeError {
    match child.child.try_wait() {
        Ok(Some(status)) => PipeError::stage(format!("{kind} exited unexpectedly ({status})")),
        _ => {
            if err.kind() == io::ErrorKind::UnexpectedEof {
                PipeError::stage(format!("{kind} exited unexpectedly"))
            } else {
                PipeError::stage(format!("{kind} protocol I/O: {err}"))
            }
        }
    }
}

pub fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        unsafe {
            libc_kill(pid, 15);
        }
        let deadline = Instant::now() + Duration::from_millis(150);
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
unsafe fn libc_kill(pid: u32, sig: i32) {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    let _ = kill(pid as i32, sig);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::Command;

    fn echo_command(text: &str) -> Command {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg(format!("echo {text}"));
            cmd
        }
        #[cfg(not(windows))]
        {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", &format!("echo {text}")]);
            cmd
        }
    }

    #[test]
    fn create_no_window_matches_winbase() {
        assert_eq!(CREATE_NO_WINDOW, 0x0800_0000);
    }

    #[test]
    fn apply_no_window_still_captures_stdout() {
        let mut cmd = echo_command("hello-no-window");
        apply_no_window(&mut cmd);
        let out = cmd.output().expect("echo");
        assert!(out.status.success(), "{:?}", out.status);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("hello-no-window"),
            "stdout={stdout:?} stderr={:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn spawn_piped_captures_child_stdout() {
        let mut cmd = echo_command("spawn-piped-ok");
        let mut io = spawn_piped(&mut cmd, SpawnOpts::default(), "echo").expect("spawn");
        let mut buf = Vec::new();
        io.stdout.read_to_end(&mut buf).expect("read");
        let status = io.wait_exit().expect("wait");
        assert!(status.success(), "{status:?}");
        let stdout = String::from_utf8_lossy(&buf);
        assert!(stdout.contains("spawn-piped-ok"), "stdout={stdout:?}");
    }

    #[test]
    fn spawn_piped_terminate_still_kills() {
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        let mut io = spawn_piped(&mut cmd, SpawnOpts::default(), "sleep").expect("spawn");
        terminate_child(&mut io.child);
        let status = io.child.try_wait().expect("status after terminate");
        assert!(status.is_some(), "child should have been reaped");
        assert!(
            !status.unwrap().success(),
            "expected killed child, got {status:?}"
        );
    }
}
