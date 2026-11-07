//! GUI document and session state that can be tested without opening a window.

use std::path::{Path, PathBuf};

use pipe_core::{SessionStatus, StageId};
use pipe_parser::parse_pipeline;
use pipe_stages::{inspect_topology, NodeKind, PipelineTopology, StageRegistry};

use crate::document::PipeDocument;
use crate::graph::{layout, GraphView};
use crate::warnings::collect_warnings;
use crate::worker::Snapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    Node(usize),
    Edge(usize),
}

pub struct GuiModel {
    pub document: PipeDocument,
    pub cwd: PathBuf,
    pub topology: Option<PipelineTopology>,
    pub graph: Option<GraphView>,
    pub parse_error: Option<String>,
    pub warnings: Vec<String>,
    pub selection: Selection,
    pub session: Option<Snapshot>,
    pub session_spec: Option<String>,
    pub log: Vec<String>,
    pub console: Vec<String>,
    pub pending_close: bool,
    pub picker: PickerState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Open,
    SaveAs,
    Cwd,
}

#[derive(Debug, Clone)]
pub struct PickerState {
    pub open: bool,
    pub kind: PickerKind,
    pub dir: PathBuf,
    pub name: String,
}

impl Default for PickerState {
    fn default() -> Self {
        Self {
            open: false,
            kind: PickerKind::Open,
            dir: PathBuf::from("."),
            name: String::new(),
        }
    }
}

impl GuiModel {
    pub fn new(cwd: PathBuf) -> Self {
        let mut m = Self {
            document: PipeDocument::untitled(),
            cwd,
            topology: None,
            graph: None,
            parse_error: None,
            warnings: Vec::new(),
            selection: Selection::None,
            session: None,
            session_spec: None,
            log: Vec::new(),
            console: Vec::new(),
            pending_close: false,
            picker: PickerState::default(),
        };
        m.reparse();
        m
    }

    pub fn load_path(&mut self, path: &Path) -> Result<(), String> {
        self.document = PipeDocument::open(path).map_err(|e| e.to_string())?;
        self.reparse();
        self.log
            .push(format!("opened {} (not executed)", path.display()));
        Ok(())
    }

    pub fn set_text(&mut self, text: String) {
        self.document.text = text;
        self.reparse();
    }

    pub fn reparse(&mut self) {
        let spec = self.document.text.trim();
        if spec.is_empty() {
            self.topology = None;
            self.graph = None;
            self.parse_error = None;
            self.warnings.clear();
            return;
        }
        match parse_pipeline(&self.document.text) {
            Ok(parsed) => {
                self.parse_error = None;
                let topo = inspect_topology(&parsed);
                let registry = StageRegistry::default();
                let unknown: Vec<bool> = topo
                    .nodes
                    .iter()
                    .map(|n| {
                        n.kind == NodeKind::Stage
                            && !n.verb.is_empty()
                            && !registry.contains(&n.verb)
                    })
                    .collect();
                self.warnings = collect_warnings(&parsed, &topo, &registry);
                self.graph = Some(layout(&topo, &unknown));
                self.topology = Some(topo);
            }
            Err(e) => {
                self.parse_error = Some(e.to_string());
                self.topology = None;
                self.graph = None;
                self.warnings.clear();
            }
        }
    }

    pub fn dirty(&self) -> bool {
        self.document.is_dirty()
    }

    pub fn source_changed_since_session(&self) -> bool {
        match &self.session_spec {
            Some(s) => s != &self.document.text,
            None => false,
        }
    }

    pub fn status(&self) -> SessionStatus {
        self.session
            .as_ref()
            .map(|s| s.status.clone())
            .unwrap_or(SessionStatus::Idle)
    }

    pub fn selected_runtime_stage(&self) -> Option<StageId> {
        let Selection::Node(idx) = self.selection else {
            return None;
        };
        let topo = self.topology.as_ref()?;
        let live = self.session.as_ref()?.live.as_slice();
        runtime_id_for_node(topo, live, idx)
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 400 {
            self.log.drain(0..self.log.len() - 400);
        }
    }
}

pub fn runtime_id_for_node(
    topo: &PipelineTopology,
    live: &[pipe_core::LiveStage],
    node_index: usize,
) -> Option<StageId> {
    let mut runtime = 0usize;
    for (i, n) in topo.nodes.iter().enumerate() {
        if matches!(n.kind, NodeKind::LabelReference | NodeKind::Connector) {
            continue;
        }
        if i == node_index {
            return live.get(runtime).map(|s| s.id);
        }
        runtime += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmp(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pipe-gui-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        p
    }

    #[test]
    fn open_does_not_mark_running() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal hello | > nowhere.txt".into());
        assert!(m.graph.is_some());
        assert!(matches!(m.status(), SessionStatus::Idle));
        assert!(m.session.is_none());
        assert!(m.warnings.iter().any(|w| w.contains("writes")));
    }

    #[test]
    fn dirty_and_restart_semantics() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a| hole".into());
        m.session_spec = Some("literal a| hole".into());
        assert!(!m.source_changed_since_session());
        m.set_text("literal b| hole".into());
        assert!(m.source_changed_since_session());
        assert!(m.dirty());
    }

    #[test]
    fn utf8_path_round_trip() {
        let dir = tmp("dir-café");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("niño.pipe");
        let mut doc = PipeDocument::from_bytes(None, b"literal x| hole\n").unwrap();
        doc.save_to(&path).unwrap();
        let loaded = PipeDocument::open(&path).unwrap();
        assert_eq!(loaded.text, "literal x| hole\n");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn closed_idle_running_dirty_flags() {
        let mut m = GuiModel::new(std::env::temp_dir());
        assert!(matches!(m.status(), SessionStatus::Idle));
        assert!(!m.dirty());
        m.set_text("literal z| hole".into());
        assert!(m.dirty());
        m.document.mark_clean();
        assert!(!m.dirty());
        m.pending_close = true;
        assert!(m.pending_close);
    }
}
