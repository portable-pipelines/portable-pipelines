//! In-app directory listing. Windows/macOS also offer a native dialog.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DirEntryView {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub fn list_dir(path: &Path) -> Result<Vec<DirEntryView>, String> {
    let mut rows = Vec::new();
    let rd = fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
    for ent in rd {
        let ent = ent.map_err(|e| e.to_string())?;
        let path = ent.path();
        let is_dir = path.is_dir();
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        rows.push(DirEntryView { name, path, is_dir });
    }
    rows.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase()),
    });
    Ok(rows)
}

pub fn default_browse_dir(cwd: &Path) -> PathBuf {
    if cwd.is_dir() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// Native dialog when the crate feature is present for this OS.
pub fn native_open(start: &Path) -> Option<PathBuf> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        rfd::FileDialog::new()
            .add_filter("Pipeline", &["pipe", "txt"])
            .set_directory(start)
            .pick_file()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = start;
        None
    }
}

/// Add `.pipe` when the path has no extension. Keep `.txt` (and others) as typed.
pub fn ensure_pipe_extension(path: PathBuf) -> PathBuf {
    match path.extension() {
        Some(_) => path,
        None => {
            if path.as_os_str().is_empty() {
                PathBuf::from("pipeline.pipe")
            } else {
                path.with_extension("pipe")
            }
        }
    }
}

pub fn native_save(start: &Path, filename: &str) -> Option<PathBuf> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        rfd::FileDialog::new()
            .add_filter("Pipeline", &["pipe", "txt"])
            .set_directory(start)
            .set_file_name(filename)
            .save_file()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (start, filename);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_without_extension_becomes_pipe() {
        assert_eq!(
            ensure_pipe_extension(PathBuf::from("notes")),
            PathBuf::from("notes.pipe")
        );
        assert_eq!(
            ensure_pipe_extension(PathBuf::from("pipeline.pipe")),
            PathBuf::from("pipeline.pipe")
        );
        assert_eq!(
            ensure_pipe_extension(PathBuf::from("readme.txt")),
            PathBuf::from("readme.txt")
        );
    }
}
