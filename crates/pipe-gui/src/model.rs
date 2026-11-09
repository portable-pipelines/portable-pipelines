//! GUI document and session state that can be tested without opening a window.

use std::path::{Path, PathBuf};

use pipe_core::{SessionStatus, StageId};
use pipe_parser::parse_pipeline;
use pipe_stages::{
    inspect_topology, is_language_stage_verb, NodeKind, PipelineTopology, StageRegistry,
};

use crate::document::PipeDocument;
use crate::graph::{apply_nudges, layout, recompute_ports, GraphView, NodeNudges};
use crate::preview::{
    edge_title, run_overview, FocusLabels, RecordFocus, RunOverview, StageBadges,
};
use crate::warnings::{collect_about, collect_warnings, PipelineAbout};
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
    /// View-only node offsets. Fit/Reset clears these; they never change the spec.
    pub node_nudge: NodeNudges,
    /// Bottom records pane. Collapsed by default height still leaves the graph usable.
    pub inspector_open: bool,
    /// Optional jargon-free extra counts/slots behind a disclosure.
    pub inspector_details: bool,
    /// Explicit script-file view (read only after the user asks).
    pub script_source: Option<ScriptSourceView>,
    /// Bottom legend shows words next to drawn marks. Icons stay visible when collapsed.
    pub legend_expanded: bool,
    /// How many times Step was clicked in this session.
    pub step_count: usize,
    pub about: PipelineAbout,
}

