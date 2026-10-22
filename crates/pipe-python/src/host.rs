//! Spawn the interpreter and map protocol requests onto `StageCtx`.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use pipe_core::{PipeError, PipeResult, Record, StageCtx};

use crate::discover::{
    find_pipelines_parent, find_python, format_old_python_error, pythonpath_value,
    query_python_version,
};
use crate::protocol::{
    decode_i32_le, encode_err, encode_hello, encode_ok, read_frame, write_frame, EOF as EOF_TYPE,
    EXIT, FAIL, HELLO, HELLO_OK, OK, OUTPUT, PEEKTO, READTO, RECORD, SHORT,
};

struct ChildIo {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl ChildIo {
    fn write_frame(&mut self, typ: u8, payload: &[u8]) -> io::Result<()> {
        write_frame(&mut self.stdin, typ, payload)
    }

    fn read_frame(&mut self) -> io::Result<crate::protocol::Frame> {
        read_frame(&mut self.stdout)
    }

    fn terminate(mut self) {
        terminate_child(&mut self.child);
    }

    fn wait_exit(mut self) -> io::Result<std::process::ExitStatus> {
        drop(self.stdin);
        drop(self.stdout);
        self.child.wait()
    }
}

pub fn run_python_stage(ctx: &mut StageCtx, verb: &str, args: &[String]) -> PipeResult<()> {
    reject_secondary_streams(ctx)?;

    let path = PathBuf::from(verb);
    if !path.exists() {
        return Err(PipeError::io(format!(
            "Python script not found: {}",
            path.display()
        )));
    }

    let python = find_python().map_err(PipeError::stage)?;
    let version = query_python_version(&python).map_err(PipeError::stage)?;
    if !version.meets_min() {
        return Err(PipeError::stage(format_old_python_error(version)));
    }
    let pipelines_parent = find_pipelines_parent().map_err(PipeError::stage)?;

    let mut child = spawn_helper(&python, &pipelines_parent, &path, args)?;
    match drive(ctx, &mut child, &path, args) {
        Ok(()) => {
            let _ = child.wait_exit();
            Ok(())
        }
        Err(e) => {
            child.terminate();
            Err(e)
        }
    }
}

fn reject_secondary_streams(ctx: &StageCtx) -> PipeResult<()> {
    let extra_in = ctx.connected_inputs().iter().any(|s| s.0 > 0);
    let extra_out = ctx.connected_outputs().iter().any(|s| s.0 > 0);
    if extra_in || extra_out {
        return Err(PipeError::stage(
            "Python stages are single-stream in v1; a label connected a secondary stream",
        ));
    }
    Ok(())
}

fn spawn_helper(
    python: &Path,
    pipelines_parent: &Path,
    script: &Path,
    args: &[String],
) -> PipeResult<ChildIo> {
    let mut cmd = Command::new(python);
    cmd.arg("-u")
        .arg("-m")
        .arg("pipelines.stage")
        .arg("--")
        .arg(script)
        .args(args)
        .env("PYTHONPATH", pythonpath_value(pipelines_parent))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().map_err(|e| {
        PipeError::stage(format!("could not start Python {}: {e}", python.display()))
    })?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| PipeError::stage("Python stage stdin pipe missing"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PipeError::stage("Python stage stdout pipe missing"))?;
    Ok(ChildIo {
        child,
        stdin,
        stdout,
    })
}

fn drive(
    ctx: &mut StageCtx,
    child: &mut ChildIo,
    script: &Path,
    args: &[String],
) -> PipeResult<()> {
    let script_s = script.to_string_lossy().into_owned();
    child
        .write_frame(HELLO, &encode_hello(&script_s, args))
        .map_err(|e| protocol_io_error(child, e))?;

    let hello = child
        .read_frame()
        .map_err(|e| protocol_io_error(child, e))?;
    match hello.typ {
        HELLO_OK => {}
        FAIL => {
            return Err(fail_from_child(&hello.payload));
        }
        other => {
            return Err(PipeError::stage(format!(
                "Python stage sent unexpected hello reply 0x{other:02x}"
            )));
        }
    }

    loop {
        let frame = match child.read_frame() {
            Ok(f) => f,
            Err(e) => return Err(protocol_io_error(child, e)),
        };
        match frame.typ {
            READTO => handle_readto(ctx, child)?,
            PEEKTO => handle_peekto(ctx, child)?,
            OUTPUT => handle_output(ctx, child, &frame.payload)?,
            SHORT => handle_short(ctx, child)?,
            EXIT => {
                let rc = decode_i32_le(&frame.payload)
                    .map_err(|e| PipeError::stage(format!("Python EXIT payload: {e}")))?;
                ctx.set_return_code(rc);
                return Ok(());
            }
            FAIL => return Err(fail_from_child(&frame.payload)),
            other => {
                let msg = format!("unknown Python protocol type 0x{other:02x}");
                let _ = child.write_frame(crate::protocol::ERR, &encode_err(8, &msg));
                return Err(PipeError::stage(msg));
            }
        }
    }
}

fn handle_readto(ctx: &mut StageCtx, child: &mut ChildIo) -> PipeResult<()> {
    match ctx.read() {
        Ok(Some(r)) => child
            .write_frame(RECORD, r.as_bytes())
            .map_err(|e| protocol_io_error(child, e)),
        Ok(None) => child
            .write_frame(EOF_TYPE, &[])
            .map_err(|e| protocol_io_error(child, e)),
        Err(PipeError::Severed) => child
            .write_frame(EOF_TYPE, &[])
            .map_err(|e| protocol_io_error(child, e)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => Err(e),
    }
}

fn handle_peekto(ctx: &mut StageCtx, child: &mut ChildIo) -> PipeResult<()> {
    match peekto_record(ctx) {
        Ok(Some(r)) => child
            .write_frame(RECORD, r.as_bytes())
            .map_err(|e| protocol_io_error(child, e)),
        Ok(None) => child
            .write_frame(EOF_TYPE, &[])
            .map_err(|e| protocol_io_error(child, e)),
        Err(PipeError::Severed) => child
            .write_frame(EOF_TYPE, &[])
            .map_err(|e| protocol_io_error(child, e)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => Err(e),
    }
}

fn peekto_record(ctx: &mut StageCtx) -> PipeResult<Option<Record>> {
    let stream = ctx.selected_input();
    loop {
        ctx.wait_any_input(&[stream])?;
        if ctx.input_at_eof(stream) {
            return Ok(None);
        }
        if let Some(r) = ctx.peek() {
            return Ok(Some(r));
        }
        if !ctx.input_connected(stream) {
            return Ok(None);
        }
    }
}

fn handle_output(ctx: &mut StageCtx, child: &mut ChildIo, payload: &[u8]) -> PipeResult<()> {
    match ctx.write(Record::from(payload)) {
        Ok(()) => child
            .write_frame(OK, &encode_ok(0))
            .map_err(|e| protocol_io_error(child, e)),
        Err(PipeError::Severed) => child
            .write_frame(OK, &encode_ok(12))
            .map_err(|e| protocol_io_error(child, e)),
        Err(PipeError::Aborted) => Err(PipeError::Aborted),
        Err(e) => {
            let msg = e.to_string();
            let _ = child.write_frame(crate::protocol::ERR, &encode_err(8, &msg));
            Err(e)
        }
    }
}

fn handle_short(ctx: &mut StageCtx, child: &mut ChildIo) -> PipeResult<()> {
    ctx.short();
    child
        .write_frame(OK, &encode_ok(0))
        .map_err(|e| protocol_io_error(child, e))
}

fn fail_from_child(payload: &[u8]) -> PipeError {
    let msg = String::from_utf8_lossy(payload);
    let msg = msg.trim();
    if msg.is_empty() {
        PipeError::stage("Python stage failed")
    } else {
        PipeError::stage(msg.to_string())
    }
}

fn protocol_io_error(child: &mut ChildIo, err: io::Error) -> PipeError {
    match child.child.try_wait() {
        Ok(Some(status)) => {
            PipeError::stage(format!("Python stage exited unexpectedly ({status})"))
        }
        _ => {
            if err.kind() == io::ErrorKind::UnexpectedEof {
                PipeError::stage("Python stage exited unexpectedly")
            } else {
                PipeError::stage(format!("Python stage protocol I/O: {err}"))
            }
        }
    }
}

fn terminate_child(child: &mut Child) {
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
