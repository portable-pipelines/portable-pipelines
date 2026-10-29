//! SUBCOM — Author's Edition 1.1.11 p.534.
//!
//! Issue commands to a named subcommand environment (REXX ADDRESS-style).
//! Portable environments:
//!
//! - `COMMAND`, `CMD`, `SYSTEM`, `SH`, `SHELL`: the host shell (`cmd /C` on
//!   Windows, `sh -c` elsewhere), the same runner COMMAND uses.
//!
//! CMS, TSO, and other named environments are not implemented. Use PIPCMD
//! for pipeline commands.

use pipe_core::{StageCtx, StreamId};

pub fn subcom(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_subcom(&args) {
        Ok((env, initial)) => run_subcom(ctx, &env, initial),
        Err(e) => ctx.fail(format!("SUBCOM: {e}")),
    })
}

fn parse_subcom(args: &[String]) -> Result<(String, Option<String>), String> {
    if args.is_empty() {
        return Err("subcommand environment required".into());
    }
    let env = args[0].to_ascii_uppercase();
    if !is_host_shell(&env) {
        return Err(format!(
            "subcommand environment {} is not implemented",
            args[0]
        ));
    }
    let rest = args[1..].join(" ");
    let initial = if rest.trim().is_empty() {
        None
    } else {
        Some(rest)
    };
    Ok((env, initial))
}

fn is_host_shell(env: &str) -> bool {
    env == "COMMAND" || env == "CMD" || env == "SYSTEM" || env == "SH" || env == "SHELL"
}

fn run_subcom(ctx: &mut StageCtx, _env: &str, initial: Option<String>) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("SUBCOM: only the primary input may be connected");
            return;
        }
    }

    let mut cmds = Vec::new();
    if let Some(c) = initial {
        cmds.push(c);
    }
    if ctx.input_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        loop {
            match ctx.read() {
                Ok(Some(r)) => {
                    let line = r.to_string_lossy();
                    if line.trim().is_empty() {
                        continue;
                    }
                    cmds.push(line);
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
    }

    for cmd in cmds {
        if !crate::host_cmd::run_host_command(ctx, &cmd, "SUBCOM") {
            break;
        }
    }
}
