//! Remember GUI choices between sessions.
//!
//! Stored as a small JSON file in the platform config directory
//! (`portable-pipelines/gui.json`). Restart / Run again / Step again
//! do not touch this file.

use std::path::{Path, PathBuf};

/// Where record grids appear for a hovered stage or connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShowRecords {
    /// Short peek on hover; grids live in the Records pane (default).
    #[default]
    Pane,
    /// Full hover card with record grids on the graph.
    Graph,
}

impl ShowRecords {
    pub const STORAGE_KEY: &'static str = "show_records";

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pane => "pane",
            Self::Graph => "graph",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pane => "In Records pane",
            Self::Graph => "On the graph",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pane" | "records-pane" | "in-records-pane" | "in records pane" => Some(Self::Pane),
            "graph" | "on-the-graph" | "on the graph" => Some(Self::Graph),
            _ => None,
        }
    }

    /// True when hover shows the record grids on the graph.
    pub fn grids_on_graph(self) -> bool {
        matches!(self, Self::Graph)
    }
}

/// Config path. `PIPE_GUI_CONFIG` overrides for tests.
pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("PIPE_GUI_CONFIG") {
        return PathBuf::from(p);
    }
    platform_config_dir().join("gui.json")
}

fn platform_config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("portable-pipelines");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("portable-pipelines");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("portable-pipelines");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("portable-pipelines");
    }
    PathBuf::from("portable-pipelines")
}

pub fn load_from_path(path: &Path) -> ShowRecords {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ShowRecords::Pane;
    };
    parse_config(&text).unwrap_or(ShowRecords::Pane)
}

pub fn load_persisted() -> ShowRecords {
    load_from_path(&config_path())
}

fn parse_config(text: &str) -> Option<ShowRecords> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    ShowRecords::parse(v.get(ShowRecords::STORAGE_KEY)?.as_str()?)
}

pub fn save_to_path(path: &Path, mode: ShowRecords) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
    }
    let mut v = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !v.is_object() {
        v = serde_json::json!({});
    }
    v[ShowRecords::STORAGE_KEY] = serde_json::Value::String(mode.as_str().into());
    let body = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    std::fs::write(path, format!("{body}\n")).map_err(|e| e.to_string())
}

pub fn save_persisted(mode: ShowRecords) -> Result<(), String> {
    save_to_path(&config_path(), mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(tag: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "pipe-gui-show-records-{}-{}-{}.json",
            tag,
            std::process::id(),
            n
        ))
    }

    #[test]
    fn default_is_records_pane() {
        assert_eq!(ShowRecords::default(), ShowRecords::Pane);
        assert!(!ShowRecords::Pane.grids_on_graph());
        assert!(ShowRecords::Graph.grids_on_graph());
        assert_eq!(ShowRecords::Pane.label(), "In Records pane");
        assert_eq!(ShowRecords::Graph.label(), "On the graph");
    }

    #[test]
    fn parse_accepts_short_and_label_forms() {
        assert_eq!(ShowRecords::parse("pane"), Some(ShowRecords::Pane));
        assert_eq!(ShowRecords::parse("graph"), Some(ShowRecords::Graph));
        assert_eq!(
            ShowRecords::parse("In Records pane"),
            Some(ShowRecords::Pane)
        );
        assert_eq!(ShowRecords::parse("On the graph"), Some(ShowRecords::Graph));
        assert!(ShowRecords::parse("lanes").is_none());
        assert!(ShowRecords::parse("debugger").is_none());
    }

    #[test]
    fn config_roundtrip_survives_a_new_session() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);
        assert_eq!(load_from_path(&path), ShowRecords::Pane);
        save_to_path(&path, ShowRecords::Graph).unwrap();
        assert_eq!(load_from_path(&path), ShowRecords::Graph);
        // Restart / Run again do not rewrite this file. Loading again
        // still returns the user's choice.
        assert_eq!(load_from_path(&path), ShowRecords::Graph);
        save_to_path(&path, ShowRecords::Pane).unwrap();
        assert_eq!(load_from_path(&path), ShowRecords::Pane);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"show_records\": \"pane\""), "{text}");
        assert!(!text.to_lowercase().contains("debugger"));
        assert!(!text.to_lowercase().contains("lane"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_preserves_other_keys() {
        let path = temp_path("preserve");
        fs::write(&path, "{\n  \"theme\": \"dark\"\n}\n").unwrap();
        save_to_path(&path, ShowRecords::Graph).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"theme\": \"dark\""), "{text}");
        assert!(text.contains("\"show_records\": \"graph\""), "{text}");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn malformed_config_falls_back_to_pane() {
        let path = temp_path("bad");
        fs::write(&path, "not json").unwrap();
        assert_eq!(load_from_path(&path), ShowRecords::Pane);
        fs::write(&path, "{\"show_records\": 3}").unwrap();
        assert_eq!(load_from_path(&path), ShowRecords::Pane);
        let _ = fs::remove_file(&path);
    }
}
