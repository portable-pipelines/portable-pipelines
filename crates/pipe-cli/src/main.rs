//! Cross-platform Portable Pipelines CLI.
//!
//! Usage:
//!   pipe "literal hello| console"
//!   pipe "(TRACE) literal hello| console"
//!   pipe --trace "literal hello| console"
//!   pipe (ENDCHAR ?) literal a | L: locate /x/ | console ? L: | console

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    pipe_rexx::install_hook();
    pipe_python::install_hook();

    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "Portable Pipelines\n\nA portable implementation of IBM CMS Pipelines (z/VM).\n\nUsage:\n  pipe [--trace] \"<pipeline specification>\"\n\nOptions:\n  --trace, -t    Emit dispatcher TRACE on stderr (same as (TRACE) in the spec)\n  --version, -v, -V    Print version and exit\n\nExamples:\n  pipe \"literal hello| console\"\n  pipe \"(TRACE) literal hello| console\"\n  pipe --trace \"disk input.txt | locate /foo/ | console\"\n"
        );
        return ExitCode::from(1);
    }

    if args
        .iter()
        .any(|a| a == "--version" || a == "-v" || a == "-V")
    {
        println!("pipe {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let force_trace = args.iter().any(|a| a == "--trace" || a == "-t");
    if force_trace {
        args.retain(|a| a != "--trace" && a != "-t");
    }

    if args.is_empty() {
        eprintln!("PIPE: missing pipeline specification after --trace");
        return ExitCode::from(1);
    }

    // Join remaining args so users can omit quotes on some shells.
    let spec = if args.len() == 1 {
        args.pop().unwrap()
    } else {
        args.join(" ")
    };

    match pipe_stages::run_spec_with_trace_return_code(&spec, force_trace) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(rc) => {
            // CMS PIPE RC (AGGRC last stage, p.258) mapped to a process
            // status. Values outside 0..=255 wrap to the low 8 bits.
            ExitCode::from(rc as u8)
        }
        Err(e) => {
            eprintln!("PIPE: {e}");
            ExitCode::from(8)
        }
    }
}