/// Bytes from a deliberate "View script file" action. Not topology, not hover.
#[derive(Debug, Clone)]
pub struct ScriptSourceView {
    pub path: PathBuf,
    pub language: String,
    pub body: Result<String, String>,
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
            node_nudge: NodeNudges::new(),
            inspector_open: true,
            inspector_details: false,
            script_source: None,
            legend_expanded: true,
            step_count: 0,
            about: PipelineAbout::default(),
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
            self.about = PipelineAbout::default();
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
                self.about = collect_about(&parsed);
                let mut view = layout(&topo, &flags);
                apply_nudges(&mut view, &self.node_nudge);
                self.graph = Some(view);
                self.topology = Some(topo);
            }
            Err(e) => {
                self.parse_error = Some(e.to_string());
                self.topology = None;
                self.graph = None;
                self.warnings.clear();
                self.about = PipelineAbout::default();
            }
        }
    }

    pub fn session_overview(&self) -> RunOverview {
        let names = self.stage_names();
        run_overview(self.session.as_ref(), &names, self.step_count)
    }

    fn stage_names(&self) -> Vec<(StageId, String, String)> {
        let Some(view) = &self.graph else {
            return Vec::new();
        };
        let Some(topo) = &self.topology else {
            return Vec::new();
        };
        view.nodes
            .iter()
            .filter_map(|n| {
                topology_stage_id(topo, n.index).map(|id| (id, n.title.clone(), n.verb.clone()))
            })
            .collect()
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

    pub fn inspect_focus(&self) -> Option<RecordFocus> {
        self.focus_for(self.selection)
    }

    pub fn edge_waiting(&self, edge_index: usize) -> bool {
        matches!(
            self.edge_mark(edge_index),
            crate::symbols::RecordMark::Waiting
        )
    }

    /// Connection mark: hourglass if a record is waiting, check if the
    /// writer has finished, otherwise idle (zzz).
    pub fn edge_mark(&self, edge_index: usize) -> crate::symbols::RecordMark {
        use crate::symbols::RecordMark;
        let Some(focus) = self.focus_for(Selection::Edge(edge_index)) else {
            return RecordMark::Idle;
        };
        let Some(snap) = &self.session else {
            return RecordMark::Idle;
        };
        let RecordFocus::Edge {
            writer,
            writer_stream,
            reader,
            reader_stream,
        } = focus
        else {
            return RecordMark::Idle;
        };
        let slot = snap.slots.iter().find(|s| {
            s.writer == writer
                && s.reader == reader
                && s.writer_stream.0 == writer_stream
                && s.reader_stream.0 == reader_stream
        });
        if slot.is_some_and(|s| s.record.is_some()) {
            return RecordMark::Waiting;
        }
        let reader_ended = snap.live.iter().any(|l| l.id == reader && l.ended);
        let writer_ended = snap.live.iter().any(|l| l.id == writer && l.ended);
        if slot.is_some_and(|s| s.severed || s.writer_done) || reader_ended || writer_ended {
            RecordMark::Done
        } else {
            RecordMark::Idle
        }
    }

    pub fn node_badges(&self, node_index: usize) -> Option<StageBadges> {
        let snap = self.session.as_ref()?;
        let topo = self.topology.as_ref()?;
        let view = self.graph.as_ref()?;
        let id = topology_stage_id(topo, node_index)?;
        let verb = view
            .nodes
            .iter()
            .find(|n| n.index == node_index)
            .map(|n| n.verb.as_str())
            .unwrap_or("");
        let ended = snap.live.iter().any(|l| l.id == id && l.ended);
        let names: Vec<(StageId, String)> = view
            .nodes
            .iter()
            .filter_map(|n| topology_stage_id(topo, n.index).map(|sid| (sid, n.title.clone())))
            .collect();
        Some(crate::preview::stage_badges_named(
            &snap.events,
            &snap.slots,
            id,
            verb,
            ended,
            &names,
            &snap.io_counts,
        ))
    }

    pub fn inspect_labels(&self) -> FocusLabels {
        self.labels_for(self.selection).unwrap_or_default()
    }

    pub fn labels_for(&self, sel: Selection) -> Option<FocusLabels> {
        let view = self.graph.as_ref()?;
        let topo = self.topology.as_ref()?;
        let names: Vec<(StageId, String)> = view
            .nodes
            .iter()
            .filter_map(|n| topology_stage_id(topo, n.index).map(|id| (id, n.title.clone())))
            .collect();
        match sel {
            Selection::None => None,
            Selection::Node(i) => {
                let n = view.nodes.iter().find(|n| n.index == i)?;
                let output_count = distinct_streams(
                    view.edges
                        .iter()
                        .filter(|e| e.writer == i)
                        .map(|e| e.writer_stream),
                );
                let input_count = distinct_streams(
                    view.edges
                        .iter()
                        .filter(|e| e.reader == i)
                        .map(|e| e.reader_stream),
                );
                Some(FocusLabels {
                    title: n.hover_name.clone(),
                    stage: n.title.clone(),
                    writer: n.title.clone(),
                    reader: String::new(),
                    verb: n.verb.clone(),
                    output_count,
                    input_count,
                    names,
                })
            }
            Selection::Edge(i) => {
                let e = view.edges.get(i)?;
                let writer = view
                    .nodes
                    .iter()
                    .find(|n| n.index == e.writer)
                    .map(|n| n.title.as_str())
                    .unwrap_or("?");
                let reader = view
                    .nodes
                    .iter()
                    .find(|n| n.index == e.reader)
                    .map(|n| n.title.as_str())
                    .unwrap_or("?");
                let output_count = distinct_streams(
                    view.edges
                        .iter()
                        .filter(|x| x.writer == e.writer)
                        .map(|x| x.writer_stream),
                );
                Some(FocusLabels {
                    title: edge_title(writer, reader),
                    stage: writer.to_string(),
                    writer: writer.to_string(),
                    reader: reader.to_string(),
                    verb: String::new(),
                    output_count,
                    input_count: 0,
                    names,
                })
            }
        }
    }

    pub fn focus_for(&self, sel: Selection) -> Option<RecordFocus> {
        match sel {
            Selection::None => None,
            Selection::Node(i) => {
                let topo = self.topology.as_ref()?;
                let id = if let Some(live) = self.session.as_ref() {
                    runtime_id_for_node(topo, live.live.as_slice(), i)
                        .or_else(|| topology_stage_id(topo, i))
                } else {
                    topology_stage_id(topo, i)
                }?;
                Some(RecordFocus::Stage(id))
            }
            Selection::Edge(i) => {
                let e = self.graph.as_ref()?.edges.get(i)?;
                let topo = self.topology.as_ref()?;
                Some(RecordFocus::Edge {
                    writer: topology_stage_id(topo, e.writer)?,
                    writer_stream: e.writer_stream,
                    reader: topology_stage_id(topo, e.reader)?,
                    reader_stream: e.reader_stream,
                })
            }
        }
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

    /// Stage to try first on **Step one record**. `None` if nothing is selected
    /// or the selection has already ended — the worker then picks a waiting reader.
    pub fn preferred_step_stage(&self) -> Option<StageId> {
        let id = self.selected_runtime_stage()?;
        match self
            .session
            .as_ref()
            .and_then(|s| s.live.iter().find(|l| l.id == id))
        {
            Some(l) if l.ended => None,
            Some(_) => Some(id),
            None => Some(id),
        }
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

    /// Clear view-only node offsets. Source bytes and wiring are unchanged.
    pub fn reset_node_positions(&mut self) {
        self.node_nudge.clear();
        self.reparse();
    }

    pub fn nudge_node(&mut self, topo_index: usize, dx: f32, dy: f32) {
        let key = self.graph.as_ref().and_then(|view| {
            view.nodes
                .iter()
                .find(|n| n.index == topo_index)
                .map(|n| (n.pipeline, n.stage))
        });
        let Some(key) = key else {
            return;
        };
        let entry = self.node_nudge.entry(key).or_insert((0.0, 0.0));
        entry.0 += dx;
        entry.1 += dy;
        if let Some(view) = self.graph.as_mut() {
            if let Some(n) = view.nodes.iter_mut().find(|n| n.index == topo_index) {
                n.rect.x += dx;
                n.rect.y += dy;
            }
            recompute_ports(view);
        }
    }

    /// Read a script file only when the user asked. Hover/topology never call this.
    pub fn open_script_source(&mut self, verb: &str) {
        let path = resolve_script_path(&self.cwd, verb);
        let language = crate::graph::script_language(verb)
            .unwrap_or("script")
            .to_string();
        let body = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "could not read {} (working directory {}): {e}",
                path.display(),
                self.cwd.display()
            )
        });
        self.script_source = Some(ScriptSourceView {
            path,
            language,
            body,
        });
    }
}

