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
         The signed GitHub Release zip ships `pipe` only. `--gui` is a local-build preview until a later release includes it.\n\
         \n\
         From a clone on Windows (PowerShell):\n\
           cargo build --release -p pipe-cli -p pipe-gui\n\
           .\\target\\release\\pipe.exe --gui\n\
           .\\target\\release\\pipe.exe --gui pipeline.txt\n\
         \n\
         Linux and macOS:\n\
           cargo build --release -p pipe-cli -p pipe-gui\n\
           ./target/release/pipe --gui\n\
           ./target/release/pipe --gui pipeline.txt\n",
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

fn looks_like_file(s: &str) -> bool {
    let p = Path::new(s);
    if p.exists() {
        return true;
    }
    let lower = s.to_ascii_lowercase();
    lower.ends_with(".pipe") || lower.ends_with(".txt") || s.contains('/') || s.contains('\\')
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
    let child_args = gui_child_args(rest);
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
        "PIPE: pipe-gui exited with status {code}. If no window appeared, the GUI process failed before it could draw (often a graphics driver or missing library). On Windows, pipe-gui is a Windows-subsystem app so its own eprintln is hidden unless it attached to this console."
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
    fn missing_message_has_build_line() {
        let msg = missing_gui_message(Path::new("pipe.exe"), Path::new("pipe-gui.exe"));
        assert!(msg.contains("cargo build --release -p pipe-cli -p pipe-gui"));
        assert!(msg.contains("Release zip ships `pipe` only"));
    }

    #[test]
    fn child_failure_is_printed() {
        let msg = gui_exit_message(1);
        assert!(msg.contains("exited with status 1"));
        assert!(msg.contains("Windows-subsystem"));
    }
}
