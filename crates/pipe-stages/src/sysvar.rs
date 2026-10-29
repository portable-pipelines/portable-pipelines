//! SYSVAR — Author's Edition 1.1.11 p.541.
//!
//! Write host or system variables into the pipeline. Must be first.
//!
//! With no operands, writes one `NAME value` record per process environment
//! variable, sorted by name. Named operands write one record each: the value
//! of that variable. Computed names (not CMS sysvars): `OS`, `ARCH`, `PID`,
//! `CWD`, `HOSTNAME`, `USER`, `HOME`, `PATH`, `TEMP`. Other names are looked
//! up in the environment (exact, then ASCII-uppercase). An unset name writes
//! an empty record.

use std::env;

use pipe_core::{Record, StageCtx, StreamId};

pub fn sysvar(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        for stream in ctx.connected_inputs() {
            ctx.fail(format!(
                "SYSVAR: input stream {} may not be connected (must be first)",
                stream.0
            ));
            return;
        }
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        ctx.select_output(StreamId::PRIMARY);
        if args.is_empty() {
            write_all_env(ctx);
            return;
        }
        for name in &args {
            let value = lookup_sysvar(name);
            if ctx.write(Record::from(value)).is_err() {
                return;
            }
        }
    })
}

fn write_all_env(ctx: &mut StageCtx) {
    let mut pairs: Vec<(String, String)> = env::vars().collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, value) in pairs {
        let line = format!("{name} {value}");
        if ctx.write(Record::from(line)).is_err() {
            return;
        }
    }
}

pub fn lookup_sysvar(name: &str) -> String {
    let u = name.to_ascii_uppercase();
    match u.as_str() {
        "OS" => env::consts::OS.to_string(),
        "ARCH" => env::consts::ARCH.to_string(),
        "PID" => std::process::id().to_string(),
        "CWD" | "PWD" => env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        "HOSTNAME" => env_first(&["HOSTNAME", "COMPUTERNAME"]),
        "USER" => env_first(&["USER", "USERNAME", "LOGNAME"]),
        "HOME" => env_first(&["HOME", "USERPROFILE"]),
        "PATH" => env::var("PATH").unwrap_or_default(),
        "TEMP" | "TMP" => env_first(&["TEMP", "TMP", "TMPDIR"]),
        _ => env::var(name).or_else(|_| env::var(&u)).unwrap_or_default(),
    }
}

fn env_first(keys: &[&str]) -> String {
    for k in keys {
        if let Ok(v) = env::var(k) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_and_arch_are_host_consts() {
        assert_eq!(lookup_sysvar("OS"), env::consts::OS);
        assert_eq!(lookup_sysvar("os"), env::consts::OS);
        assert_eq!(lookup_sysvar("ARCH"), env::consts::ARCH);
        assert_eq!(lookup_sysvar("PID"), std::process::id().to_string());
    }

    #[test]
    fn unknown_name_is_empty() {
        assert_eq!(lookup_sysvar("PIPE_SYSVAR_NO_SUCH_VARIABLE_xyz"), "");
    }
}
