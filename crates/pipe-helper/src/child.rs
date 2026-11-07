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
        if !self.stdout_has_data()? {
            return Ok(None);
        }
        fill_available(&mut self.stdout, &mut self.read_buf)?;
        self.take_buffered_frame()
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

#[derive(Clone, Copy, Debug, Default)]
pub struct SpawnOpts {
    /// Windows: `CREATE_NO_WINDOW`. Ignored on Unix.
    pub create_no_window: bool,
}

/// Finish `cmd` with piped stdin/stdout and inherited stderr, then spawn.
pub fn spawn_piped(cmd: &mut Command, opts: SpawnOpts, kind: &str) -> PipeResult<ChildIo> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    if opts.create_no_window {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
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
        if !stdout_ready(stdout)? {
            return Ok(());
        }
        let mut tmp = [0u8; 4096];
        match std::io::Read::read(stdout, &mut tmp) {
            Ok(0) => {
                // POLLHUP can be set together with the last bytes. Keep
                // whatever is already buffered so EXIT/HELLO_OK can parse.
                if buf.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "helper pipe closed",
                    ));
                }
                return Ok(());
            }
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) => return Err(e),
        }
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
            return Err(io::Error::last_os_error());
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
