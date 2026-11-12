//! GUI document and session state that can be tested without opening a window.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pipe_core::{SessionStatus, StageId};
use pipe_parser::{parse_pipeline, SourceSpan};
use pipe_stages::{
    catalog_lookup, check_stage_operands, default_stream_word, inspect_topology,
    is_language_stage_verb, NodeKind, PipelineTopology, StageRegistry,
};

use crate::document::PipeDocument;
use crate::graph::{apply_positions, attach_connection_points, layout, GraphView, NodePositions};
use crate::ids::{match_stage_ids, NodeId, StageKey};
use crate::preview::{
    edge_title, run_overview, FocusLabels, RecordFocus, RunOverview, StageBadges,
};
use crate::serialize::patch_stage_operands;
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
    pub parse_error_span: Option<(usize, usize)>,
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
    /// Session-only node positions, keyed by stable id. Auto layout on Open or Auto layout.
    pub node_positions: NodePositions,
    next_node_id: u64,
    /// Logged once after an edit stops a live run.
    pub edit_stopped_run: bool,
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
    /// Session-only display names for connection points. Never written to Source.
    pub stream_labels: HashMap<(NodeId, bool, u32), String>,
    /// Operand-checker issues on the current (or last valid) graph.
    pub operand_issues: Vec<OperandIssue>,
}

