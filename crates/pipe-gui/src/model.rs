//! GUI document and session state that can be tested without opening a window.

use std::path::{Path, PathBuf};

use pipe_core::{SessionStatus, StageId};
use pipe_parser::parse_pipeline;
use pipe_stages::{
    inspect_topology, is_language_stage_verb, NodeKind, PipelineTopology, StageRegistry,
};

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
    /// Set when the user chose Discard or a successful Save from the close prompt.
    pub close_confirmed: bool,
    /// Save-as from the close prompt: confirm quit after a successful write.
    pub close_after_save: bool,
    pub picker: PickerState,
    /// Orthogonal (right-angle) vs direct edges. In-session only; no settings file.
    pub orthogonal_edges: bool,
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
            close_confirmed: false,
            close_after_save: false,
            picker: PickerState::default(),
            orthogonal_edges: true,
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
                let flags: Vec<crate::graph::NodeFlags> = topo
                    .nodes
                    .iter()
                    .map(|n| node_flags(n.kind, &n.verb, &registry))
                    .collect();
                self.warnings = collect_warnings(&parsed, &topo, &registry);
                self.graph = Some(layout(&topo, &flags));
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
        if let Some(live) = self.session.as_ref().map(|s| s.live.as_slice()) {
            if let Some(id) = runtime_id_for_node(topo, live, idx) {
                return Some(id);
            }
        }
        topology_stage_id(topo, idx)
    }

    pub fn first_topology_stage_id(&self) -> Option<StageId> {
        let topo = self.topology.as_ref()?;
        for (i, n) in topo.nodes.iter().enumerate() {
            if !matches!(n.kind, NodeKind::LabelReference | NodeKind::Connector) {
                return topology_stage_id(topo, i);
            }
        }
        None
    }

    /// True when Quit/close should show Save/Discard/Cancel instead of exiting.
    pub fn close_needs_prompt(&self) -> bool {
        self.dirty() && !self.close_confirmed
    }

    pub fn request_close(&mut self) -> bool {
        if self.close_needs_prompt() {
            self.pending_close = true;
            false
        } else {
            true
        }
    }

    pub fn cancel_close(&mut self) {
        self.pending_close = false;
        self.close_confirmed = false;
        self.close_after_save = false;
    }

    pub fn confirm_discard(&mut self) {
        self.pending_close = false;
        self.close_after_save = false;
        self.close_confirmed = true;
    }

    pub fn confirm_saved(&mut self) {
        if !self.dirty() {
            self.pending_close = false;
            self.close_after_save = false;
            self.close_confirmed = true;
        }
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
    let id = topology_stage_id(topo, node_index)?;
    live.iter().find(|s| s.id == id).map(|s| s.id).or(Some(id))
}

/// Runtime `StageId` from topology order (no session, no execution).
pub fn topology_stage_id(topo: &PipelineTopology, node_index: usize) -> Option<StageId> {
    let mut runtime = 0usize;
    for (i, n) in topo.nodes.iter().enumerate() {
        if matches!(n.kind, NodeKind::LabelReference | NodeKind::Connector) {
            continue;
        }
        if i == node_index {
            return Some(StageId(runtime));
        }
        runtime += 1;
    }
    None
}

fn node_flags(kind: NodeKind, verb: &str, registry: &StageRegistry) -> crate::graph::NodeFlags {
    let script = is_language_stage_verb(verb);
    let unknown =
        kind == NodeKind::Stage && !verb.is_empty() && !registry.contains(verb) && !script;
    crate::graph::NodeFlags { unknown, script }
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

    #[test]
    fn discard_does_not_reopen_prompt() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal z| hole".into());
        assert!(!m.request_close());
        assert!(m.pending_close);
        m.confirm_discard();
        assert!(!m.pending_close);
        assert!(m.close_confirmed);
        assert!(m.request_close());
    }

    #[test]
    fn cancel_close_keeps_unsaved_text() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal z| hole".into());
        assert!(!m.request_close());
        m.cancel_close();
        assert!(!m.pending_close);
        assert!(!m.close_confirmed);
        assert!(m.dirty());
        assert_eq!(m.document.text, "literal z| hole");
    }

    #[test]
    fn python_node_is_script_not_unknown() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a | examples/marina/tuck.py owner | hole".into());
        let view = m.graph.as_ref().expect("graph");
        let py = view
            .nodes
            .iter()
            .find(|n| n.title.contains("tuck.py"))
            .expect("tuck.py node");
        assert!(py.script, "{py:?}");
        assert!(!py.unknown, "{py:?}");
        let unknown = view.nodes.iter().find(|n| n.title.contains("NOTASTAGE"));
        assert!(unknown.is_none());
    }

    #[test]
    fn locate_topology_id_matches_live_order() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal apple|literal banana|literal apricot|locate /ap/|console".into());
        let topo = m.topology.as_ref().unwrap();
        let loc = topo.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let id = topology_stage_id(topo, loc).unwrap();
        assert_eq!(id.0, 3, "three literals then locate");
    }
}
