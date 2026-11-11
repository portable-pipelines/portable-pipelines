//! Remember GUI choices between sessions.
//!
//! Stored as a small JSON file in the platform config directory
//! (`portable-pipelines/gui.json`). Restart / Run again / Step again
//! do not touch this file.

use std::collections::HashMap;
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
    pub const COLUMN_WIDTHS_KEY: &'static str = "column_widths";
    pub const GRID_FRAMES_KEY: &'static str = "grid_frames";
    /// Slim Records strip when grids live on the hover card.
    pub const ON_GRAPH_PANE_HINT: &'static str =
        "Records show on the graph. Switch Show records to In Records pane to see them here.";

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

    /// True when the bottom Records pane shows record grids.
    pub fn pane_shows_grids(self) -> bool {
        matches!(self, Self::Pane)
    }

    /// Which Records chrome to draw for this mode and Hide/Show state.
    pub fn chrome(self, inspector_open: bool) -> RecordsChrome {
        match self {
            Self::Pane if inspector_open => RecordsChrome::Grids,
            Self::Pane => RecordsChrome::Collapsed,
            Self::Graph => RecordsChrome::OnGraphHint,
        }
    }
}

/// Bottom Records chrome. Grids never share the pane with On the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordsChrome {
    /// Resizable pane with record grids (Show records = In Records pane).
    Grids,
    /// User hid the pane; a slim Show bar remains.
    Collapsed,
    /// Slim hint: grids live on the hover card.
    OnGraphHint,
}

pub const ON_GRAPH_RECORDS_HEIGHT: f32 = 40.0;

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

fn read_config_object(path: &Path) -> serde_json::Value {
    let mut v = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !v.is_object() {
        v = serde_json::json!({});
    }
    v
}

fn write_config_object(path: &Path, v: &serde_json::Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
    }
    let body = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(path, format!("{body}\n")).map_err(|e| e.to_string())
}

pub fn save_to_path(path: &Path, mode: ShowRecords) -> Result<(), String> {
    let mut v = read_config_object(path);
    v[ShowRecords::STORAGE_KEY] = serde_json::Value::String(mode.as_str().into());
    write_config_object(path, &v)
}

pub fn save_persisted(mode: ShowRecords) -> Result<(), String> {
    save_to_path(&config_path(), mode)
}

pub fn load_column_widths_from_path(path: &Path) -> HashMap<String, Vec<f32>> {
    let v = read_config_object(path);
    parse_column_widths(&v)
}

pub fn load_column_widths() -> HashMap<String, Vec<f32>> {
    load_column_widths_from_path(&config_path())
}

fn parse_column_widths(v: &serde_json::Value) -> HashMap<String, Vec<f32>> {
    let mut out = HashMap::new();
    let Some(map) = v
        .get(ShowRecords::COLUMN_WIDTHS_KEY)
        .and_then(|x| x.as_object())
    else {
        return out;
    };
    for (k, val) in map {
        let Some(arr) = val.as_array() else {
            continue;
        };
        let widths: Vec<f32> = arr
            .iter()
            .filter_map(|n| n.as_f64().map(|x| x as f32))
            .collect();
        if !widths.is_empty() {
            out.insert(k.clone(), widths);
        }
    }
    out
}

pub fn save_column_widths_to_path(
    path: &Path,
    widths: &HashMap<String, Vec<f32>>,
) -> Result<(), String> {
    let mut v = read_config_object(path);
    let mut obj = serde_json::Map::new();
    for (k, ws) in widths {
        obj.insert(
            k.clone(),
            serde_json::Value::Array(
                ws.iter()
                    .map(|w| serde_json::json!((*w as f64 * 10.0).round() / 10.0))
                    .collect(),
            ),
        );
    }
    v[ShowRecords::COLUMN_WIDTHS_KEY] = serde_json::Value::Object(obj);
    write_config_object(path, &v)
}

pub fn save_column_widths(widths: &HashMap<String, Vec<f32>>) -> Result<(), String> {
    save_column_widths_to_path(&config_path(), widths)
}

pub fn load_grid_frames_from_path(path: &Path) -> HashMap<String, (f32, f32)> {
    let v = read_config_object(path);
    parse_grid_frames(&v)
}

pub fn load_grid_frames() -> HashMap<String, (f32, f32)> {
    load_grid_frames_from_path(&config_path())
}

