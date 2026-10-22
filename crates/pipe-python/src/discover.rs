//! Interpreter and helper-package discovery.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MIN_MAJOR: u32 = 3;
pub const MIN_MINOR: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PythonVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PythonVersion {
    pub fn meets_min(self) -> bool {
        self.major > MIN_MAJOR || (self.major == MIN_MAJOR && self.minor >= MIN_MINOR)
    }

    pub fn display(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

pub fn format_old_python_error(found: PythonVersion) -> String {
    format!(
        "Python 3.10 or later is required; found {}",
        found.display()
    )
}

pub fn pipe_require_python() -> bool {
    matches!(env::var("PIPE_REQUIRE_PYTHON").as_deref(), Ok("1"))
}

/// True when a usable interpreter (3.10+) can be found.
pub fn python_available() -> bool {
    match find_python() {
        Ok(p) => match query_python_version(&p) {
            Ok(v) => v.meets_min(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

pub fn discovery_hint() -> String {
    "set PIPE_PYTHON to python.exe / python3, or install Python 3.10+ on PATH (Windows: the py -3 launcher is tried first after PIPE_PYTHON)".to_string()
}

/// Locate the interpreter. Order: `PIPE_PYTHON`, then `py -3` on Windows,
/// then `python3`, then `python`.
pub fn find_python() -> Result<PathBuf, String> {
    if let Ok(val) = env::var("PIPE_PYTHON") {
        let p = PathBuf::from(&val);
        if p.is_dir() {
            for name in ["python.exe", "python3.exe", "python3", "python"] {
                let cand = p.join(name);
                if cand.is_file() {
                    return Ok(cand);
                }
            }
            return Err(format!(
                "PIPE_PYTHON is a directory but contains no python executable: {val}"
            ));
        }
        if !p.as_os_str().is_empty() {
            return Ok(p);
        }
    }

    #[cfg(windows)]
    {
        if let Some(exe) = py_launcher_executable() {
            return Ok(exe);
        }
    }

    if let Some(p) = which("python3") {
        if !is_macos_clt_stub(&p) {
            return Ok(p);
        }
    }
    if let Some(p) = which("python") {
        return Ok(p);
    }
    Err(format!(
        "Python interpreter not found ({})",
        discovery_hint()
    ))
}

#[cfg(windows)]
fn py_launcher_executable() -> Option<PathBuf> {
    let out = Command::new("py")
        .args(["-3", "-c", "import sys; print(sys.executable)"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return None;
    }
    Some(PathBuf::from(s))
}

fn which(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

fn is_macos_clt_stub(path: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    Command::new(path)
        .args(["-c", "import sys"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
}

pub fn query_python_version(python: &Path) -> Result<PythonVersion, String> {
    let out = Command::new(python)
        .args(["-c", "import sys; print('%d.%d.%d' % sys.version_info[:3])"])
        .output()
        .map_err(|e| format!("could not run {}: {e}", python.display()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "could not query Python version from {}: {err}",
            python.display()
        ));
    }
    parse_version_line(&String::from_utf8_lossy(&out.stdout))
}

pub fn parse_version_line(s: &str) -> Result<PythonVersion, String> {
    let line = s
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| "Python version output was empty".to_string())?;
    let mut parts = line.split('.');
    let major = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("unrecognized Python version: {line}"))?;
    let minor = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("unrecognized Python version: {line}"))?;
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Ok(PythonVersion {
        major,
        minor,
        patch,
    })
}

/// Directory that contains the `pipelines` package (so PYTHONPATH is its parent).
pub fn find_pipelines_parent() -> Result<PathBuf, String> {
    if let Ok(val) = env::var("PIPE_PYTHONPATH") {
        let p = PathBuf::from(&val);
        if is_pipelines_dir(&p) {
            return p
                .parent()
                .map(|x| x.to_path_buf())
                .ok_or_else(|| "PIPE_PYTHONPATH pipelines dir has no parent".into());
        }
        if is_pipelines_dir(&p.join("pipelines")) {
            return Ok(p);
        }
        return Err(format!(
            "PIPE_PYTHONPATH does not contain a pipelines package: {val}"
        ));
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            if is_pipelines_dir(&dir.join("pipelines")) {
                return Ok(dir.to_path_buf());
            }
        }
    }

    let crate_python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("python");
    if is_pipelines_dir(&crate_python.join("pipelines")) {
        return Ok(crate_python);
    }

    if let Ok(cwd) = env::current_dir() {
        let mut cur = Some(cwd.as_path());
        while let Some(dir) = cur {
            if is_pipelines_dir(&dir.join("pipelines")) {
                return Ok(dir.to_path_buf());
            }
            let nested = dir.join("crates/pipe-python/python");
            if is_pipelines_dir(&nested.join("pipelines")) {
                return Ok(nested);
            }
            cur = dir.parent();
        }
    }

    Err(
        "pipelines helper package not found (expected a pipelines folder next to pipe.exe, or set PIPE_PYTHONPATH)"
            .into(),
    )
}

fn is_pipelines_dir(p: &Path) -> bool {
    p.join("__init__.py").is_file() && p.join("stage.py").is_file()
}

pub fn pythonpath_value(pipelines_parent: &Path) -> OsString {
    let mut paths = vec![pipelines_parent.to_path_buf()];
    if let Some(existing) = env::var_os("PYTHONPATH") {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).unwrap_or_else(|_| pipelines_parent.as_os_str().to_os_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_python_message_names_the_version() {
        let v = PythonVersion {
            major: 3,
            minor: 9,
            patch: 18,
        };
        assert!(!v.meets_min());
        let msg = format_old_python_error(v);
        assert!(msg.contains("3.10"), "{msg}");
        assert!(msg.contains("3.9.18"), "{msg}");
    }

    #[test]
    fn python_310_meets_min() {
        assert!(PythonVersion {
            major: 3,
            minor: 10,
            patch: 0
        }
        .meets_min());
        assert!(PythonVersion {
            major: 3,
            minor: 12,
            patch: 3
        }
        .meets_min());
        assert!(!PythonVersion {
            major: 3,
            minor: 8,
            patch: 10
        }
        .meets_min());
    }

    #[test]
    fn parse_version_line_reads_three_parts() {
        let v = parse_version_line("3.12.3\n").unwrap();
        assert_eq!(
            v,
            PythonVersion {
                major: 3,
                minor: 12,
                patch: 3
            }
        );
    }

    #[test]
    fn crate_python_package_is_discoverable() {
        let parent = find_pipelines_parent().expect("helper package in this crate");
        assert!(is_pipelines_dir(&parent.join("pipelines")));
    }
}