pub fn resolve_script_path(cwd: &Path, verb: &str) -> PathBuf {
    let p = Path::new(verb);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
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
fn distinct_streams<I: Iterator<Item = u32>>(it: I) -> u32 {
    let mut v: Vec<u32> = it.collect();
    v.sort_unstable();
    v.dedup();
    v.len() as u32
}

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
        assert_eq!(py.title, "tuck.py");
        assert!(py.detail.contains("Python"), "{}", py.detail);
        let unknown = view.nodes.iter().find(|n| n.title.contains("NOTASTAGE"));
        assert!(unknown.is_none());
    }

    #[test]
    fn selecting_a_stage_does_not_drop_source_bytes() {
        let spec = std::fs::read_to_string("examples/marina/to-nested.pipe").unwrap_or_else(|_| {
            "literal a | csv | csv | csv | ../examples/marina/tuck.py owner | hole".into()
        });
        let mut m = GuiModel::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        m.set_text(spec.clone());
        let before = m.document.text.clone();
        let csv = m
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.title.eq_ignore_ascii_case("csv") || n.verb.eq_ignore_ascii_case("csv"))
            .map(|n| n.index)
            .expect("csv node");
        m.selection = Selection::Node(csv);
        assert_eq!(
            m.document.text, before,
            "source bytes must survive graph selection"
        );
        assert!(m.document.text.contains("csv"));
    }

    #[test]
    fn node_nudge_is_view_only() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a | hole".into());
        let before = m.document.text.clone();
        let idx = m.graph.as_ref().unwrap().nodes[0].index;
        let x0 = m.graph.as_ref().unwrap().nodes[0].rect.x;
        m.nudge_node(idx, 30.0, 8.0);
        assert_eq!(m.document.text, before);
        assert!((m.graph.as_ref().unwrap().nodes[0].rect.x - (x0 + 30.0)).abs() < 0.01);
        m.reset_node_positions();
        assert_eq!(m.document.text, before);
        assert!((m.graph.as_ref().unwrap().nodes[0].rect.x - x0).abs() < 0.01);
        assert!(m.node_nudge.is_empty());
    }

    #[test]
    fn script_path_resolves_against_cwd_without_reading() {
        let cwd = PathBuf::from("/tmp/work");
        let p = resolve_script_path(&cwd, "../examples/marina/tuck.py");
        assert!(p.ends_with("examples/marina/tuck.py"));
        let mut m = GuiModel::new(cwd);
        m.set_text("literal a | ../examples/marina/tuck.py owner | hole".into());
        assert!(m.script_source.is_none(), "topology must not open the file");
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

    #[test]
    fn preferred_step_skips_ended_selection() {
        use crate::worker::Snapshot;
        use pipe_core::LiveStage;
        let live = |id: usize, name: &str, ended: bool| LiveStage {
            id: StageId(id),
            name: name.into(),
            ended,
            return_code: 0,
            waiting_debug: false,
            waiting_callpipe: false,
            waiting_helper: false,
            commit_group: 0,
        };
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a|literal b|hole".into());
        let first = m.graph.as_ref().unwrap().nodes[0].index;
        m.selection = Selection::Node(first);
        m.session = Some(Snapshot {
            status: SessionStatus::Paused,
            events: Vec::new(),
            slots: Vec::new(),
            live: vec![
                live(0, "LITERAL", true),
                live(1, "LITERAL", false),
                live(2, "HOLE", false),
            ],
            last_step: None,
            spec_fingerprint: String::new(),
            io_counts: Vec::new(),
            record_logs: Vec::new(),
        });
        assert!(
            m.preferred_step_stage().is_none(),
            "ended selection must not be preferred"
        );
        m.selection = Selection::None;
        assert!(m.preferred_step_stage().is_none());
    }
}
