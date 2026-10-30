//! Spawn a language-stage child with framed stdin/stdout.

use std::io;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
#[cfg(unix)]
use std::time::{Duration, Instant};

use pipe_core::{PipeError, PipeResult};

use crate::protocol::{read_frame, write_frame, Frame};

pub struct ChildIo {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

impl ChildIo {
    pub fn write_frame(&mut self, typ: u8, payload: &[u8]) -> io::Result<()> {
        write_frame(&mut self.stdin, typ, payload)
    }

    pub fn read_frame(&mut self) -> io::Result<Frame> {
        read_frame(&mut self.stdout)
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
    })
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
