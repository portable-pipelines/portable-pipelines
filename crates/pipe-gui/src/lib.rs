//! Native desktop editor and debugger for Portable Pipelines.
//!
//! The window talks to a dedicated worker thread that owns `DebugSession`.
//! Opening a file or drawing the graph does not run the pipeline.

pub mod app;
pub mod document;
pub mod files;
pub mod graph;
pub mod inspector;
pub mod model;
pub mod preview;
pub mod theme;
pub mod warnings;
pub mod worker;

pub use app::{GuiApp, LaunchCfg};
pub use document::{DocumentError, PipeDocument};
pub use graph::layout;
pub use model::GuiModel;

use std::path::PathBuf;

/// Command-line arguments for `pipe-gui`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuiArgs {
    pub file: Option<PathBuf>,
    pub spec: Option<String>,
    pub cwd: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub screenshot_frames: u32,
    pub auto_run: bool,
    pub auto_step: bool,
    pub quit_after_ms: Option<u64>,
    pub show_unsaved: bool,
    pub select_verb: Option<String>,
    pub select_edge: Option<usize>,
    pub orthogonal: Option<bool>,
}

pub fn parse_gui_args(args: &[String]) -> Result<GuiArgs, String> {
    let mut out = GuiArgs {
        screenshot_frames: 12,
        ..GuiArgs::default()
    };
    let mut i = 0;
    let mut positional = Vec::new();
    while i < args.len() {
        match args[i].as_str() {
            "--spec" => {
                i += 1;
                let spec = args.get(i).ok_or("--spec requires text")?.clone();
                out.spec = Some(spec);
            }
            "--cwd" => {
                i += 1;
                let cwd = args.get(i).ok_or("--cwd requires a path")?;
                out.cwd = Some(PathBuf::from(cwd));
            }
            "--screenshot" => {
                i += 1;
                let p = args.get(i).ok_or("--screenshot requires a path")?;
                out.screenshot = Some(PathBuf::from(p));
            }
            "--screenshot-frames" => {
                i += 1;
                let n = args.get(i).ok_or("--screenshot-frames requires a number")?;
                out.screenshot_frames = n.parse().map_err(|_| "invalid --screenshot-frames")?;
            }
            "--auto-run" => {
                out.auto_run = true;
            }
            "--auto-step" => {
                out.auto_step = true;
            }
            "--show-unsaved" => {
                out.show_unsaved = true;
            }
            "--select-verb" => {
                i += 1;
                let v = args.get(i).ok_or("--select-verb requires a name")?;
                out.select_verb = Some(v.clone());
            }
            "--select-edge" => {
                i += 1;
                let n = args.get(i).ok_or("--select-edge requires an index")?;
                out.select_edge = Some(n.parse().map_err(|_| "invalid --select-edge")?);
            }
            "--edges" => {
                i += 1;
                match args.get(i).map(|s| s.as_str()) {
                    Some("orthogonal") | Some("right-angle") => out.orthogonal = Some(true),
                    Some("direct") | Some("angular") => out.orthogonal = Some(false),
                    Some(other) => {
                        return Err(format!(
                            "--edges expected orthogonal or direct, got {other}"
                        ));
                    }
                    None => return Err("--edges requires orthogonal or direct".into()),
                }
            }
            "--quit-after-ms" => {
                i += 1;
                let n = args.get(i).ok_or("--quit-after-ms requires a number")?;
                out.quit_after_ms = Some(n.parse().map_err(|_| "invalid --quit-after-ms")?);
            }
            "--help" | "-h" => {
                return Err(usage().into());
            }
            s if s.starts_with('-') => {
                return Err(format!("unknown option {s}\n{}", usage()));
            }
            _ => positional.push(args[i].clone()),
        }
        i += 1;
    }
    if positional.len() > 1 {
        return Err("pass one file, or use --spec for joined text".into());
    }
    if let Some(p) = positional.pop() {
        out.file = Some(PathBuf::from(p));
    }
    Ok(out)
}

pub fn usage() -> String {
    "pipe-gui [FILE] [--spec TEXT] [--cwd DIR] [--screenshot PNG]\n\
     [--auto-run] [--quit-after-ms N]\n\
     Native editor and debugger. Does not run a pipeline until you press Run.\n\
     CONSOLE stdin is refused so the window cannot block on a hidden terminal.\n\
     --auto-run, --auto-step, and --quit-after-ms are for tests: they still abort on Quit.\n\
     --show-unsaved, --select-verb NAME, --select-edge N, and --edges orthogonal|direct are screenshot/test aids."
        .into()
}

/// Print a launch/shutdown error where a human can see it.
///
/// `pipe-gui` is a Windows-subsystem app, so `eprintln` is silent unless this
/// process attached to the parent `pipe --gui` console. `CONOUT$` is a second
/// try on Windows after `AttachConsole`.
pub fn report_error(msg: &str) {
    eprintln!("{msg}");
    let _ = std::io::Write::flush(&mut std::io::stderr());
    #[cfg(windows)]
    {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
            let _ = writeln!(f, "{msg}");
            let _ = f.flush();
        }
    }
}

#[cfg(test)]
mod arg_tests {
    use super::*;

    #[test]
    fn file_with_spaces_is_one_path() {
        let args = vec!["docs/my file.pipe".into()];
        let g = parse_gui_args(&args).unwrap();
        assert_eq!(g.file.unwrap().to_string_lossy(), "docs/my file.pipe");
        assert!(g.spec.is_none());
    }

    #[test]
    fn spec_flag() {
        let args = vec!["--spec".into(), "literal hello| hole".into()];
        let g = parse_gui_args(&args).unwrap();
        assert_eq!(g.spec.as_deref(), Some("literal hello| hole"));
    }

    #[test]
    fn auto_run_and_quit_after_ms() {
        let args = vec![
            "--spec".into(),
            "literal hello| hole".into(),
            "--auto-run".into(),
            "--quit-after-ms".into(),
            "250".into(),
        ];
        let g = parse_gui_args(&args).unwrap();
        assert!(g.auto_run);
        assert_eq!(g.quit_after_ms, Some(250));
    }

    #[test]
    fn screenshot_aids() {
        let g = parse_gui_args(&[
            "--show-unsaved".into(),
            "--select-verb".into(),
            "LOCATE".into(),
            "--select-edge".into(),
            "0".into(),
            "--auto-step".into(),
            "--edges".into(),
            "direct".into(),
        ])
        .unwrap();
        assert!(g.show_unsaved);
        assert_eq!(g.select_verb.as_deref(), Some("LOCATE"));
        assert_eq!(g.select_edge, Some(0));
        assert!(g.auto_step);
        assert_eq!(g.orthogonal, Some(false));
    }
}
