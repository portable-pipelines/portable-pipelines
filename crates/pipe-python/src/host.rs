//! Spawn the interpreter and map protocol requests onto `StageCtx`.

use std::path::{Path, PathBuf};
use std::process::Command;

use pipe_core::{PipeError, PipeResult, StageCtx};
use pipe_helper::protocol::encode_hello;
use pipe_helper::{drive, spawn_piped, SpawnOpts};

use crate::discover::{
    find_pipelines_parent, find_python, format_old_python_error, pythonpath_value,
    query_python_version,
};

const KIND: &str = "Python stage";

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
    match drive(
        ctx,
        &mut child,
        KIND,
        &encode_hello(&path.to_string_lossy(), args),
    ) {
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
) -> PipeResult<pipe_helper::ChildIo> {
    let mut cmd = Command::new(python);
    cmd.arg("-u")
        .arg("-m")
        .arg("pipelines.stage")
        .arg("--")
        .arg(script)
        .args(args)
        .env("PYTHONPATH", pythonpath_value(pipelines_parent));
    spawn_piped(&mut cmd, SpawnOpts::default(), KIND)
}