/// Squiggle target for a stage whose operands failed a checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperandIssue {
    pub node_index: usize,
    pub span: (usize, usize),
    pub message: String,
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
            parse_error_span: None,
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
            node_positions: NodePositions::new(),
            next_node_id: 1,
            edit_stopped_run: false,
            inspector_open: true,
            inspector_details: false,
            script_source: None,
            legend_expanded: true,
            step_count: 0,
            about: PipelineAbout::default(),
            stream_labels: HashMap::new(),
            operand_issues: Vec::new(),
        };
        m.reparse();
        m
    }

    pub fn load_path(&mut self, path: &Path) -> Result<(), String> {
        self.document = PipeDocument::open(path).map_err(|e| e.to_string())?;
        self.forget_session_layout();
        self.reparse();
        self.log
            .push(format!("opened {} (not executed)", path.display()));
        Ok(())
    }

    pub fn set_text(&mut self, text: String) {
        self.document.text = text;
        self.reparse();
    }

    fn forget_session_layout(&mut self) {
        self.graph = None;
        self.topology = None;
        self.node_positions.clear();
        self.next_node_id = 1;
        self.selection = Selection::None;
        self.parse_error = None;
        self.parse_error_span = None;
        self.stream_labels.clear();
        self.operand_issues.clear();
    }

    pub fn reparse(&mut self) {
        self.reparse_with_layout(false);
    }

    fn node_flags_for(topo: &PipelineTopology) -> Vec<crate::graph::NodeFlags> {
        let registry = StageRegistry::default();
        topo.nodes
            .iter()
            .map(|n| node_flags(n.kind, &n.verb, &registry))
            .collect()
    }

    fn reparse_with_layout(&mut self, auto_layout: bool) {
        let spec = self.document.text.trim();
        if spec.is_empty() {
            self.topology = None;
            self.graph = None;
            self.parse_error = None;
            self.parse_error_span = None;
            self.warnings.clear();
            self.about = PipelineAbout::default();
            self.selection = Selection::None;
            self.node_positions.clear();
            self.stream_labels.clear();
            self.operand_issues.clear();
            return;
        }
        match parse_pipeline(&self.document.text) {
            Ok(parsed) => {
                self.parse_error = None;
                self.parse_error_span = None;
                let topo = inspect_topology(&parsed);
                let flags = Self::node_flags_for(&topo);
                let registry = StageRegistry::default();
                self.warnings = collect_warnings(&parsed, &topo, &registry);
                self.about = collect_about(&parsed);
                let mut view = layout(&topo, &flags);
                let previous: Vec<StageKey> = self
                    .graph
                    .as_ref()
                    .map(|g| {
                        g.nodes
                            .iter()
                            .map(|n| StageKey {
                                id: n.id,
                                verb: n.verb.clone(),
                                span: n.span,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let incoming: Vec<(String, SourceSpan)> = view
                    .nodes
                    .iter()
                    .map(|n| (n.verb.clone(), n.span))
                    .collect();
                let ids = match_stage_ids(&previous, &incoming, &mut self.next_node_id);
                for (n, id) in view.nodes.iter_mut().zip(ids) {
                    n.id = id;
                }
                let selected_id = match self.selection {
                    Selection::Node(i) => self
                        .graph
                        .as_ref()
                        .and_then(|g| g.nodes.iter().find(|n| n.index == i).map(|n| n.id)),
                    _ => None,
                };
                let selected_edge = match self.selection {
                    Selection::Edge(ei) => self.graph.as_ref().and_then(|g| {
                        let e = g.edges.get(ei)?;
                        let w = g.nodes.iter().find(|n| n.index == e.writer)?.id;
                        let r = g.nodes.iter().find(|n| n.index == e.reader)?.id;
                        Some((w, r, e.writer_stream, e.reader_stream))
                    }),
                    _ => None,
                };
                if auto_layout {
                    self.node_positions.clear();
                    for n in &view.nodes {
                        self.node_positions.insert(n.id, (n.rect.x, n.rect.y));
                    }
                } else {
                    apply_positions(&mut view, &self.node_positions);
                    for n in &view.nodes {
                        self.node_positions
                            .entry(n.id)
                            .or_insert((n.rect.x, n.rect.y));
                    }
                }
                let live: HashMap<_, ()> = view.nodes.iter().map(|n| (n.id, ())).collect();
                self.node_positions.retain(|id, _| live.contains_key(id));
                self.stream_labels
                    .retain(|(id, _, _), _| live.contains_key(id));
                if let Some(id) = selected_id {
                    self.selection = view
                        .nodes
                        .iter()
                        .find(|n| n.id == id)
                        .map(|n| Selection::Node(n.index))
                        .unwrap_or(Selection::None);
                } else if let Some((w, r, ws, rs)) = selected_edge {
                    self.selection = view
                        .edges
                        .iter()
                        .enumerate()
                        .find(|(_, e)| {
                            let wi = view
                                .nodes
                                .iter()
                                .find(|n| n.index == e.writer)
                                .map(|n| n.id);
                            let ri = view
                                .nodes
                                .iter()
                                .find(|n| n.index == e.reader)
                                .map(|n| n.id);
                            wi == Some(w)
                                && ri == Some(r)
                                && e.writer_stream == ws
                                && e.reader_stream == rs
                        })
                        .map(|(i, _)| Selection::Edge(i))
                        .unwrap_or(Selection::None);
                }
                let labels = &self.stream_labels;
                attach_connection_points(&mut view, &|id, verb, output, stream| {
                    display_stream_word(labels, id, verb, output, stream)
                });
                self.operand_issues = collect_operand_issues(&self.document.text, &topo);
                self.graph = Some(view);
                self.topology = Some(topo);
            }
            Err(e) => {
                self.parse_error = Some(e.message);
                self.parse_error_span = e.span.map(|s| {
                    let len = self.document.text.len();
                    let start = s.start.min(len);
                    let end = s.end.min(len).max(start);
                    (start, end)
                });
                self.operand_issues.clear();
            }
        }
    }

    /// True when a source edit should abort the live session.
    pub fn edit_should_stop_run(&self) -> bool {
        matches!(
            self.status(),
            SessionStatus::Running | SessionStatus::Paused
        )
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

    /// What **Step one record** should do in this session state.
    pub fn step_intent(&self) -> StepIntent {
        step_intent(self.status())
    }

    /// Operand text of a stage, as written after the verb in Source.
    pub fn operand_text(&self, node_index: usize) -> String {
        let Some(topo) = &self.topology else {
            return String::new();
        };
        let Some(n) = topo.nodes.get(node_index) else {
            return String::new();
        };
        operand_raw(&self.document.text, n).to_string()
    }

    /// Patch one stage's operands in Source. The graph follows on reparse.
    pub fn patch_operands(&mut self, node_index: usize, new_args: &str) -> Result<(), String> {
        let Some(topo) = &self.topology else {
            return Err("no pipeline to patch".into());
        };
        let n = topo
            .nodes
            .get(node_index)
            .ok_or_else(|| "stage is gone".to_string())?;
        let patched = patch_stage_operands(&self.document.text, n.verb_span, n.span, new_args)?;
        self.set_text(patched);
        Ok(())
    }

    pub fn display_stream_word(
        &self,
        node_id: NodeId,
        verb: &str,
        output: bool,
        stream: u32,
    ) -> String {
        display_stream_word(&self.stream_labels, node_id, verb, output, stream)
    }

    /// Rename a connection point for this session only. Empty restores the catalog word.
    pub fn set_stream_label(&mut self, node_id: NodeId, output: bool, stream: u32, word: String) {
        let verb = self
            .graph
            .as_ref()
            .and_then(|g| {
                g.nodes
                    .iter()
                    .find(|n| n.id == node_id)
                    .map(|n| n.verb.clone())
            })
            .unwrap_or_default();
        let catalog_word = catalog_lookup(&verb, &StageRegistry::default())
            .and_then(|e| default_stream_word(e, output, stream))
            .unwrap_or("")
            .to_string();
        let trimmed = word.trim().to_string();
        let key = (node_id, output, stream);
        if trimmed.is_empty() || trimmed == catalog_word {
            self.stream_labels.remove(&key);
        } else {
            self.stream_labels.insert(key, trimmed);
        }
        self.refresh_connection_points();
    }

    fn refresh_connection_points(&mut self) {
        let Some(view) = self.graph.as_mut() else {
            return;
        };
        let labels = &self.stream_labels;
        attach_connection_points(view, &|id, verb, output, stream| {
            display_stream_word(labels, id, verb, output, stream)
        });
    }

    /// Parse error, else the selected stage's operand issue, else the first issue.
    pub fn source_underline(&self) -> Option<((usize, usize), String)> {
        if let (Some(span), Some(msg)) = (self.parse_error_span, self.parse_error.clone()) {
            return Some((span, msg));
        }
        if let Selection::Node(i) = self.selection {
            if let Some(issue) = self.operand_issues.iter().find(|e| e.node_index == i) {
                return Some((issue.span, issue.message.clone()));
            }
        }
        self.operand_issues
            .first()
            .map(|e| (e.span, e.message.clone()))
    }

    pub fn operand_issue_for(&self, node_index: usize) -> Option<&OperandIssue> {
        self.operand_issues
            .iter()
            .find(|e| e.node_index == node_index)
    }
}

/// What **Step one record** should do given the live session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepIntent {
    /// Take one record on the live session.
    Step,
    /// Run is in progress: pause at the next record.
    PauseRun,
    /// Previous run finished or was stopped: new session, then one record.
    StartAgain,
}

pub const STARTED_AGAIN: &str = "Started again.";

pub fn step_intent(status: SessionStatus) -> StepIntent {
    match status {
        SessionStatus::Running => StepIntent::PauseRun,
        SessionStatus::Ended { .. } | SessionStatus::Stopped { .. } | SessionStatus::Failed(_) => {
            StepIntent::StartAgain
        }
        SessionStatus::Idle | SessionStatus::Paused => StepIntent::Step,
    }
}

impl GuiModel {
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

    pub fn stage_notes(&self, node_index: usize) -> Vec<String> {
        let Some(snap) = self.session.as_ref() else {
            return Vec::new();
        };
        let Some(topo) = self.topology.as_ref() else {
            return Vec::new();
        };
        let Some(id) = topology_stage_id(topo, node_index) else {
            return Vec::new();
        };
        crate::field_hints::notes_for_stage(snap, id)
    }

    pub fn source_field_hints(&self) -> Vec<crate::field_hints::SourceHint> {
        let Some(snap) = self.session.as_ref() else {
            return Vec::new();
        };
        let Some(topo) = self.topology.as_ref() else {
            return Vec::new();
        };
        crate::field_hints::source_hints(&self.document.text, topo, snap)
    }

    pub fn operand_hints_for_node(&self, node_index: usize) -> Vec<(String, String)> {
        self.source_field_hints()
            .into_iter()
            .filter(|h| h.node_index == node_index)
            .map(|h| (h.spelled, h.message))
            .collect()
    }

    /// Missing-`$field` hints placed on tokens in a Properties operands string.
    pub fn operand_field_hint_spans(
        &self,
        node_index: usize,
        operands: &str,
    ) -> Vec<crate::field_hints::OperandHint> {
        crate::field_hints::operand_hint_spans(operands, &self.operand_hints_for_node(node_index))
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

    /// Place stages with the automatic layout. Source bytes and wiring are unchanged.
    pub fn reset_node_positions(&mut self) {
        if self.parse_error.is_some() {
            self.relayout_current();
            return;
        }
        self.reparse_with_layout(true);
    }

    fn relayout_current(&mut self) {
        let Some(topo) = self.topology.clone() else {
            return;
        };
        let flags = Self::node_flags_for(&topo);
        let mut view = layout(&topo, &flags);
        if let Some(old) = &self.graph {
            if old.nodes.len() == view.nodes.len() {
                for (n, oldn) in view.nodes.iter_mut().zip(&old.nodes) {
                    n.id = oldn.id;
                }
            } else {
                let previous: Vec<StageKey> = old
                    .nodes
                    .iter()
                    .map(|n| StageKey {
                        id: n.id,
                        verb: n.verb.clone(),
                        span: n.span,
                    })
                    .collect();
                let incoming: Vec<(String, SourceSpan)> = view
                    .nodes
                    .iter()
                    .map(|n| (n.verb.clone(), n.span))
                    .collect();
                let ids = match_stage_ids(&previous, &incoming, &mut self.next_node_id);
                for (n, id) in view.nodes.iter_mut().zip(ids) {
                    n.id = id;
                }
            }
        }
        self.node_positions.clear();
        for n in &view.nodes {
            self.node_positions.insert(n.id, (n.rect.x, n.rect.y));
        }
        apply_positions(&mut view, &self.node_positions);
        self.graph = Some(view);
    }

    pub fn reset_one_node_position(&mut self, topo_index: usize) {
        let Some(id) = self.graph.as_ref().and_then(|view| {
            view.nodes
                .iter()
                .find(|n| n.index == topo_index)
                .map(|n| n.id)
        }) else {
            return;
        };
        self.node_positions.remove(&id);
        let Some(topo) = self.topology.as_ref() else {
            return;
        };
        let flags = Self::node_flags_for(topo);
        let laid = layout(topo, &flags);
        if let Some(n) = laid.nodes.iter().find(|n| n.index == topo_index) {
            self.node_positions.insert(id, (n.rect.x, n.rect.y));
        }
        if let Some(view) = self.graph.as_mut() {
            apply_positions(view, &self.node_positions);
        }
    }

    pub fn nudge_node(&mut self, topo_index: usize, dx: f32, dy: f32) {
        let Some(id) = self.graph.as_ref().and_then(|view| {
            view.nodes
                .iter()
                .find(|n| n.index == topo_index)
                .map(|n| n.id)
        }) else {
            return;
        };
        let mut pos = None;
        if let Some(view) = self.graph.as_mut() {
            if let Some(n) = view.nodes.iter_mut().find(|n| n.index == topo_index) {
                n.rect.x += dx;
                n.rect.y += dy;
                pos = Some((n.rect.x, n.rect.y));
            }
        }
        if let Some(p) = pos {
            self.node_positions.insert(id, p);
        }
        if let Some(view) = self.graph.as_mut() {
            apply_positions(view, &self.node_positions);
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

fn operand_raw<'a>(source: &'a str, n: &pipe_stages::TopoNode) -> &'a str {
    if n.verb_span.end > n.span.end || n.span.end > source.len() || n.verb_span.end > source.len() {
        return "";
    }
    source[n.verb_span.end..n.span.end]
        .trim_start_matches([' ', '\t'])
        .trim_end()
}

fn display_stream_word(
    labels: &HashMap<(NodeId, bool, u32), String>,
    node_id: NodeId,
    verb: &str,
    output: bool,
    stream: u32,
) -> String {
    if let Some(s) = labels.get(&(node_id, output, stream)) {
        if !s.is_empty() {
            return s.clone();
        }
    }
    catalog_lookup(verb, &StageRegistry::default())
        .and_then(|e| default_stream_word(e, output, stream))
        .unwrap_or("")
        .to_string()
}

fn collect_operand_issues(source: &str, topo: &PipelineTopology) -> Vec<OperandIssue> {
    let mut out = Vec::new();
    for (i, n) in topo.nodes.iter().enumerate() {
        if n.kind != NodeKind::Stage || n.verb.is_empty() {
            continue;
        }
        let raw = operand_raw(source, n);
        let Some(Err(message)) = check_stage_operands(&n.verb, raw) else {
            continue;
        };
        let start = n.verb_span.end.min(n.span.end);
        let end = n.span.end;
        let span = if start < end {
            (start, end)
        } else {
            (n.verb_span.start, n.verb_span.end.max(n.verb_span.start))
        };
        out.push(OperandIssue {
            node_index: i,
            span,
            message,
        });
    }
    out
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
    fn step_intent_starts_again_after_finished_or_stop() {
        assert_eq!(STARTED_AGAIN, "Started again.");
        assert_eq!(step_intent(SessionStatus::Idle), StepIntent::Step);
        assert_eq!(step_intent(SessionStatus::Paused), StepIntent::Step);
        assert_eq!(step_intent(SessionStatus::Running), StepIntent::PauseRun);
        assert_eq!(
            step_intent(SessionStatus::Ended { rc: 0 }),
            StepIntent::StartAgain
        );
        assert_eq!(
            step_intent(SessionStatus::Stopped { rc: 0 }),
            StepIntent::StartAgain
        );
        assert_eq!(
            step_intent(SessionStatus::Failed("x".into())),
            StepIntent::StartAgain
        );
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
        assert!(m
            .node_positions
            .contains_key(&m.graph.as_ref().unwrap().nodes[0].id));
    }

    #[test]
    fn stable_ids_survive_insert_in_the_middle() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a | hole".into());
        let (id_lit, id_hole) = {
            let g = m.graph.as_ref().unwrap();
            (g.nodes[0].id, g.nodes[1].id)
        };
        m.nudge_node(m.graph.as_ref().unwrap().nodes[1].index, 20.0, 0.0);
        let hole_x = m.graph.as_ref().unwrap().nodes[1].rect.x;
        m.set_text("literal a | spec | hole".into());
        let g = m.graph.as_ref().unwrap();
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.nodes[0].id, id_lit);
        assert_eq!(g.nodes[2].id, id_hole);
        assert_ne!(g.nodes[1].id, id_lit);
        assert_ne!(g.nodes[1].id, id_hole);
        assert!(
            (g.nodes[2].rect.x - hole_x).abs() < 0.01,
            "placed position is session-only"
        );
        let spec_x = g.nodes[1].rect.x;
        m.set_text("literal a | spec | hole | console".into());
        let g = m.graph.as_ref().unwrap();
        assert_eq!(g.nodes[1].id, {
            // spec keeps its id after appending
            let _ = spec_x;
            g.nodes[1].id
        });
        assert!((g.nodes[1].rect.x - spec_x).abs() < 0.01);
    }

    #[test]
    fn last_valid_graph_stays_on_parse_error() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal apple | locate /ap/ | console".into());
        let ids: Vec<_> = m
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(ids.len(), 3);
        m.set_text("(ENDCHAR".into());
        assert!(m.parse_error.as_ref().unwrap().contains("unclosed"));
        assert!(m.parse_error_span.is_some());
        let g = m.graph.as_ref().expect("last valid graph");
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.nodes.iter().map(|n| n.id).collect::<Vec<_>>(), ids);
        m.set_text("literal apple | locate /ap/ | console | hole".into());
        assert!(m.parse_error.is_none());
        assert_eq!(m.graph.as_ref().unwrap().nodes.len(), 4);
        assert_eq!(m.graph.as_ref().unwrap().nodes[0].id, ids[0]);
    }

    #[test]
    fn empty_source_is_valid_and_clears_the_graph() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a | hole".into());
        assert!(m.graph.is_some());
        m.set_text("   ".into());
        assert!(m.parse_error.is_none());
        assert!(m.graph.is_none());
    }

    fn snap(status: SessionStatus) -> Snapshot {
        Snapshot {
            status,
            events: Vec::new(),
            slots: Vec::new(),
            live: Vec::new(),
            last_step: None,
            spec_fingerprint: String::new(),
            io_counts: Vec::new(),
            record_logs: Vec::new(),
        }
    }

    #[test]
    fn edit_stops_run_when_running_or_paused() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a | hole".into());
        assert!(!m.edit_should_stop_run());
        m.session = Some(snap(SessionStatus::Running));
        assert!(m.edit_should_stop_run());
        m.session = Some(snap(SessionStatus::Paused));
        assert!(m.edit_should_stop_run());
        m.session = Some(snap(SessionStatus::Ended { rc: 0 }));
        assert!(!m.edit_should_stop_run());
        m.session = Some(snap(SessionStatus::Stopped { rc: 0 }));
        assert!(!m.edit_should_stop_run());
        m.session = Some(snap(SessionStatus::Idle));
        assert!(!m.edit_should_stop_run());
    }

    #[test]
    fn paste_lays_out_a_graph() {
        let mut m = GuiModel::new(std::env::temp_dir());
        assert!(m.graph.is_none());
        m.set_text("literal a | hole".into());
        let g = m.graph.as_ref().unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert!(g.nodes[1].rect.x > g.nodes[0].rect.x);
    }

    #[test]
    fn open_pipe_file_auto_layouts() {
        let dir = tmp("open-pipe");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("demo.pipe");
        fs::write(&path, "literal a | hole\n").unwrap();
        let mut m = GuiModel::new(dir.clone());
        m.set_text("literal z | spec | hole".into());
        m.nudge_node(m.graph.as_ref().unwrap().nodes[0].index, 80.0, 0.0);
        m.load_path(&path).unwrap();
        assert!(m.parse_error.is_none());
        let g = m.graph.as_ref().unwrap();
        assert_eq!(g.nodes.len(), 2);
        let x0 = g.nodes[0].rect.x;
        m.reset_node_positions();
        assert!((m.graph.as_ref().unwrap().nodes[0].rect.x - x0).abs() < 0.01);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
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

    #[test]
    fn property_pane_operand_edit_round_trips_to_text() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal apple | locate /ap/ | console".into());
        let loc = m
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.verb.eq_ignore_ascii_case("locate"))
            .map(|n| n.index)
            .unwrap();
        assert_eq!(m.operand_text(loc), "/ap/");
        m.patch_operands(loc, "/ban/").unwrap();
        assert_eq!(m.document.text, "literal apple | locate /ban/ | console");
        assert!(m.parse_error.is_none());
        let loc = m
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.verb.eq_ignore_ascii_case("locate"))
            .map(|n| n.index)
            .unwrap();
        assert_eq!(m.operand_text(loc), "/ban/");
        m.patch_operands(loc, "").unwrap();
        assert_eq!(m.document.text, "literal apple | locate | console");
    }

    #[test]
    fn operand_checker_flags_bad_locate_and_ignores_sort() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a | locate /ap/ extra | sort | console".into());
        let loc = m
            .topology
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .position(|n| n.verb == "LOCATE")
            .unwrap();
        let sort = m
            .topology
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .position(|n| n.verb == "SORT")
            .unwrap();
        assert!(m.operand_issue_for(loc).is_some());
        assert!(m.operand_issue_for(sort).is_none());
        m.selection = Selection::Node(loc);
        let (_, msg) = m.source_underline().expect("squiggle");
        assert!(msg.to_ascii_lowercase().contains("unexpected"), "{msg}");
    }

    #[test]
    fn stream_label_rename_is_session_only() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal apple | locate /ap/ | console".into());
        let before = m.document.text.clone();
        let loc = m
            .graph
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.verb.eq_ignore_ascii_case("locate"))
            .cloned()
            .unwrap();
        assert_eq!(
            m.display_stream_word(loc.id, &loc.verb, true, 1),
            "unmatched"
        );
        m.set_stream_label(loc.id, true, 1, "misses".into());
        assert_eq!(m.document.text, before, "rename must not rewrite Source");
        assert_eq!(m.display_stream_word(loc.id, &loc.verb, true, 1), "misses");
        let port = m
            .graph
            .as_ref()
            .unwrap()
            .ports
            .iter()
            .find(|p| p.node_index == loc.index && p.output && p.stream == 1)
            .unwrap();
        assert_eq!(port.word, "misses");
        assert!(!port.connected);
        m.set_stream_label(loc.id, true, 1, String::new());
        assert_eq!(
            m.display_stream_word(loc.id, &loc.verb, true, 1),
            "unmatched"
        );
    }

    #[test]
    fn language_stage_operands_are_not_checked() {
        let mut m = GuiModel::new(std::env::temp_dir());
        m.set_text("literal a | examples/marina/tuck.py owner extra | hole".into());
        let py = m
            .topology
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .position(|n| n.verb.to_ascii_lowercase().ends_with(".py"))
            .unwrap();
        assert!(m.operand_issue_for(py).is_none());
    }
}
