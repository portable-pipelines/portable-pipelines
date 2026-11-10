//! Native window. Windows subsystem so this is not a second console.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;

use pipe_gui::renderer::{resolve, run_gui};
use pipe_gui::{parse_gui_args, report_error, LaunchCfg};

fn main() {
    attach_parent_console();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_gui_args(&args) {
        Ok(a) => a,
        Err(e) => {
            report_error(&e);
            std::process::exit(2);
        }
    };
    let choice = match resolve(parsed.renderer) {
        Ok(c) => c,
        Err(e) => {
            report_error(&e);
            std::process::exit(2);
        }
    };
    let cwd = parsed
        .cwd
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let cfg = LaunchCfg {
        file: parsed.file,
        spec: parsed.spec,
        cwd,
        screenshot: parsed.screenshot,
        screenshot_frames: parsed.screenshot_frames,
        auto_run: parsed.auto_run,
        auto_step: parsed.auto_step,
        quit_after_ms: parsed.quit_after_ms,
        show_unsaved: parsed.show_unsaved,
        show_script: parsed.show_script,
        select_verb: parsed.select_verb,
        select_edge: parsed.select_edge,
        orthogonal: parsed.orthogonal,
    };

    if let Err(e) = run_gui(cfg, choice) {
        report_error(&e);
        std::process::exit(1);
    }
}

/// Attach to the parent `pipe --gui` console so launch errors are visible.
/// Double-click (no parent console) stays a Windows-subsystem window.
fn attach_parent_console() {
    #[cfg(windows)]
    {
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(pid: u32) -> i32;
        }
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
