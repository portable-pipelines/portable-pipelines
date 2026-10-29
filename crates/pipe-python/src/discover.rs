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
    if out.status.success() {
        if let Ok(v) = parse_version_line(&String::from_utf8_lossy(&out.stdout)) {
            return Ok(v);
        }
    }
    // A `.cmd` / `.bat` stub (used by tests on Windows) ignores `-c` and
    // prints a version on stdout. Do not retry a real interpreter with no
    // arguments: that would start a REPL and hang.
    let ext = python.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat") {
        let out = Command::new(python)
            .output()
            .map_err(|e| format!("could not run {}: {e}", python.display()))?;
        if out.status.success() {
            return parse_version_line(&String::from_utf8_lossy(&out.stdout));
        }
    }
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

/// Serialize tests that mutate `PIPE_PYTHON` / `PIPE_PYTHONPATH` / `PYTHONPATH`.
#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Compile-time `crates/pipe-python/python` (dev fallback when that path exists).
pub fn crate_python_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("python")
}

/// Directory that contains the `pipelines` package (so PYTHONPATH is its parent).
///
/// Order: `PIPE_PYTHONPATH`, then a `pipelines` folder next to the executable,
/// then [`crate_python_dir`] when that tree is still on disk.
pub fn find_pipelines_parent() -> Result<PathBuf, String> {
    let override_path = env::var_os("PIPE_PYTHONPATH").map(PathBuf::from);
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    find_pipelines_parent_from(override_path, exe_dir.as_deref(), &crate_python_dir())
}

pub fn find_pipelines_parent_from(
    pipe_pythonpath: Option<PathBuf>,
    exe_dir: Option<&Path>,
    crate_python: &Path,
) -> Result<PathBuf, String> {
    if let Some(val) = pipe_pythonpath {
        if is_pipelines_dir(&val) {
            return val.parent().map(|x| x.to_path_buf()).ok_or_else(|| {
                format!(
                    "PIPE_PYTHONPATH pipelines dir has no parent: {}",
                    val.display()
                )
            });
        }
        if is_pipelines_dir(&val.join("pipelines")) {
            return Ok(val);
        }
        return Err(format!(
            "PIPE_PYTHONPATH does not contain a pipelines package: {}",
            val.display()
        ));
    }

    if let Some(dir) = exe_dir {
        if is_pipelines_dir(&dir.join("pipelines")) {
            return Ok(dir.to_path_buf());
        }
    }

    if is_pipelines_dir(&crate_python.join("pipelines")) {
        return Ok(crate_python.to_path_buf());
    }

    Err(not_found_message(exe_dir, crate_python))
}

fn not_found_message(exe_dir: Option<&Path>, crate_python: &Path) -> String {
    let exe_look = match exe_dir {
        Some(d) => d.join("pipelines").display().to_string(),
        None => "<executable directory unavailable>".to_string(),
    };
    format!(
        "pipelines helper package not found (looked at PIPE_PYTHONPATH, {}, and {})",
        exe_look,
        crate_python.join("pipelines").display()
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

    fn write_fake_package(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("__init__.py"), "").unwrap();
        std::fs::write(dir.join("stage.py"), "").unwrap();
    }

    #[test]
    fn crate_python_package_is_discoverable() {
        let _guard = env_lock();
        let parent = find_pipelines_parent().expect("helper package in this crate");
        assert!(is_pipelines_dir(&parent.join("pipelines")));
        assert_eq!(parent, crate_python_dir());
    }

    #[test]
    fn dev_fallback_uses_cargo_manifest_dir() {
        let crate_python = crate_python_dir();
        let missing_exe =
            std::env::temp_dir().join(format!("cms-pipelines-no-helper-{}", std::process::id()));
        let found = find_pipelines_parent_from(None, Some(&missing_exe), &crate_python)
            .expect("dev fallback");
        assert_eq!(found, crate_python);
        assert!(is_pipelines_dir(&found.join("pipelines")));
    }

    #[test]
    fn next_to_exe_wins_over_dev_fallback() {
        let root =
            std::env::temp_dir().join(format!("cms-pipelines-exe-helper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_fake_package(&root.join("pipelines"));
        let crate_python = root.join("missing-crate-python");
        let found =
            find_pipelines_parent_from(None, Some(&root), &crate_python).expect("next to exe");
        assert_eq!(found, root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_package_lists_places_looked() {
        let exe = PathBuf::from("/no/such/exe-dir");
        let crate_python = PathBuf::from("/no/such/crate/python");
        let err =
            find_pipelines_parent_from(None, Some(&exe), &crate_python).expect_err("must fail");
        assert!(err.contains("PIPE_PYTHONPATH"), "{err}");
        assert!(err.contains("pipelines"), "{err}");
        assert!(err.contains("/no/such/exe-dir"), "{err}");
        assert!(err.contains("/no/such/crate/python"), "{err}");
    }

    #[test]
    fn pythonpath_value_prepends_existing() {
        let _guard = env_lock();
        let prev = env::var_os("PYTHONPATH");
        env::set_var("PYTHONPATH", env::join_paths(["/old/a", "/old/b"]).unwrap());
        let got = pythonpath_value(Path::new("/helper/parent"));
        let parts: Vec<PathBuf> = env::split_paths(&got).collect();
        assert_eq!(parts[0], PathBuf::from("/helper/parent"));
        assert!(parts.iter().any(|p| p == Path::new("/old/a")), "{parts:?}");
        match prev {
            Some(p) => env::set_var("PYTHONPATH", p),
            None => env::remove_var("PYTHONPATH"),
        }
    }
}
