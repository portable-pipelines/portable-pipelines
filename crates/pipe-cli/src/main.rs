//! Cross-platform CMS Pipelines CLI.
//!
//! Usage:
//!   pipe "literal hello | console"
//!   pipe (ENDCHAR ?) literal a | L: locate /x/ | console ? L: | console

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    pipe_rexx::install_hook();

    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "CMS Pipelines (portable)\n\nUsage:\n  pipe \"<pipeline specification>\"\n\nExamples:\n  pipe \"literal hello | console\"\n  pipe \"file input.txt | locate /foo/ | console\"\n  pipe \"literal a b c | sort | console\"\n"
        );
        return ExitCode::from(1);
    }

    // Join all args so users can omit quotes on some shells.
    let spec = if args.len() == 1 {
        args.pop().unwrap()
    } else {
        args.join(" ")
    };

    match pipe_stages::run_spec(&spec) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("PIPE: {e}");
            ExitCode::from(8)
        }
    }
}