fn parse_grid_frames(v: &serde_json::Value) -> HashMap<String, (f32, f32)> {
    let mut out = HashMap::new();
    let Some(map) = v
        .get(ShowRecords::GRID_FRAMES_KEY)
        .and_then(|x| x.as_object())
    else {
        return out;
    };
    for (k, val) in map {
        let Some(arr) = val.as_array() else {
            continue;
        };
        if arr.len() < 2 {
            continue;
        }
        let Some(w) = arr[0].as_f64() else {
            continue;
        };
        let Some(h) = arr[1].as_f64() else {
            continue;
        };
        out.insert(k.clone(), (w as f32, h as f32));
    }
    out
}

pub fn save_grid_frames_to_path(
    path: &Path,
    frames: &HashMap<String, (f32, f32)>,
) -> Result<(), String> {
    let mut v = read_config_object(path);
    let mut obj = serde_json::Map::new();
    for (k, (w, h)) in frames {
        obj.insert(
            k.clone(),
            serde_json::json!([
                (*w as f64 * 10.0).round() / 10.0,
                (*h as f64 * 10.0).round() / 10.0
            ]),
        );
    }
    v[ShowRecords::GRID_FRAMES_KEY] = serde_json::Value::Object(obj);
    write_config_object(path, &v)
}

pub fn save_grid_frames(frames: &HashMap<String, (f32, f32)>) -> Result<(), String> {
    save_grid_frames_to_path(&config_path(), frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
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
        assert!(ShowRecords::Pane.pane_shows_grids());
        assert!(!ShowRecords::Graph.pane_shows_grids());
        assert_eq!(ShowRecords::Pane.chrome(true), RecordsChrome::Grids);
        assert_eq!(ShowRecords::Pane.chrome(false), RecordsChrome::Collapsed);
        assert_eq!(ShowRecords::Graph.chrome(true), RecordsChrome::OnGraphHint);
        assert_eq!(ShowRecords::Graph.chrome(false), RecordsChrome::OnGraphHint);
        assert_eq!(ShowRecords::Pane.label(), "In Records pane");
        assert_eq!(ShowRecords::Graph.label(), "On the graph");
        assert_eq!(
            ShowRecords::ON_GRAPH_PANE_HINT,
            "Records show on the graph. Switch Show records to In Records pane to see them here."
        );
        let hint = ShowRecords::ON_GRAPH_PANE_HINT;
        assert!(!hint.to_lowercase().contains("debugger"));
        assert!(!hint.to_lowercase().contains("lane"));
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

    #[test]
    fn column_widths_roundtrip_without_dropping_show_records() {
        let path = temp_path("cols");
        save_to_path(&path, ShowRecords::Graph).unwrap();
        let mut map = HashMap::new();
        map.insert("stage:2:recv".into(), vec![52.0, 220.0]);
        save_column_widths_to_path(&path, &map).unwrap();
        assert_eq!(load_from_path(&path), ShowRecords::Graph);
        let loaded = load_column_widths_from_path(&path);
        assert_eq!(
            loaded.get("stage:2:recv").cloned().unwrap(),
            vec![52.0, 220.0]
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"show_records\": \"graph\""), "{text}");
        assert!(!text.to_lowercase().contains("debugger"));
        assert!(!text.to_lowercase().contains("lane"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn grid_frames_roundtrip_without_dropping_show_records() {
        let path = temp_path("frames");
        save_to_path(&path, ShowRecords::Graph).unwrap();
        let mut map = HashMap::new();
        map.insert("stage:2:recv".into(), (320.0, 180.0));
        save_grid_frames_to_path(&path, &map).unwrap();
        assert_eq!(load_from_path(&path), ShowRecords::Graph);
        let loaded = load_grid_frames_from_path(&path);
        assert_eq!(loaded.get("stage:2:recv").copied().unwrap(), (320.0, 180.0));
        let mut cols = HashMap::new();
        cols.insert("stage:2:recv".into(), vec![80.0, 240.0]);
        save_column_widths_to_path(&path, &cols).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"show_records\": \"graph\""), "{text}");
        assert!(text.contains("\"grid_frames\""), "{text}");
        assert!(text.contains("column_widths"), "{text}");
        assert!(!text.to_lowercase().contains("debugger"));
        assert!(!text.to_lowercase().contains("lane"));
        let _ = fs::remove_file(&path);
    }
}
