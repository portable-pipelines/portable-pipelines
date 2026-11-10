//! `pipe --gui` launches the sibling native window. It does not run the spec.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Remaining argv after stripping `--gui`, if that flag was present.
pub fn take_gui_mode(args: &[String]) -> Option<Vec<String>> {
    let mut found = false;
    let mut rest = Vec::with_capacity(args.len());
    for a in args {
        if !found && a == "--gui" {
            found = true;
            continue;
        }
        rest.push(a.clone());
    }
    found.then_some(rest)
}

pub fn sibling_gui(current_exe: &Path) -> PathBuf {
    let mut p = current_exe.to_path_buf();
    p.set_file_name(if cfg!(windows) {
        "pipe-gui.exe"
    } else {
        "pipe-gui"
    });
    p
}

pub fn missing_gui_message(exe: &Path, gui: &Path) -> String {
    format!(
        "PIPE: the desktop window `{gui}` was not found next to `{exe}`.\n\
         \n\
         `pipe --gui` looks for `pipe-gui` in the same folder as `pipe`. It does not search PATH.\n\
         \n\
         The Portable Pipelines release includes both programs. If you moved `pipe`, copy `pipe-gui` next to it as well.\n",
        gui = gui.display(),
        exe = exe.display()
    )
}

/// Turn remaining argv into pipe-gui arguments. Paths with spaces stay one argv
/// when the caller already passed a single path. Several tokens that together
/// name an existing file are joined. Otherwise they are `--spec` text.
pub fn gui_child_args(rest: &[String]) -> Vec<String> {
    if rest.is_empty() {
        return Vec::new();
    }
    if rest.len() == 1 {
        let one = &rest[0];
        if looks_like_file(one) {
            return vec![one.clone()];
        }
        return vec!["--spec".into(), one.clone()];
    }
    let joined = rest.join(" ");
    if Path::new(&joined).exists() {
        return vec![joined];
    }
    vec!["--spec".into(), joined]
}

/// Pull `--renderer` out of leftover argv so it is not joined into `--spec`.
pub fn peel_gui_passthrough(rest: &[String]) -> (Vec<String>, Vec<String>) {
    let mut passthrough = Vec::new();
    let mut leftover = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = &rest[i];
        if let Some(v) = a.strip_prefix("--renderer=") {
            passthrough.push("--renderer".into());
            passthrough.push(v.to_string());
            i += 1;
            continue;
        }
        if a == "--renderer" {
            passthrough.push(a.clone());
            i += 1;
            if let Some(v) = rest.get(i) {
                passthrough.push(v.clone());
                i += 1;
            }
            continue;
        }
        leftover.push(a.clone());
        i += 1;
    }
    (passthrough, leftover)
}

fn looks_like_file(s: &str) -> bool {
    let p = Path::new(s);
    if p.exists() {
        return true;
    }
    let lower = s.to_ascii_lowercase();
    lower.ends_with(".pipe") || lower.ends_with(".txt") || s.contains('/') || s.contains('\\')
}

pub fn wants_gui_help(rest: &[String]) -> bool {
    rest.iter().any(|a| a == "--help" || a == "-h")
}

pub fn gui_help_message(gui: &Path) -> String {
    format!(
        "PIPE: desktop GUI is {}\n\
         \n\
         Usage:\n\
           pipe --gui\n\
           pipe --gui FILE\n\
           pipe --gui --renderer auto|wgpu|glow\n\
         \n\
         `pipe --gui` looks for `pipe-gui` in the same folder as `pipe`. It does not search PATH.\n",
        gui.display()
    )
}

pub fn launch_gui(rest: &[String]) -> ExitCode {
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("PIPE: could not find this executable: {e}");
            return ExitCode::from(8);
        }
    };
    let gui = sibling_gui(&exe);
    if !gui.is_file() {
        eprint!("{}", missing_gui_message(&exe, &gui));
        return ExitCode::from(8);
    }
    if wants_gui_help(rest) {
        print!("{}", gui_help_message(&gui));
        return ExitCode::SUCCESS;
    }
    let (passthrough, leftover) = peel_gui_passthrough(rest);
    let mut child_args = gui_child_args(&leftover);
    child_args.extend(passthrough);
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut cmd = Command::new(&gui);
    cmd.args(&child_args);
    cmd.arg("--cwd");
    cmd.arg(&cwd);
    eprintln!("PIPE: starting {}", gui.display());
    match cmd.status() {
        Ok(st) => {
            if st.success() {
                ExitCode::SUCCESS
            } else {
                let code = st.code().unwrap_or(1);
                eprintln!("{}", gui_exit_message(code));
                ExitCode::from(code as u8)
            }
        }
        Err(e) => {
            eprintln!("PIPE: could not start {}: {e}", gui.display());
            ExitCode::from(8)
        }
    }
}

