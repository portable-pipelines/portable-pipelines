//! Host shell used by COMMAND and SUBCOM.
//!
//! Windows: `cmd /C` plus the command text via [`std::os::windows::process::CommandExt::raw_arg`].
//! Rust’s argv quoting would wrap a command that contains spaces as one `"..."` token.
//! `cmd /C` then applies its own quote-stripping, which breaks nested quotes (PowerShell
//! `-Command "..."`, `type "path with spaces"`) and is not how a user types `cmd /C`.
//! Unix is unchanged: `sh -c` with the command as one argument.

use std::io::BufRead;
use std::process::{Command, Output};

use pipe_core::{Record, StageCtx};

pub fn run_host_command(ctx: &mut StageCtx, cmd: &str, verb: &str) -> bool {
    match spawn_host_shell(cmd) {
        Ok(out) => write_stdout_records(ctx, &out.stdout, verb),
        Err(e) => {
            ctx.fail(format!("{verb}: {e}"));
            false
        }
    }
}

fn spawn_host_shell(cmd: &str) -> std::io::Result<Output> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("cmd").arg("/C").raw_arg(cmd).output()
    }
    #[cfg(not(windows))]
    {
        Command::new("sh").args(["-c", cmd]).output()
    }
}

fn write_stdout_records(ctx: &mut StageCtx, stdout: &[u8], verb: &str) -> bool {
    let mut cursor = std::io::Cursor::new(stdout);
    loop {
        match read_host_record(&mut cursor) {
            Ok(Some(line)) => {
                if ctx.write(Record::from(line)).is_err() {
                    return false;
                }
            }
            Ok(None) => return true,
            Err(e) => {
                ctx.fail(format!("{verb}: {e}"));
                return false;
            }
        }
    }
}

fn read_host_record<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
    Ok(Some(buf))
}
