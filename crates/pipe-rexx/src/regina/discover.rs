//! Locate a user-installed Regina shared library.

use std::env;
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;

/// Candidate library file names per platform.
pub fn library_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["regina.dll", "regina64.dll", "regina32.dll"]
    }
    #[cfg(target_os = "macos")]
    {
        &["libregina.dylib", "libregina.3.dylib"]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        &["libregina.so", "libregina.so.3", "libregina.so.2"]
    }
}

/// Search order for Regina:
/// 1. `PIPE_REGINA` or `REGINA` / `REGINA_HOME` env vars
/// 2. Directory of the current executable
/// 3. PATH entries
/// 4. Common install prefixes
pub fn find_regina_library() -> Option<PathBuf> {
    for key in ["PIPE_REGINA", "REGINA", "REGINA_HOME"] {
        if let Ok(val) = env::var(key) {
            if let Some(p) = resolve_env_value(&val) {
                return Some(p);
            }
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in library_names() {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    if let Ok(path) = env::var("PATH") {
        for entry in env::split_paths(&path) {
            for name in library_names() {
                let p = entry.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    for dir in common_dirs() {
        for name in library_names() {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
            // lib/ under a prefix
            let p2 = dir.join("lib").join(name);
            if p2.is_file() {
                return Some(p2);
            }
            let p3 = dir.join("bin").join(name);
            if p3.is_file() {
                return Some(p3);
            }
        }
    }

    None
}

fn resolve_env_value(val: &str) -> Option<PathBuf> {
    let p = PathBuf::from(val);
    if p.is_file() {
        return Some(p);
    }
    if p.is_dir() {
        for name in library_names() {
            let c = p.join(name);
            if c.is_file() {
                return Some(c);
            }
            let c = p.join("bin").join(name);
            if c.is_file() {
                return Some(c);
            }
            let c = p.join("lib").join(name);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    // Bare name — try loading by soname later; represent as path-like.
    if !val.contains('/') && !val.contains('\\') && !val.contains('.') {
        // e.g. REGINA=C:\Regina → already handled as dir; skip
    }
    None
}

fn common_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        for base in [
            r"C:\Program Files\Regina",
            r"C:\Program Files (x86)\Regina",
            r"C:\Regina",
        ] {
            dirs.push(PathBuf::from(base));
        }
        if let Ok(pf) = env::var("ProgramFiles") {
            dirs.push(Path::new(&pf).join("Regina"));
        }
        if let Ok(pf) = env::var("ProgramFiles(x86)") {
            dirs.push(Path::new(&pf).join("Regina"));
        }
    }
    #[cfg(unix)]
    {
        for base in [
            "/usr/lib",
            "/usr/local/lib",
            "/usr/lib64",
            "/usr/local/lib64",
            // Debian/Ubuntu multiarch (libregina.so.3 lives here, not /usr/lib).
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
            "/opt/regina/lib",
            "/opt/homebrew/lib",
        ] {
            dirs.push(PathBuf::from(base));
        }
    }
    dirs
}

/// Human-readable hint for docs / TRACE.
pub fn discovery_hint() -> String {
    format!(
        "Set PIPE_REGINA or REGINA_HOME to the Regina install, or put {} on PATH",
        library_names().join(" / ")
    )
}