pub fn gui_exit_message(code: i32) -> String {
    format!(
        "PIPE: pipe-gui exited with status {code}. If no window appeared, the GUI process failed before it could draw (often a graphics driver). Default is Direct3D/Vulkan (wgpu) then OpenGL (glow). Override with --renderer wgpu|glow or PIPE_GUI_RENDERER. On Windows, pipe-gui is a Windows-subsystem app so its own eprintln is hidden unless it attached to this console."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_gui_before_join() {
        let args = vec!["--gui".into(), "my file.pipe".into()];
        let rest = take_gui_mode(&args).unwrap();
        assert_eq!(rest, vec!["my file.pipe"]);
        assert!(take_gui_mode(&["literal".into(), "a".into()]).is_none());
    }

    #[test]
    fn spec_tokens_become_spec_flag() {
        let args = gui_child_args(&["literal".into(), "hello|".into(), "hole".into()]);
        assert_eq!(args, vec!["--spec", "literal hello| hole"]);
    }

    #[test]
    fn pipe_suffix_is_a_file_even_if_missing() {
        let args = gui_child_args(&["docs/a b.pipe".into()]);
        assert_eq!(args, vec!["docs/a b.pipe"]);
    }

    #[test]
    fn missing_message_names_same_folder() {
        let msg = missing_gui_message(Path::new("pipe.exe"), Path::new("pipe-gui.exe"));
        assert!(msg.contains("same folder as `pipe`"));
        assert!(msg.contains("does not search PATH"));
        assert!(msg.contains("copy `pipe-gui` next to it"));
        assert!(!msg.contains("cargo build"));
        assert!(!msg.contains("ships `pipe` only"));
    }

    #[test]
    fn child_failure_is_printed() {
        let msg = gui_exit_message(1);
        assert!(msg.contains("exited with status 1"));
        assert!(msg.contains("Windows-subsystem"));
        assert!(msg.contains("--renderer"));
        assert!(msg.contains("PIPE_GUI_RENDERER"));
    }

    #[test]
    fn renderer_is_not_joined_into_spec() {
        let rest = vec![
            "--renderer".into(),
            "wgpu".into(),
            "literal hello| hole".into(),
        ];
        let (flags, leftover) = peel_gui_passthrough(&rest);
        assert_eq!(flags, vec!["--renderer", "wgpu"]);
        assert_eq!(leftover, vec!["literal hello| hole"]);
        let mut child = gui_child_args(&leftover);
        child.extend(flags);
        assert_eq!(
            child,
            vec!["--spec", "literal hello| hole", "--renderer", "wgpu"]
        );
    }

    #[test]
    fn help_flag_is_detected() {
        assert!(wants_gui_help(&["--help".into()]));
        assert!(wants_gui_help(&["-h".into()]));
        assert!(wants_gui_help(&["file.pipe".into(), "--help".into()]));
        assert!(!wants_gui_help(&["file.pipe".into()]));
        let msg = gui_help_message(Path::new("pipe-gui"));
        assert!(msg.contains("desktop GUI is pipe-gui"));
        assert!(msg.contains("pipe --gui FILE"));
        assert!(msg.contains("does not search PATH"));
    }

    #[test]
    fn renderer_equals_and_file() {
        let rest = vec!["docs/a.pipe".into(), "--renderer=glow".into()];
        let (flags, leftover) = peel_gui_passthrough(&rest);
        assert_eq!(flags, vec!["--renderer", "glow"]);
        assert_eq!(leftover, vec!["docs/a.pipe"]);
        let mut child = gui_child_args(&leftover);
        child.extend(flags);
        assert_eq!(child, vec!["docs/a.pipe", "--renderer", "glow"]);
    }
}
