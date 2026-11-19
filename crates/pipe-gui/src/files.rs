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

/// Where Open and Save as start. Prefer the open file's folder, then the
/// last folder a `.pipe` was opened or saved, then the working directory.
pub fn browse_start_dir(
    document_path: Option<&Path>,
    last_folder: Option<&Path>,
    cwd: &Path,
) -> PathBuf {
    if let Some(path) = document_path {
        if let Some(dir) = path.parent() {
            if dir.as_os_str().is_empty() {
                return default_browse_dir(cwd);
            }
            if dir.is_dir() {
                return dir.to_path_buf();
            }
        }
    }
    if let Some(dir) = last_folder {
        if dir.is_dir() {
            return dir.to_path_buf();
        }
    }
    default_browse_dir(cwd)
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
    fn browse_start_prefers_open_file_then_last_folder_then_cwd() {
        let root = std::env::temp_dir().join(format!(
            "pipe-gui-browse-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let file_dir = root.join("opened");
        let last_dir = root.join("last");
        let cwd = root.join("cwd");
        fs::create_dir_all(&file_dir).unwrap();
        fs::create_dir_all(&last_dir).unwrap();
        fs::create_dir_all(&cwd).unwrap();
        let file = file_dir.join("demo.pipe");
        fs::write(&file, "literal a | console\n").unwrap();

        assert_eq!(
            browse_start_dir(Some(&file), Some(&last_dir), &cwd),
            file_dir
        );
        assert_eq!(browse_start_dir(None, Some(&last_dir), &cwd), last_dir);
        assert_eq!(browse_start_dir(None, None, &cwd), cwd);
        assert_eq!(
            browse_start_dir(Some(Path::new("bare.pipe")), Some(&last_dir), &cwd),
            cwd
        );
        let missing = root.join("gone");
        assert_eq!(browse_start_dir(None, Some(&missing), &cwd), cwd);

        let _ = fs::remove_dir_all(&root);
    }

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
