//! Native desktop editor and debugger for Portable Pipelines.
//!
//! The window talks to a dedicated worker thread that owns `DebugSession`.
//! Opening a file or drawing the graph does not run the pipeline.

pub mod app;
pub mod document;
pub mod files;
pub mod graph;
pub mod model;
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
     Native editor and debugger. Does not run a pipeline until you press Run.\n\
     CONSOLE stdin is refused so the window cannot block on a hidden terminal."
        .into()
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
}
