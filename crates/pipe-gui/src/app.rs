use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Key, Layout, Margin, PointerButton, Pos2, Rect,
    RichText, Sense, Stroke, TextEdit, Ui, Vec2,
};
use pipe_core::SessionStatus;
use pipe_stages::{catalog_lookup, is_language_stage_verb, NodeKind, StageRegistry};

use crate::edit::{self, DragTargetKind, DropTarget, WireEnd};
use crate::files::{default_browse_dir, ensure_pipe_extension, list_dir, native_open, native_save};
use crate::graph::{
    edge_is_related, ellipsize_label, hit_edge_route, hit_node, hit_port, node_is_related,
    route_all, route_label_anchor, stream_caption, GraphView, LayoutDirection, LABEL_ELLIPSIS,
};
use crate::hover::{
    hover_bridge, keep_open, place_adjacent, pointer_in_rect, RetainInput, GRACE, OPEN_DELAY,
    PREVIEW_MAX,
};
use crate::ids::NodeId;
use crate::inspector::ScriptIdentity;
use crate::model::{GuiModel, PickerKind, Selection, StepIntent, STARTED_AGAIN};
use crate::preview::{
    edge_hover_peek, edge_sent_count, in_badge_label, out_badge_label, stage_hover_peek, HoverPeek,
    StageBadges,
};
use crate::report_error;
use crate::settings::{RecordsChrome, ShowRecords, ON_GRAPH_RECORDS_HEIGHT};
use crate::symbols::{paint_mark, show_legend, RecordMark};
use crate::theme;
use crate::worker::{SessionWorker, WorkerCmd, WorkerEvent, SHUTDOWN_JOIN};

pub struct GuiApp {
    pub model: GuiModel,
    worker: SessionWorker,
    cam: Camera,
    screenshot: Option<PathBuf>,
    screenshot_frames: u32,
    screenshot_done: bool,
    frames: u32,
    fit_graph: bool,
    auto_run: bool,
    auto_run_done: bool,
    auto_step: bool,
    auto_step_done: bool,
    quit_after_ms: Option<u64>,
    started: Instant,
    shutting_down: bool,
    shutdown_at: Option<Instant>,
    panning: bool,
    dragging_node: Option<usize>,
    drag_accum: f32,
    hover: Selection,
    context_target: Selection,
    source_sync_span: Option<(usize, usize)>,
    hover_since: Option<Instant>,
    over_badge: bool,
    preview: PreviewBubble,
    show_records: ShowRecords,
    hover_verb: Option<String>,
    forced_hover: bool,
    edit_to: Option<String>,
    edit_to_applied: bool,
    props_node: Option<NodeId>,
    props_operands: String,
    props_streams: HashMap<(bool, u32), String>,
    palette_drag: Option<String>,
    forced_drag_verb: Option<String>,
    forced_drag_kind: DragTargetKind,
    drop_target: Option<DropTarget>,
    drag_graph_pos: Option<(f32, f32)>,
    wiring: Option<WireEnd>,
    wire_target: Option<WireEnd>,
    wire_pos: Option<(f32, f32)>,
    forced_wire_from: Option<String>,
    forced_wire_to: Option<String>,
    place_on: Option<String>,
    apply_place: bool,
    apply_place_done: bool,
    delete_verb: Option<String>,
    delete_applied: bool,
    drop_verbs: Option<Vec<String>>,
    drop_verbs_done: bool,
}

/// Short delay before the graph peek appears, so crossing the canvas is quiet.
const PEEK_DELAY: Duration = Duration::from_millis(280);

/// Interactive record preview on the graph. Hover delay, then stays while
/// the pointer is on the anchor, the popup, or the bridge between them.
/// A click pins it until empty-canvas, Close, or Escape.
struct PreviewBubble {
    target: Selection,
    pinned: bool,
    hover_since: Option<Instant>,
    last_inside: Option<Instant>,
    pos: Pos2,
    area_rect: Option<Rect>,
    anchor_rect: Option<Rect>,
    last_size: Option<Vec2>,
    drag_in_popup: bool,
    just_pinned: bool,
}

impl Default for PreviewBubble {
    fn default() -> Self {
        Self {
            target: Selection::None,
            pinned: false,
            hover_since: None,
            last_inside: None,
            pos: Pos2::new(420.0, 120.0),
            area_rect: None,
            anchor_rect: None,
            last_size: None,
            drag_in_popup: false,
            just_pinned: false,
        }
    }
}

impl PreviewBubble {
    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy)]
struct Camera {
    pan: Vec2,
    zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan: Vec2::new(12.0, 12.0),
            zoom: 1.0,
        }
    }
}

#[derive(Clone)]
pub struct LaunchCfg {
    pub file: Option<PathBuf>,
    pub spec: Option<String>,
    pub cwd: PathBuf,
    pub screenshot: Option<PathBuf>,
    pub screenshot_frames: u32,
    pub auto_run: bool,
    pub auto_step: bool,
    pub quit_after_ms: Option<u64>,
    pub show_unsaved: bool,
    pub show_script: bool,
    pub select_verb: Option<String>,
    pub select_edge: Option<usize>,
    pub orthogonal: Option<bool>,
    pub show_records: Option<ShowRecords>,
    pub hover_verb: Option<String>,
    pub col_widths: Option<Vec<f32>>,
    pub edit_to: Option<String>,
    pub drag_verb: Option<String>,
    pub drag_target: Option<String>,
    pub apply_place: bool,
    pub delete_verb: Option<String>,
    pub wire_from: Option<String>,
    pub wire_to: Option<String>,
    pub place_on: Option<String>,
    pub layout: Option<LayoutDirection>,
    pub drop_verbs: Option<Vec<String>>,
}

impl GuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cfg: LaunchCfg) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut model = GuiModel::new(cfg.cwd);
        if let Some(path) = &cfg.file {
            if let Err(e) = model.load_path(path) {
                model.push_log(e);
            }
        } else if let Some(spec) = cfg.spec {
            model.document.text = spec;
            model.reparse();
            model.push_log("loaded specification from the command line (not executed)");
        } else {
            model.push_log(
                "Drag a stage from the palette, paste a specification, or open a file. Run is never automatic.",
            );
        }
        if let Some(ortho) = cfg.orthogonal {
            model.orthogonal_edges = ortho;
        }
        if let Some(dir) = cfg.layout {
            model.layout_direction = dir;
            if model.graph.is_some() {
                model.reset_node_positions();
            }
        }
        if let Some(verb) = &cfg.select_verb {
            if let Some(topo) = &model.topology {
                let key = verb.to_ascii_uppercase();
                let exact = topo
                    .nodes
                    .iter()
                    .position(|n| n.verb.eq_ignore_ascii_case(verb));
                let named = topo.nodes.iter().position(|n| {
                    n.verb.to_ascii_uppercase().contains(&key)
                        || n.label
                            .as_deref()
                            .is_some_and(|l| l.eq_ignore_ascii_case(verb))
                });
                if let Some(i) = exact.or(named) {
                    model.selection = Selection::Node(i);
                }
            }
        }
        if let Some(ei) = cfg.select_edge {
            if model.graph.as_ref().is_some_and(|g| ei < g.edges.len()) {
                model.selection = Selection::Edge(ei);
            }
        }
        if cfg.show_unsaved {
            model.document.text.push_str("\n");
            model.pending_close = true;
        }
        if cfg.show_script {
            let verb = match model.selection {
                Selection::Node(i) => model
                    .topology
                    .as_ref()
                    .and_then(|t| t.nodes.get(i).map(|n| n.verb.clone())),
                _ => cfg.select_verb.clone(),
            };
            if let Some(v) = verb {
                model.open_script_source(&v);
            }
        }
        let app = Self {
            model,
            worker: SessionWorker::start(),
            cam: Camera::default(),
            screenshot: cfg.screenshot,
            screenshot_frames: cfg.screenshot_frames.max(2),
            screenshot_done: false,
            frames: 0,
            fit_graph: true,
            auto_run: cfg.auto_run,
            auto_run_done: false,
            auto_step: cfg.auto_step,
            auto_step_done: false,
            quit_after_ms: cfg.quit_after_ms,
            started: Instant::now(),
            shutting_down: false,
            shutdown_at: None,
            panning: false,
            dragging_node: None,
            drag_accum: 0.0,
            hover: Selection::None,
            context_target: Selection::None,
            source_sync_span: None,
            hover_since: None,
            over_badge: false,
            preview: PreviewBubble::default(),
            show_records: cfg
                .show_records
                .unwrap_or_else(crate::settings::load_persisted),
            hover_verb: cfg.hover_verb,
            forced_hover: false,
            edit_to: cfg.edit_to,
            edit_to_applied: false,
            props_node: None,
            props_operands: String::new(),
            props_streams: HashMap::new(),
            palette_drag: None,
            forced_drag_verb: cfg.drag_verb,
            forced_drag_kind: cfg
                .drag_target
                .as_deref()
                .and_then(DragTargetKind::parse)
                .unwrap_or(DragTargetKind::Port),
            drop_target: None,
            drag_graph_pos: None,
            wiring: None,
            wire_target: None,
            wire_pos: None,
            forced_wire_from: cfg.wire_from,
            forced_wire_to: cfg.wire_to,
            place_on: cfg.place_on,
            apply_place: cfg.apply_place,
            apply_place_done: false,
            delete_verb: cfg.delete_verb,
            delete_applied: false,
            drop_verbs: cfg.drop_verbs,
            drop_verbs_done: false,
        };
        if let Some(ws) = &cfg.col_widths {
            crate::sheet::seed_session_widths(ws);
        }
        app
    }

    fn begin_shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        self.shutdown_at = Some(Instant::now());
        self.worker.request_stop();
        self.worker.send(WorkerCmd::Shutdown);
        self.model.push_log(
            "Quit: stopping the session (abort, not run-to-completion). Already-written files stay written.",
        );
    }

    fn drain_worker(&mut self) {
        for ev in self.worker.try_recv() {
            match ev {
                WorkerEvent::Snapshot(s) => {
                    if let Some(note) = s.last_step.as_ref() {
                        self.model.push_log(note.clone());
                    }
                    self.model.session = Some(s);
                }
                WorkerEvent::Console(line) => {
                    self.model.console.push(line);
                    if self.model.console.len() > 400 {
                        self.model.console.remove(0);
                    }
                }
                WorkerEvent::Message(line) | WorkerEvent::Info(line) => {
                    self.model.push_log(line);
                }
                WorkerEvent::Error(line) => {
                    self.model.push_log(format!("error: {line}"));
                }
            }
        }
    }

    fn prepare_session(&mut self) {
        self.model.console.clear();
        self.model.step_count = 0;
        // Drop the live snapshot immediately so graph marks, counts, and the
        // Records pane reset while the worker builds the new session.
        self.model.session = None;
        self.model.session_spec = Some(self.model.document.text.clone());
        self.model.edit_stopped_run = false;
        self.worker.send(WorkerCmd::Prepare {
            spec: self.model.document.text.clone(),
            cwd: self.model.cwd.clone(),
        });
    }

    fn start_again(&mut self) {
        self.prepare_session();
        self.model.push_log(STARTED_AGAIN);
    }

    fn on_restart(&mut self) {
        if self.model.parse_error.is_some() {
            self.model.push_log("fix the parse error before Restart");
            return;
        }
        if self.model.document.text.trim().is_empty() {
            self.model.push_log("nothing to restart");
            return;
        }
        self.start_again();
    }

    fn on_run(&mut self) {
        if self.model.parse_error.is_some() {
            self.model.push_log("fix the parse error before Run");
            return;
        }
        if self.model.document.text.trim().is_empty() {
            self.model.push_log("nothing to run");
            return;
        }
        let again = self.model.status().is_terminal();
        let restart =
            self.model.session.is_none() || self.model.source_changed_since_session() || again;
        if restart {
            if again {
                self.start_again();
            } else {
                self.prepare_session();
            }
            self.model
                .push_log("Run requested. Side effects happen now if the spec writes files.");
        } else {
            self.model
                .push_log("Continuing this session (same console; not a fresh run).");
        }
        self.worker.send(WorkerCmd::Run);
    }

    fn on_pause(&mut self) {
        self.worker.request_pause();
        self.worker.send(WorkerCmd::Pause);
        self.model.push_log(
            "Pause requested. Takes effect at the next StageCtx read, write, or short. A blocking host file syscall already in flight is not interrupted.",
        );
    }

    fn on_stop(&mut self) {
        self.worker.request_stop();
        self.model
            .push_log("Stop requested. Already-written host files stay written.");
    }

    fn on_step(&mut self) {
        if self.model.parse_error.is_some() {
            self.model.push_log("fix the parse error before Step");
            return;
        }
        if self.model.document.text.trim().is_empty() {
            self.model.push_log("nothing to step");
            return;
        }
        match self.model.step_intent() {
            StepIntent::PauseRun => {
                self.worker.request_pause();
                self.worker.send(WorkerCmd::Pause);
                self.model.push_log("Step: pausing at the next record.");
                return;
            }
            StepIntent::StartAgain => {
                self.start_again();
            }
            StepIntent::Step => {
                if self.model.session.is_none() || self.model.source_changed_since_session() {
                    self.prepare_session();
                }
            }
        }
        self.worker.send(WorkerCmd::Step {
            stage: self.model.preferred_step_stage(),
        });
        self.model.step_count = self.model.step_count.saturating_add(1);
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::apply(ctx);
        if !self.edit_to_applied {
            if let Some(text) = self.edit_to.take() {
                self.model.set_text(text);
                self.edit_to_applied = true;
            } else {
                self.edit_to_applied = true;
            }
        }
        if !self.drop_verbs_done {
            if let Some(verbs) = self.drop_verbs.clone() {
                for (i, verb) in verbs.iter().enumerate() {
                    if i == 0 {
                        let _ = self.model.place_from_palette(
                            verb,
                            DropTarget::Canvas,
                            Some((100.0, 80.0)),
                        );
                        continue;
                    }
                    let prev = verbs[i - 1].clone();
                    let (idx, pos) = self
                        .model
                        .graph
                        .as_ref()
                        .and_then(|view| {
                            let n = view
                                .nodes
                                .iter()
                                .rev()
                                .find(|n| n.verb.eq_ignore_ascii_case(&prev))?;
                            let idx = n.index;
                            let pos = view
                                .ports
                                .iter()
                                .find(|p| p.node_index == idx && p.output && p.stream == 0)
                                .map(|p| p.pos)
                                .unwrap_or((n.rect.x + n.rect.w, n.rect.y + n.rect.h * 0.5));
                            Some((idx, pos))
                        })
                        .unwrap_or((0, (100.0, 80.0)));
                    let _ = self.model.place_from_palette(
                        verb,
                        DropTarget::Port {
                            node_index: idx,
                            output: true,
                            stream: 0,
                        },
                        Some(pos),
                    );
                }
                self.fit_graph = true;
            }
            self.drop_verbs_done = true;
        }
        if self.model.graph.is_some() {
            if self.apply_place && !self.apply_place_done {
                if let Some(verb) = self.forced_drag_verb.clone() {
                    let target = if let Some(on) = self.place_on.clone() {
                        self.resolve_wire_end(&on)
                            .map(|w| DropTarget::Port {
                                node_index: w.node_index,
                                output: w.output,
                                stream: w.stream,
                            })
                            .unwrap_or(DropTarget::Canvas)
                    } else {
                        match self.forced_drag_kind {
                            DragTargetKind::Edge => DropTarget::Edge { edge_index: 0 },
                            DragTargetKind::Port => {
                                if let Some(view) = self.model.graph.as_ref() {
                                    view.ports
                                        .iter()
                                        .find(|p| p.output && p.stream == 0)
                                        .map(|p| DropTarget::Port {
                                            node_index: p.node_index,
                                            output: true,
                                            stream: 0,
                                        })
                                        .unwrap_or(DropTarget::Canvas)
                                } else {
                                    DropTarget::Canvas
                                }
                            }
                            DragTargetKind::Canvas => DropTarget::Canvas,
                        }
                    };
                    self.drop_palette_verb(&verb, target, None);
                    self.forced_drag_verb = None;
                    self.fit_graph = true;
                }
                self.apply_place_done = true;
            }
            if !self.delete_applied {
                if let Some(verb) = self.delete_verb.take() {
                    if let Some(view) = self.model.graph.as_ref() {
                        let key = verb.to_ascii_uppercase();
                        if let Some(n) = view
                            .nodes
                            .iter()
                            .find(|n| n.verb.to_ascii_uppercase() == key)
                        {
                            let i = n.index;
                            self.stop_run_if_editing();
                            if let Err(e) = self.model.delete_stage(i) {
                                self.model.push_log(e);
                            }
                        }
                    }
                    self.delete_applied = true;
                } else {
                    self.delete_applied = true;
                }
            }
        }
        self.drain_worker();
        self.frames = self.frames.saturating_add(1);
        ctx.request_repaint_after(std::time::Duration::from_millis(50));

        let mut quit = false;
        egui::TopBottomPanel::top("chrome")
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(10)))
            .show(ctx, |ui| {
                self.chrome(ui, &mut quit);
            });

        egui::TopBottomPanel::bottom("legend")
            .exact_height(if self.model.legend_expanded {
                28.0
            } else {
                24.0
            })
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(8, 3)),
            )
            .show(ctx, |ui| {
                show_legend(ui, &mut self.model.legend_expanded);
            });

        egui::TopBottomPanel::bottom("console")
            .resizable(true)
            .default_height(100.0)
            .min_height(64.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                self.console_panel(ui);
            });

        match self.show_records.chrome(self.model.inspector_open) {
            RecordsChrome::Grids => {
                egui::TopBottomPanel::bottom("records")
                    .resizable(true)
                    .default_height(240.0)
                    .min_height(120.0)
                    .frame(
                        Frame::new()
                            .fill(theme::PANEL)
                            .inner_margin(Margin::same(8)),
                    )
                    .show(ctx, |ui| {
                        self.inspector(ui);
                    });
            }
            RecordsChrome::Collapsed => {
                egui::TopBottomPanel::bottom("records-bar")
                    .exact_height(28.0)
                    .frame(
                        Frame::new()
                            .fill(theme::PANEL)
                            .inner_margin(Margin::symmetric(8, 4)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Records").strong().small());
                            ui.label(
                                RichText::new("collapsed — graph keeps the space")
                                    .small()
                                    .color(theme::MUTED),
                            );
                            if ui.small_button("Show").clicked() {
                                self.model.inspector_open = true;
                            }
                        });
                    });
            }
            RecordsChrome::OnGraphHint => {
                // Separate id so the slim bar does not overwrite the
                // resizable Records pane height in egui memory.
                egui::TopBottomPanel::bottom("records_on_graph")
                    .exact_height(ON_GRAPH_RECORDS_HEIGHT)
                    .resizable(false)
                    .frame(
                        Frame::new()
                            .fill(theme::PANEL)
                            .inner_margin(Margin::symmetric(8, 6)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Records").strong());
                            ui.add(
                                egui::Label::new(
                                    RichText::new(ShowRecords::ON_GRAPH_PANE_HINT)
                                        .small()
                                        .color(theme::MUTED),
                                )
                                .truncate(),
                            );
                        });
                    });
            }
        }

        egui::SidePanel::left("source")
            .resizable(true)
            .default_width(380.0)
            .min_width(240.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                self.source_panel(ui);
            });

        egui::SidePanel::left("palette")
            .resizable(true)
            .default_width(220.0)
            .min_width(160.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                self.palette_panel(ui);
            });

        egui::SidePanel::right("properties")
            .resizable(true)
            .default_width(300.0)
            .min_width(220.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                self.properties_panel(ui);
            });

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(0)))
            .show(ctx, |ui| {
                self.graph_panel(ui);
            });

        self.maybe_graph_tooltip(ctx);

        if self.model.picker.open {
            self.picker_modal(ctx);
        }

        if self.model.pending_close {
            self.unsaved_modal(ctx, &mut quit);
        }

        if self.model.script_source.is_some() {
            self.script_source_modal(ctx);
        }

        self.shortcuts(ctx);

        if ctx.input(|i| i.viewport().close_requested()) && !self.shutting_down {
            if self.quit_after_ms.is_some() || self.screenshot.is_some() {
                self.begin_shutdown();
            } else if self.model.close_needs_prompt() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.model.pending_close = true;
            } else {
                self.begin_shutdown();
            }
        }

        if self.auto_run && !self.auto_run_done && self.frames >= 2 && !self.shutting_down {
            self.auto_run_done = true;
            self.on_run();
        }
        if self.auto_step && !self.auto_step_done && self.frames >= 2 && !self.shutting_down {
            self.auto_step_done = true;
            self.on_step();
        }
        if let Some(ms) = self.quit_after_ms {
            if !self.shutting_down && self.started.elapsed() >= Duration::from_millis(ms) {
                quit = true;
            }
        }

        if let Some(path) = self.screenshot.clone() {
            if self.frames >= self.screenshot_frames && !self.screenshot_done {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let image = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = image {
                if let Err(e) = write_screenshot(&path, &image) {
                    eprintln!("screenshot: {e}");
                }
                self.screenshot_done = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        if quit && !self.shutting_down {
            let skip_prompt = self.quit_after_ms.is_some() || self.screenshot.is_some();
            if skip_prompt || self.model.request_close() {
                self.begin_shutdown();
            }
        }
        if self.model.close_confirmed && !self.shutting_down {
            self.begin_shutdown();
        }

        if self.shutting_down {
            if self.worker.is_finished() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self
                .shutdown_at
                .is_some_and(|t| t.elapsed() > SHUTDOWN_JOIN)
            {
                report_error(
                    "pipe-gui: the session did not stop in time after Quit; exiting so pipe --gui can return",
                );
                std::process::exit(1);
            } else {
                ctx.request_repaint();
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.worker.shutdown(SHUTDOWN_JOIN) {
            return;
        }
        report_error(
            "pipe-gui: the session thread did not stop after Quit. An in-flight host file write or helper may still be running. Exiting so pipe --gui is not stuck. Already-written files stay written; a write in flight may be truncated.",
        );
        std::process::exit(1);
    }
}

impl GuiApp {
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::Enter),
                i.key_pressed(Key::F5),
                i.key_pressed(Key::F10),
                i.modifiers.shift && i.key_pressed(Key::F5),
                i.modifiers.command && i.key_pressed(Key::S),
                i.modifiers.command && i.key_pressed(Key::O),
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
            )
        });
        if input.0 || input.1 {
            self.on_run();
        }
        if input.2 {
            self.on_step();
        }
        if input.3 {
            self.on_stop();
        }
        if input.4 {
            self.save();
        }
        if input.5 {
            self.open_picker(PickerKind::Open);
        }
        if input.6 {
            let source_focus = ctx.memory(|m| m.has_focus(egui::Id::new("pipe-source")));
            let ops_focus = ctx.memory(|m| m.has_focus(egui::Id::new("pipe-properties-operands")));
            if !source_focus && !ops_focus {
                match self.model.selection {
                    Selection::Node(i) => {
                        self.stop_run_if_editing();
                        if let Err(e) = self.model.delete_stage(i) {
                            self.model.push_log(e);
                        }
                    }
                    Selection::Edge(i) => {
                        self.stop_run_if_editing();
                        if let Err(e) = self.model.disconnect_edge(i) {
                            self.model.push_log(e);
                        }
                    }
                    Selection::None => {}
                }
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) && self.show_records.grids_on_graph() {
            self.preview.clear();
        }
    }

    fn chrome(&mut self, ui: &mut Ui, quit: &mut bool) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Portable Pipelines")
                    .font(FontId::proportional(18.0))
                    .color(theme::TEXT)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Quit").clicked() {
                    *quit = true;
                }
                if ui.button("Save as").clicked() {
                    self.open_picker(PickerKind::SaveAs);
                }
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui.button("Open").clicked() {
                    self.open_picker(PickerKind::Open);
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let path = self
                .model
                .document
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "untitled".into());
            let dirty = if self.model.dirty() {
                "  • unsaved"
            } else {
                ""
            };
            ui.label(RichText::new(format!("{path}{dirty}")).color(theme::MUTED));
            ui.separator();
            ui.label(RichText::new("working directory").color(theme::MUTED));
            ui.monospace(self.model.cwd.display().to_string());
            if ui.small_button("Change").clicked() {
                self.open_picker(PickerKind::Cwd);
            }
            ui.separator();
            ui.label(
                RichText::new(format!("newlines {}", self.model.document.newline.label()))
                    .color(theme::MUTED),
            );
            if self.model.document.utf8_bom {
                ui.label(RichText::new("UTF-8 BOM").color(theme::MUTED));
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let terminal = self.model.status().is_terminal();
            let run_label = if terminal { "Run again" } else { "  Run  " };
            let run = ui.add(
                egui::Button::new(RichText::new(run_label).strong().color(Color32::WHITE))
                    .fill(theme::ACCENT_DIM),
            );
            if terminal {
                run.clone()
                    .on_hover_text("Starts over from the beginning and runs to the end.");
            }
            if run.clicked() {
                self.on_run();
            }
            if ui.button("Pause").clicked() {
                self.on_pause();
            }
            let step_label = if terminal {
                "Step again"
            } else {
                "Step one record"
            };
            let step = ui.button(step_label);
            let step_tip = match self.model.step_intent() {
                StepIntent::StartAgain => "Starts over from the beginning and takes one record.",
                StepIntent::PauseRun => "Pause at the next record.",
                StepIntent::Step => "",
            };
            if !step_tip.is_empty() {
                step.clone().on_hover_text(step_tip);
            }
            if step.clicked() {
                self.on_step();
            }
            if ui.button("Stop").clicked() {
                self.on_stop();
            }
            let restart = ui.button("Restart");
            restart.clone().on_hover_text(
                "New session from the top. Source and layout stay. Marks, counts, and Records reset.",
            );
            if restart.clicked() {
                self.on_restart();
            }
            if ui.button("Fit").clicked() {
                self.fit_graph = true;
            }
            if ui.button("Auto layout").clicked() {
                self.model.reset_node_positions();
                self.fit_graph = true;
            }
            if ui.button("Zoom −").clicked() {
                self.cam.zoom = (self.cam.zoom * 0.9).clamp(0.45, 2.8);
            }
            if ui.button("Zoom +").clicked() {
                self.cam.zoom = (self.cam.zoom * 1.1).clamp(0.45, 2.8);
            }
            ui.label(
                RichText::new(format!("{:.0}%", self.cam.zoom * 100.0))
                    .color(theme::MUTED)
                    .small(),
            );
            ui.checkbox(&mut self.model.orthogonal_edges, "Right-angle edges");
            let mut dir = self.model.layout_direction;
            egui::ComboBox::from_id_salt("layout-dir")
                .selected_text(format!("Layout: {}", dir.label()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut dir,
                        LayoutDirection::Horizontal,
                        LayoutDirection::Horizontal.label(),
                    );
                    ui.selectable_value(
                        &mut dir,
                        LayoutDirection::Vertical,
                        LayoutDirection::Vertical.label(),
                    );
                });
            if dir != self.model.layout_direction {
                self.model.set_layout_direction(dir);
                self.fit_graph = true;
            }
            ui.separator();
            let show_lbl = ui.label(RichText::new("Show records").small().color(theme::MUTED));
            show_lbl.on_hover_text(
                "In Records pane: hover is a short peek; click for grids below. On the graph: hover shows the record grids next to the stage or connection; Records is a short hint.",
            );
            let mut mode = self.show_records;
            egui::ComboBox::from_id_salt("show-records")
                .selected_text(mode.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, ShowRecords::Pane, ShowRecords::Pane.label());
                    ui.selectable_value(&mut mode, ShowRecords::Graph, ShowRecords::Graph.label());
                });
            if mode != self.show_records {
                self.set_show_records(mode);
            }
            ui.label(
                RichText::new("Drag a stage to move it. Auto layout on Open or Auto layout. Drag empty canvas to pan.")
                    .small()
                    .color(theme::MUTED),
            );
            ui.separator();
            status_badge(ui, self.model.status());
        });
    }

    fn palette_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Palette").strong());
            ui.label(
                RichText::new("drag onto the graph")
                    .color(theme::MUTED)
                    .small(),
            );
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (group, rows) in edit::grouped_palette() {
                    let open = !group.is_user_bucket();
                    egui::CollapsingHeader::new(group.label())
                        .default_open(open)
                        .show(ui, |ui| {
                            if rows.is_empty() {
                                ui.label(RichText::new("None yet.").small().color(theme::MUTED));
                                return;
                            }
                            for e in rows {
                                let resp = ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(edit::palette_verb_label(e.name))
                                                .monospace(),
                                        )
                                        .fill(theme::PANEL_ALT)
                                        .min_size(Vec2::new(ui.available_width(), 22.0))
                                        .sense(Sense::click_and_drag()),
                                    )
                                    .on_hover_text(format!("{}\n{}", e.syntax, e.help));
                                if resp.drag_started() {
                                    self.palette_drag = Some(e.name.to_string());
                                }
                                if resp.clicked() && !resp.dragged() {
                                    self.drop_palette_verb(e.name, DropTarget::Canvas, None);
                                }
                            }
                        });
                }
            });
    }

    fn drop_palette_verb(&mut self, verb: &str, target: DropTarget, pos: Option<(f32, f32)>) {
        self.stop_run_if_editing();
        if let Err(e) = self.model.place_from_palette(verb, target, pos) {
            self.model.push_log(e);
        }
    }

    fn current_drag_verb(&self) -> Option<&str> {
        self.palette_drag
            .as_deref()
            .or(self.forced_drag_verb.as_deref())
    }

    fn update_palette_drag(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect) {
        let Some(verb) = self.current_drag_verb().map(|s| s.to_string()) else {
            self.drop_target = None;
            self.drag_graph_pos = None;
            return;
        };
        let graph_pos = if self.forced_drag_verb.is_some() {
            if let Some(view) = self.model.graph.as_ref() {
                edit::screenshot_drag_pos(view, self.forced_drag_kind, self.model.orthogonal_edges)
            } else {
                (140.0, 90.0)
            }
        } else if let Some(pos) = ui
            .input(|i| i.pointer.latest_pos())
            .filter(|_| resp.contains_pointer() || resp.hovered())
        {
            let gp = screen_to_graph(pos, rect, self.cam);
            (gp.x, gp.y)
        } else {
            self.drop_target = None;
            self.drag_graph_pos = None;
            if ui.input(|i| i.pointer.primary_released()) {
                self.palette_drag = None;
            }
            return;
        };
        self.drag_graph_pos = Some(graph_pos);
        self.drop_target = Some(if let Some(view) = self.model.graph.as_ref() {
            edit::nearest_target(
                view,
                graph_pos.0,
                graph_pos.1,
                &verb,
                self.model.orthogonal_edges,
            )
        } else {
            DropTarget::Canvas
        });
        if self.palette_drag.is_some() && ui.input(|i| i.pointer.primary_released()) {
            let target = self.drop_target.clone().unwrap_or(DropTarget::Canvas);
            let over = resp.contains_pointer()
                || ui
                    .input(|i| i.pointer.latest_pos())
                    .is_some_and(|p| rect.contains(p));
            self.palette_drag = None;
            if over {
                self.drop_palette_verb(&verb, target, Some(graph_pos));
            }
            self.drop_target = None;
            self.drag_graph_pos = None;
        }
    }

    fn resolve_wire_end(&self, spec: &str) -> Option<WireEnd> {
        let (verb, output, stream) = edit::parse_wire_end(spec)?;
        let view = self.model.graph.as_ref()?;
        let key = verb.to_ascii_uppercase();
        let n = view
            .nodes
            .iter()
            .find(|n| n.verb.to_ascii_uppercase() == key)?;
        Some(WireEnd {
            node_index: n.index,
            output,
            stream,
        })
    }

    fn update_wire_drag(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect, view: &GraphView) {
        if let (Some(from_s), Some(to_s)) = (
            self.forced_wire_from.as_deref(),
            self.forced_wire_to.as_deref(),
        ) {
            if let (Some(from), Some(to)) =
                (self.resolve_wire_end(from_s), self.resolve_wire_end(to_s))
            {
                self.wiring = Some(from);
                self.wire_target = Some(to);
                self.wire_pos = Some(edit::screenshot_wire_pos(view, from, Some(to)));
                return;
            }
        }
        let Some(from) = self.wiring else {
            return;
        };
        if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
            let gp = screen_to_graph(pos, rect, self.cam);
            self.wire_pos = Some((gp.x, gp.y));
            self.wire_target = edit::nearest_wire_target(view, gp.x, gp.y, from);
        }
        if ui.input(|i| i.pointer.primary_released()) {
            let target = self.wire_target;
            let from = self.wiring.take();
            self.wire_target = None;
            self.wire_pos = None;
            let Some(from) = from else {
                return;
            };
            self.stop_run_if_editing();
            if let Some(to) = target {
                if let Err(e) = self.model.connect_points(from, to) {
                    self.model.push_log(e);
                }
            } else if resp.contains_pointer() {
                let connected = view.ports.iter().any(|p| {
                    p.node_index == from.node_index
                        && p.output == from.output
                        && p.stream == from.stream
                        && p.connected
                });
                if connected {
                    if let Err(e) =
                        self.model
                            .disconnect_point(from.node_index, from.output, from.stream)
                    {
                        self.model.push_log(e);
                    }
                }
            }
        }
    }

    fn paint_wire_ghost(&self, painter: &egui::Painter, rect: Rect) {
        let Some(from) = self.wiring else {
            return;
        };
        let Some(view) = self.model.graph.as_ref() else {
            return;
        };
        let Some(start) = view.ports.iter().find(|p| {
            p.node_index == from.node_index && p.output == from.output && p.stream == from.stream
        }) else {
            return;
        };
        let end = if let Some(to) = self.wire_target {
            view.ports
                .iter()
                .find(|p| {
                    p.node_index == to.node_index && p.output == to.output && p.stream == to.stream
                })
                .map(|p| p.pos)
                .or(self.wire_pos)
        } else {
            self.wire_pos
        };
        let Some(end) = end else {
            return;
        };
        let to_screen = |x: f32, y: f32| -> Pos2 {
            Pos2::new(
                rect.left() + self.cam.pan.x + x * self.cam.zoom,
                rect.top() + self.cam.pan.y + y * self.cam.zoom,
            )
        };
        painter.line_segment(
            [to_screen(start.pos.0, start.pos.1), to_screen(end.0, end.1)],
            Stroke::new(3.0_f32, theme::SELECT),
        );
    }

    fn paint_palette_ghost(&self, painter: &egui::Painter, rect: Rect) {
        let Some(verb) = self.current_drag_verb() else {
            return;
        };
        let Some((gx, gy)) = self.drag_graph_pos else {
            return;
        };
        let to_screen = |x: f32, y: f32| -> Pos2 {
            Pos2::new(
                rect.left() + self.cam.pan.x + x * self.cam.zoom,
                rect.top() + self.cam.pan.y + y * self.cam.zoom,
            )
        };
        let w = 196.0 * self.cam.zoom;
        let h = 56.0 * self.cam.zoom;
        let r = Rect::from_min_size(to_screen(gx - 98.0, gy - 28.0), Vec2::new(w, h));
        let fill = theme::NODE.linear_multiply(0.72);
        painter.rect(
            r,
            8.0,
            fill,
            Stroke::new(2.0_f32, theme::SELECT),
            egui::StrokeKind::Inside,
        );
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            edit::palette_verb_label(verb),
            FontId::proportional(13.0 * self.cam.zoom.clamp(0.75, 1.25)),
            theme::SELECT,
        );
    }

    fn source_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Source").strong());
            ui.label(RichText::new("authoritative").color(theme::MUTED).small());
        });
        ui.add_space(4.0);
        let span = self.selected_span();
        let underline = self.model.source_underline();
        let err_span = underline.as_ref().map(|(s, _)| *s);
        let field_hints = self.model.source_field_hints();
        let hint_spans: Vec<(usize, usize)> = field_hints.iter().map(|h| h.span).collect();
        let wrap_at = source_wrap_width(ui.available_width());
        let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let w = source_wrap_width(wrap.min(wrap_at));
            highlight_source(ui, buf.as_str(), w, span, err_span, &hint_spans)
        };
        let output = TextEdit::multiline(&mut self.model.document.text)
            .id(egui::Id::new("pipe-source"))
            .code_editor()
            .desired_width(ui.available_width())
            .desired_rows(24)
            .frame(false)
            .layouter(&mut layouter)
            .show(ui);
        if output.response.changed() {
            self.on_source_edited();
            self.source_sync_span = None;
        }
        if let Some((span, msg)) = underline {
            paint_squiggle(
                ui,
                &output.galley,
                output.galley_pos,
                &self.model.document.text,
                span,
                Some(&msg),
                theme::ERROR,
            );
        }
        for h in &field_hints {
            paint_squiggle(
                ui,
                &output.galley,
                output.galley_pos,
                &self.model.document.text,
                h.span,
                None,
                theme::WARN,
            );
        }
        if !field_hints.is_empty() {
            ui.add_space(6.0);
            for h in &field_hints {
                ui.colored_label(theme::WARN, format!("{}: {}", h.spelled, h.message));
            }
        }
        // Linked graph→source selection: jump the cursor so wrapped text stays on-screen.
        // Do not steal the caret while the user is typing.
        if let Some((a, b)) = span {
            if self.source_sync_span != Some((a, b)) && !output.response.has_focus() {
                let text = &self.model.document.text;
                let range = CCursorRange::two(byte_to_ccursor(text, a), byte_to_ccursor(text, b));
                let mut state = output.state;
                state.cursor.set_char_range(Some(range));
                state.store(ui.ctx(), output.response.id);
                output.response.scroll_to_me(Some(Align::Center));
                self.source_sync_span = Some((a, b));
            }
        } else {
            self.source_sync_span = None;
        }
    }

    fn on_source_edited(&mut self) {
        self.stop_run_if_editing();
        self.model.reparse();
    }

    fn stop_run_if_editing(&mut self) {
        if self.model.edit_should_stop_run() {
            self.worker.request_stop();
            if !self.model.edit_stopped_run {
                self.model
                    .push_log("Stopped. The next Run or Step starts from the beginning.");
                self.model.edit_stopped_run = true;
            }
        }
    }

    fn sync_properties_draft(&mut self, operands_focused: bool) {
        let selected = match self.model.selection {
            Selection::Node(i) => self
                .model
                .graph
                .as_ref()
                .and_then(|g| g.nodes.iter().find(|n| n.index == i))
                .map(|n| (n.id, n.index)),
            _ => None,
        };
        let id = selected.map(|(id, _)| id);
        let node_changed = id != self.props_node;
        if node_changed {
            self.props_node = id;
            self.props_streams.clear();
        }
        if let Some((_, index)) = selected {
            if node_changed || !operands_focused {
                let from_source = self.model.operand_text(index);
                if node_changed || from_source != self.props_operands {
                    self.props_operands = from_source;
                }
            }
            if let Some(view) = &self.model.graph {
                for p in view.ports.iter().filter(|p| p.node_index == index) {
                    self.props_streams
                        .entry((p.output, p.stream))
                        .or_insert_with(|| p.word.clone());
                }
            }
        } else {
            self.props_operands.clear();
            self.props_streams.clear();
        }
    }

    fn properties_panel(&mut self, ui: &mut Ui) {
        let operands_focused =
            ui.memory(|m| m.has_focus(egui::Id::new("pipe-properties-operands")));
        self.sync_properties_draft(operands_focused);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Properties").strong());
            ui.label(
                RichText::new("of the selected stage")
                    .color(theme::MUTED)
                    .small(),
            );
        });
        ui.add_space(6.0);
        let Some(index) = (match self.model.selection {
            Selection::Node(i) => Some(i),
            _ => None,
        }) else {
            ui.label(
                RichText::new("Select a stage to see its syntax and help and edit its operands.")
                    .color(theme::MUTED),
            );
            return;
        };
        let (verb, title, script, unknown, node_id) = {
            let Some(n) = self
                .model
                .graph
                .as_ref()
                .and_then(|g| g.nodes.iter().find(|n| n.index == index))
            else {
                ui.label(RichText::new("The selected stage is gone.").color(theme::MUTED));
                return;
            };
            (n.verb.clone(), n.title.clone(), n.script, n.unknown, n.id)
        };
        let registry = StageRegistry::default();
        let catalog = catalog_lookup(&verb, &registry);
        ui.label(RichText::new(&title).strong().size(16.0));
        if let Some(e) = catalog {
            ui.label(RichText::new(e.group.label()).color(theme::MUTED).small());
        } else if script {
            ui.label(
                RichText::new("My REXX and Python stages")
                    .color(theme::MUTED)
                    .small(),
            );
        } else if unknown {
            ui.label(RichText::new("unknown verb").color(theme::ERROR).small());
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Operands").small().color(theme::MUTED));
        let issue = self.model.operand_issue_for(index).cloned();
        let err_span = issue.as_ref().map(|_| (0usize, self.props_operands.len()));
        let field_hints = self
            .model
            .operand_field_hint_spans(index, &self.props_operands);
        let hint_spans: Vec<(usize, usize)> = field_hints.iter().map(|h| h.span).collect();
        let wrap_at = source_wrap_width(ui.available_width());
        let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let w = source_wrap_width(wrap.min(wrap_at));
            highlight_source(ui, buf.as_str(), w, None, err_span, &hint_spans)
        };
        let op_output = TextEdit::multiline(&mut self.props_operands)
            .id(egui::Id::new("pipe-properties-operands"))
            .code_editor()
            .desired_width(ui.available_width())
            .desired_rows(2)
            .layouter(&mut layouter)
            .show(ui);
        if op_output.response.changed() {
            self.stop_run_if_editing();
            if let Err(e) = self.model.patch_operands(index, &self.props_operands) {
                self.model.push_log(e);
            }
        }
        if let Some(issue) = issue {
            let span = (0, self.props_operands.len());
            paint_squiggle(
                ui,
                &op_output.galley,
                op_output.galley_pos,
                &self.props_operands,
                span,
                None,
                theme::ERROR,
            );
            ui.colored_label(theme::ERROR, &issue.message);
        }
        for h in &field_hints {
            paint_squiggle(
                ui,
                &op_output.galley,
                op_output.galley_pos,
                &self.props_operands,
                h.span,
                None,
                theme::WARN,
            );
            ui.colored_label(theme::WARN, format!("{}: {}", h.spelled, h.message));
        }
        ui.add_space(8.0);
        if let Some(e) = catalog {
            ui.label(RichText::new("Syntax").small().color(theme::MUTED));
            ui.label(RichText::new(e.syntax).monospace().weak());
            ui.add_space(4.0);
            ui.label(RichText::new("Help").small().color(theme::MUTED));
            ui.label(e.help);
        } else if script || is_language_stage_verb(&verb) {
            ui.label("A Python or REXX stage. Operands are not checked as you type.");
        } else if unknown {
            ui.label("This verb is not a built-in stage.");
        } else {
            ui.label(RichText::new("No catalog help for this stage yet.").color(theme::MUTED));
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(
            RichText::new("Connection points")
                .small()
                .color(theme::MUTED),
        );
        ui.label(
            RichText::new("Display names for this session. They do not change the pipe text.")
                .small()
                .color(theme::MUTED),
        );
        let ports: Vec<_> = self
            .model
            .graph
            .as_ref()
            .map(|g| {
                g.ports
                    .iter()
                    .filter(|p| p.node_index == index)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if ports.is_empty() {
            ui.label(
                RichText::new("No connection points.")
                    .color(theme::MUTED)
                    .small(),
            );
            return;
        }
        let mut relabel: Option<(bool, u32, String)> = None;
        for p in &ports {
            let side = if p.output { "Output" } else { "Input" };
            let unused = if p.connected { "" } else { " unused" };
            ui.horizontal(|ui| {
                ui.label(format!("{side} {}{unused}", p.stream));
                let entry = self
                    .props_streams
                    .entry((p.output, p.stream))
                    .or_insert_with(|| p.word.clone());
                let resp = ui.add(TextEdit::singleline(entry).desired_width(ui.available_width()));
                if resp.changed() {
                    relabel = Some((p.output, p.stream, entry.clone()));
                }
            });
        }
        if let Some((output, stream, word)) = relabel {
            self.model.set_stream_label(node_id, output, stream, word);
        }
    }

    fn selected_span(&self) -> Option<(usize, usize)> {
        let Selection::Node(i) = self.model.selection else {
            return None;
        };
        let n = self.model.topology.as_ref()?.nodes.get(i)?;
        Some((n.span.start, n.span.end))
    }

    fn graph_panel(&mut self, ui: &mut Ui) {
        Frame::new()
            .fill(theme::BG)
            .inner_margin(Margin::same(0))
            .show(ui, |ui| {
                let (resp, painter) =
                    ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
                let rect = resp.rect;
                painter.rect_filled(rect, 0.0, theme::BG);
                if self.fit_graph {
                    if let Some(view) = &self.model.graph {
                        let zx = (rect.width() - 24.0) / view.width.max(1.0);
                        let zy = (rect.height() - 24.0) / view.height.max(1.0);
                        // Keep stage titles readable; pan for the rest of a long graph.
                        self.cam.zoom = zx.min(zy).clamp(0.40, 1.35);
                        self.cam.pan = Vec2::new(16.0, 20.0);
                    }
                    self.fit_graph = false;
                }
                draw_grid(&painter, rect, self.cam);
                self.update_palette_drag(ui, &resp, rect);

                if self.model.graph.is_some() {
                    let view = self.model.graph.clone().unwrap();
                    let alt = ui.input(|i| i.modifiers.alt);
                    self.apply_forced_hover(&view);
                    let graph_grids = self.show_records.grids_on_graph();
                    self.preview.anchor_rect = screen_anchor(
                        &view,
                        if graph_grids {
                            self.preview.target
                        } else {
                            self.hover
                        },
                        rect,
                        self.cam,
                        self.model.orthogonal_edges,
                    );
                    let over_preview = graph_grids && self.pointer_over_preview(ui.ctx());
                    let retain = graph_grids && self.preview_should_retain(ui.ctx());
                    if resp.hovered() && !over_preview {
                        if let Some(pos) = resp.hover_pos() {
                            let gp = screen_to_graph(pos, rect, self.cam);
                            let next = if let Some(i) = hit_node(&view, gp.x, gp.y) {
                                Selection::Node(i)
                            } else if let Some(i) =
                                hit_edge_route(&view, gp.x, gp.y, self.model.orthogonal_edges)
                            {
                                Selection::Edge(i)
                            } else {
                                Selection::None
                            };
                            if next != self.hover {
                                self.hover = next;
                                self.hover_since = if matches!(next, Selection::None) {
                                    None
                                } else {
                                    Some(Instant::now())
                                };
                            }
                        }
                    } else if self.dragging_node.is_none()
                        && !retain
                        && !self.preview.pinned
                        && !self.forced_hover
                    {
                        self.hover = Selection::None;
                        self.hover_since = None;
                    }
                    // Bind the node on press. drag_started waits for movement, which can
                    // leave the box and miss the grab. Connection points win over the box.
                    if resp.contains_pointer()
                        && ui.input(|i| i.pointer.primary_pressed())
                        && !alt
                        && !over_preview
                        && self.current_drag_verb().is_none()
                    {
                        if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                            let gp = screen_to_graph(pos, rect, self.cam);
                            if let Some(pi) = hit_port(&view, gp.x, gp.y, 16.0) {
                                let p = &view.ports[pi];
                                self.wiring = Some(WireEnd {
                                    node_index: p.node_index,
                                    output: p.output,
                                    stream: p.stream,
                                });
                                self.wire_pos = Some((gp.x, gp.y));
                                self.dragging_node = None;
                                self.panning = false;
                            } else if let Some(i) = hit_node(&view, gp.x, gp.y) {
                                self.dragging_node = Some(i);
                                self.drag_accum = 0.0;
                                self.model.selection = Selection::Node(i);
                                self.panning = false;
                            } else {
                                self.dragging_node = None;
                                self.panning = hit_edge_route(
                                    &view,
                                    gp.x,
                                    gp.y,
                                    self.model.orthogonal_edges,
                                )
                                .is_none();
                            }
                        }
                    }
                    self.update_wire_drag(ui, &resp, rect, &view);
                    if let Some(i) = self.dragging_node {
                        if resp.dragged_by(PointerButton::Primary) {
                            let delta = resp.drag_delta();
                            self.drag_accum += delta.length();
                            if self.drag_accum > 4.0 {
                                let z = self.cam.zoom.max(0.2);
                                self.model.nudge_node(i, delta.x / z, delta.y / z);
                            }
                        }
                    }
                    if resp.drag_stopped() {
                        self.panning = false;
                        self.dragging_node = None;
                        self.drag_accum = 0.0;
                    }
                    let pan = resp.dragged_by(PointerButton::Middle)
                        || (resp.dragged_by(PointerButton::Primary)
                            && self.dragging_node.is_none()
                            && self.wiring.is_none()
                            && self.current_drag_verb().is_none()
                            && (self.panning || alt));
                    if pan {
                        self.cam.pan += resp.drag_delta();
                    }
                    if resp.hovered() && self.dragging_node.is_none() && !over_preview && !retain {
                        let delta = ui.input(|i| i.smooth_scroll_delta.y);
                        if delta.abs() > 0.1 {
                            let factor = if delta > 0.0 { 1.08 } else { 0.92 };
                            self.cam.zoom = (self.cam.zoom * factor).clamp(0.45, 2.8);
                        }
                    }
                    if resp.secondary_clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let gp = screen_to_graph(pos, rect, self.cam);
                            self.context_target = if let Some(i) = hit_node(&view, gp.x, gp.y)
                            {
                                self.model.selection = Selection::Node(i);
                                Selection::Node(i)
                            } else if let Some(i) =
                                hit_edge_route(&view, gp.x, gp.y, self.model.orthogonal_edges)
                            {
                                self.model.selection = Selection::Edge(i);
                                Selection::Edge(i)
                            } else {
                                Selection::None
                            };
                        }
                    }
                    let painted = self.model.graph.clone().unwrap_or(view);
                    let mut badge_hits = Vec::new();
                    self.paint_graph(&painter, rect, &painted, &mut badge_hits);
                    self.over_badge = false;
                    for (i, (hit, tip)) in badge_hits.iter().enumerate() {
                        let resp = ui.interact(*hit, ui.id().with(("node-badge", i)), Sense::hover());
                        if !tip.is_empty() {
                            resp.clone().on_hover_text(tip);
                        }
                        if resp.contains_pointer() {
                            self.over_badge = true;
                        }
                    }
                    if resp.clicked_by(PointerButton::Primary)
                        && self.dragging_node.is_none()
                        && self.drag_accum <= 4.0
                        && !over_preview
                    {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let gp = screen_to_graph(pos, rect, self.cam);
                            if let Some(i) = hit_node(&painted, gp.x, gp.y) {
                                self.model.selection = Selection::Node(i);
                                if graph_grids {
                                    self.pin_preview(Selection::Node(i), pos);
                                }
                            } else if let Some(i) = hit_edge_route(
                                &painted,
                                gp.x,
                                gp.y,
                                self.model.orthogonal_edges,
                            ) {
                                self.model.selection = Selection::Edge(i);
                                if graph_grids {
                                    self.pin_preview(Selection::Edge(i), pos);
                                }
                            } else {
                                self.model.selection = Selection::None;
                                if graph_grids {
                                    self.preview.clear();
                                }
                            }
                        }
                    }
                    let mut ctx_step = false;
                    let mut ctx_view = false;
                    let mut ctx_reset = false;
                    let mut ctx_fit = false;
                    let mut ctx_delete = false;
                    let target = self.context_target;
                    let (ctx_title, ctx_detail, ctx_script, ctx_verb) = context_copy(&self.model, target);
                    resp.context_menu(|ui| {
                        match target {
                            Selection::Node(_) => {
                                ui.label(RichText::new(&ctx_title).strong());
                                if !ctx_detail.is_empty() {
                                    ui.label(RichText::new(&ctx_detail).small().color(theme::MUTED));
                                }
                                ui.label(
                                    RichText::new("Observations only — this menu does not run the stage.")
                                        .small()
                                        .color(theme::MUTED),
                                );
                                if ui.button("Step this stage").clicked() {
                                    ctx_step = true;
                                }
                                if ctx_script && ui.button("View script file").clicked() {
                                    ctx_view = true;
                                }
                                if ui.button("Reset this node position").clicked() {
                                    ctx_reset = true;
                                }
                                if self.model.can_edit_graph()
                                    && ui.button("Delete this stage").clicked()
                                {
                                    ctx_delete = true;
                                }
                            }
                            Selection::Edge(_) => {
                                ui.label(RichText::new(&ctx_title).strong());
                                ui.label(
                                    RichText::new(
                                        "Drop a stage on a connection to splice it in. Drag from one connection point to another to wire a stream.",
                                    )
                                    .small()
                                    .color(theme::MUTED),
                                );
                                if self.model.can_edit_graph()
                                    && ui.button("Disconnect this connection").clicked()
                                {
                                    ctx_delete = true;
                                }
                            }
                            Selection::None => {
                                ui.label("Empty canvas");
                                ui.label(
                                    RichText::new("Drag empty space to pan. Drag a stage to move it. Auto layout on Open or Auto layout.")
                                        .small()
                                        .color(theme::MUTED),
                                );
                                if ui.button("Fit graph").clicked() {
                                    ctx_fit = true;
                                }
                            }
                        }
                    });
                    if ctx_step {
                        self.on_step();
                    }
                    if ctx_view {
                        if let Some(v) = ctx_verb {
                            self.model.open_script_source(&v);
                        }
                    }
                    if ctx_reset {
                        if let Selection::Node(i) = target {
                            self.model.reset_one_node_position(i);
                        }
                    }
                    if ctx_delete {
                        match target {
                            Selection::Node(i) => {
                                self.stop_run_if_editing();
                                if let Err(e) = self.model.delete_stage(i) {
                                    self.model.push_log(e);
                                }
                            }
                            Selection::Edge(i) => {
                                self.stop_run_if_editing();
                                if let Err(e) = self.model.disconnect_edge(i) {
                                    self.model.push_log(e);
                                }
                            }
                            Selection::None => {}
                        }
                    }
                    if ctx_fit {
                        self.fit_graph = true;
                    }
                    if self.model.parse_error.is_some() {
                        painter.text(
                            Pos2::new(rect.left() + 12.0, rect.top() + 8.0),
                            Align2::LEFT_TOP,
                            "Source has a parse error. Showing the last valid graph.",
                            FontId::proportional(13.0),
                            theme::ERROR,
                        );
                    }
                } else if let Some(err) = &self.model.parse_error {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        err,
                        FontId::proportional(15.0),
                        theme::ERROR,
                    );
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Drag a stage from the palette, paste a specification, or open a file.",
                        FontId::proportional(15.0),
                        theme::MUTED,
                    );
                }
                self.paint_palette_ghost(&painter, rect);
                self.paint_wire_ghost(&painter, rect);
            });
    }

    fn paint_graph(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        view: &GraphView,
        badge_hits: &mut Vec<(Rect, String)>,
    ) {
        let to_screen = |x: f32, y: f32| -> Pos2 {
            Pos2::new(
                rect.left() + self.cam.pan.x + x * self.cam.zoom,
                rect.top() + self.cam.pan.y + y * self.cam.zoom,
            )
        };
        let focus_node = match self.model.selection {
            Selection::Node(i) => Some(i),
            _ => match self.hover {
                Selection::Node(i) => Some(i),
                _ => None,
            },
        };
        let focus_edge = match self.model.selection {
            Selection::Edge(i) => Some(i),
            _ => match self.hover {
                Selection::Edge(i) => Some(i),
                _ => None,
            },
        };
        let dragging_palette = self.current_drag_verb().is_some();
        let dimming = !dragging_palette
            && self.wiring.is_none()
            && (focus_node.is_some() || focus_edge.is_some());
        let routes = route_all(view, self.model.orthogonal_edges);
        let draw_edge = |painter: &egui::Painter, ei: usize, on_top: bool| {
            let e = &view.edges[ei];
            let related = edge_is_related(e, focus_node, focus_edge, ei);
            if dimming && related != on_top {
                return;
            }
            if !dimming && on_top {
                return;
            }
            let mut color = if e.writer_stream == 0 {
                theme::ACCENT
            } else {
                theme::SECONDARY
            };
            if dimming && !related {
                color = color.linear_multiply(0.22);
            }
            let selected = matches!(self.model.selection, Selection::Edge(i) if i == ei)
                || matches!(self.hover, Selection::Edge(i) if i == ei);
            let lit_splice = matches!(
                self.drop_target,
                Some(DropTarget::Edge { edge_index }) if edge_index == ei
            );
            if lit_splice {
                color = theme::SELECT;
            }
            let width = if lit_splice {
                5.0_f32
            } else if selected {
                3.6_f32
            } else if related && dimming {
                2.8_f32
            } else {
                1.8_f32
            };
            let pts = &routes[ei];
            for w in pts.windows(2) {
                painter.line_segment(
                    [to_screen(w[0].0, w[0].1), to_screen(w[1].0, w[1].1)],
                    Stroke::new(width, color),
                );
            }
            let a = to_screen(e.from.0, e.from.1);
            let b = to_screen(e.to.0, e.to.1);
            let pr = if related && dimming { 4.2 } else { 3.4 } * self.cam.zoom.clamp(0.7, 1.3);
            painter.circle_filled(a, pr, color);
            painter.circle_filled(b, pr, color);
            let reader_label = view
                .nodes
                .iter()
                .find(|n| n.index == e.reader)
                .and_then(|n| n.label.as_deref());
            let (mx, my) = route_label_anchor(pts);
            let mid = to_screen(mx, my);
            let stream_word = view
                .ports
                .iter()
                .find(|p| p.node_index == e.writer && p.output && p.stream == e.writer_stream)
                .map(|p| p.word.as_str())
                .filter(|s| !s.is_empty());
            if let Some(cap) = stream_caption(e.writer_stream, reader_label, stream_word) {
                painter.text(
                    mid + Vec2::new(0.0, -10.0),
                    egui::Align2::CENTER_CENTER,
                    cap,
                    FontId::proportional(11.0 * self.cam.zoom.clamp(0.75, 1.15)),
                    color,
                );
            }
            let mark = self.model.edge_mark(ei);
            let msize = 11.0 * self.cam.zoom.clamp(0.75, 1.25);
            paint_mark(painter, mid + Vec2::new(0.0, 10.0), msize, mark);
        };
        for ei in 0..view.edges.len() {
            draw_edge(painter, ei, false);
        }
        for n in &view.nodes {
            let r = Rect::from_min_size(
                to_screen(n.rect.x, n.rect.y),
                Vec2::new(n.rect.w * self.cam.zoom, n.rect.h * self.cam.zoom),
            );
            let selected = matches!(self.model.selection, Selection::Node(i) if i == n.index);
            let related = node_is_related(n.index, view, focus_node, focus_edge);
            let fill = if n.unknown {
                theme::NODE_UNKNOWN
            } else if n.script {
                theme::NODE_SCRIPT
            } else {
                theme::NODE
            };
            let fill = if dimming && !related {
                fill.linear_multiply(0.45)
            } else {
                fill
            };
            let noted = !self.model.stage_notes(n.index).is_empty();
            let (stroke_w, stroke_c) = if selected {
                (2.4_f32, theme::SELECT)
            } else if noted {
                (2.4_f32, theme::WARN)
            } else {
                (1.0_f32, theme::BORDER)
            };
            painter.rect(
                r,
                8.0,
                fill,
                Stroke::new(stroke_w, stroke_c),
                egui::StrokeKind::Inside,
            );
            let title_size = (13.0 * self.cam.zoom.clamp(0.75, 1.25)).max(11.0);
            let title_color = if dimming && !related {
                theme::MUTED
            } else {
                theme::TEXT
            };
            let badges = self.model.node_badges(n.index);
            let in_res = badges
                .as_ref()
                .map(|b| in_badge_reserve(b.received, self.cam.zoom))
                .unwrap_or(0.0);
            let out_res = badges
                .as_ref()
                .map(|b| out_badge_reserve(b, self.cam.zoom))
                .unwrap_or(0.0);
            let title_x = 10.0 + in_res;
            let title_max = (r.width() - title_x - out_res - 4.0).max(24.0);
            let title_font = FontId::proportional(title_size);
            let galley = painter.ctx().fonts(|f| {
                let mut job = egui::text::LayoutJob::simple_singleline(
                    n.title.clone(),
                    title_font.clone(),
                    title_color,
                );
                job.wrap.max_width = title_max;
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                job.wrap.overflow_character = Some('…');
                f.layout_job(job)
            });
            painter.galley(r.left_top() + Vec2::new(title_x, 8.0), galley, title_color);
            let sub = match n.kind {
                NodeKind::LabelDefinition => "label (COPY at run)".to_string(),
                NodeKind::Connector => "CALLPIPE connector".to_string(),
                _ if n.unknown => "unknown verb".to_string(),
                _ => n.detail.clone(),
            };
            if !sub.is_empty() {
                let sub_font =
                    FontId::proportional((11.0 * self.cam.zoom.clamp(0.75, 1.1)).max(10.0));
                let sub_max = (r.width() - 20.0).max(24.0);
                let fitted = fit_stage_label(painter.ctx(), &sub_font, &sub, sub_max, n.keep_end);
                let sub_color = if n.unknown {
                    theme::ERROR
                } else {
                    theme::MUTED
                };
                let clip = painter.with_clip_rect(r);
                let galley = clip
                    .ctx()
                    .fonts(|f| f.layout_no_wrap(fitted, sub_font, sub_color));
                clip.galley(
                    r.left_top() + Vec2::new(10.0, 30.0 * self.cam.zoom.max(0.85)),
                    galley,
                    sub_color,
                );
            }
            if let Some(b) = badges {
                paint_node_badges(painter, r, b, self.cam.zoom, badge_hits);
            }
        }
        let mut extra_lit = Vec::new();
        if let Some(w) = self.wiring {
            extra_lit.push((w.node_index, w.output, w.stream));
        }
        if let Some(w) = self.wire_target {
            extra_lit.push((w.node_index, w.output, w.stream));
        }
        paint_connection_points(
            painter,
            view,
            to_screen,
            self.cam.zoom,
            focus_node,
            focus_edge,
            dimming,
            self.drop_target.as_ref(),
            &extra_lit,
        );
        if dimming {
            for ei in 0..view.edges.len() {
                draw_edge(painter, ei, true);
            }
        }
    }

    fn inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Records").strong());
            if let Some(snap) = &self.model.session {
                ui.label(
                    RichText::new(crate::inspector::live_status_line(snap)).color(theme::MUTED),
                );
            } else {
                ui.label(RichText::new("No live session").color(theme::MUTED));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Hide").clicked() {
                    self.model.inspector_open = false;
                }
            });
        });
        egui::ScrollArea::both()
            .id_salt("records-pane-scroll")
            .auto_shrink([false, false])
            .scroll_source(crate::sheet::scroll_source())
            .show(ui, |ui| {
                let mut details = self.model.inspector_details;
                match self.model.selection {
                    Selection::None => {
                        crate::inspector::show_run_overview(ui, &self.model.session_overview());
                    }
                    Selection::Node(i) => {
                        let (name, unknown, script, view_verb) = {
                            let placed = self
                                .model
                                .graph
                                .as_ref()
                                .and_then(|g| g.nodes.iter().find(|p| p.index == i));
                            let name = placed
                                .map(|p| p.hover_name.clone())
                                .or_else(|| {
                                    self.model
                                        .topology
                                        .as_ref()
                                        .and_then(|t| t.nodes.get(i).map(|n| n.verb.clone()))
                                })
                                .unwrap_or_else(|| "stage".into());
                            let unknown = placed.is_some_and(|p| p.unknown);
                            let script = placed.filter(|p| p.script).map(|p| ScriptIdentity {
                                verb: p.verb.clone(),
                                running: p.running.clone(),
                                cwd: self.model.cwd.clone(),
                            });
                            let view_verb = script.as_ref().map(|s| s.verb.clone());
                            (name, unknown, script, view_verb)
                        };
                        if unknown {
                            ui.colored_label(
                                theme::ERROR,
                                "Unknown verb: not a built-in stage and not a .py/.rexx script.",
                            );
                        }
                        if view_verb.is_some() && ui.button("View script file").clicked() {
                            if let Some(v) = &view_verb {
                                self.model.open_script_source(v);
                            }
                        }
                        let notes = self.model.stage_notes(i);
                        let operand_hints = self.model.operand_hints_for_node(i);
                        let focus = self.model.inspect_focus();
                        let labels = self.model.inspect_labels();
                        crate::inspector::show_compact(
                            ui,
                            self.model.session.as_ref(),
                            focus,
                            &labels,
                            &name,
                            script,
                            &notes,
                            &operand_hints,
                            &mut details,
                        );
                    }
                    Selection::Edge(i) => {
                        let labels = self.model.inspect_labels();
                        let title = if labels.title.is_empty() {
                            edge_inspect_title(&self.model, i)
                        } else {
                            labels.title.clone()
                        };
                        let focus = self.model.inspect_focus();
                        crate::inspector::show_compact(
                            ui,
                            self.model.session.as_ref(),
                            focus,
                            &labels,
                            &title,
                            None,
                            &[],
                            &[],
                            &mut details,
                        );
                    }
                }
                self.model.inspector_details = details;
                ui.add_space(8.0);
                if let Some(err) = &self.model.parse_error {
                    ui.colored_label(theme::ERROR, err);
                }
                for w in &self.model.warnings {
                    if crate::warnings::is_blocking_note(w) {
                        ui.colored_label(theme::ERROR, w);
                    }
                }
                crate::inspector::show_about(ui, &self.model.about);
            });
    }

    fn console_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Console and messages").strong());
            ui.label(
                RichText::new("CONSOLE output is captured here. CONSOLE stdin is refused.")
                    .small()
                    .color(theme::MUTED),
            );
        });
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.scope(|ui| {
                    ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                    if self.model.console.is_empty() && self.model.log.is_empty() {
                        ui.label(RichText::new("No output yet.").color(theme::MUTED));
                    }
                    for line in &self.model.log {
                        ui.label(RichText::new(line).color(theme::MUTED));
                    }
                    for line in &self.model.console {
                        ui.label(RichText::new(line).color(theme::TEXT));
                    }
                });
            });
    }

    fn pointer_pos(ctx: &egui::Context) -> Option<Pos2> {
        ctx.input(|i| {
            i.pointer
                .interact_pos()
                .or(i.pointer.hover_pos())
                .or(i.pointer.latest_pos())
        })
    }

    fn set_show_records(&mut self, mode: ShowRecords) {
        if self.show_records == mode {
            return;
        }
        self.show_records = mode;
        if mode.pane_shows_grids() {
            self.model.inspector_open = true;
            self.preview.clear();
        }
        let _ = crate::settings::save_persisted(mode);
    }

    fn apply_forced_hover(&mut self, view: &GraphView) {
        let Some(verb) = self.hover_verb.as_deref() else {
            return;
        };
        let Some(i) = node_index_for_verb(view, verb) else {
            return;
        };
        self.hover = Selection::Node(i);
        self.hover_since = Some(Instant::now() - Duration::from_secs(1));
        self.forced_hover = true;
        if self.show_records.grids_on_graph() {
            self.preview.target = Selection::Node(i);
            self.preview.hover_since = Some(Instant::now() - Duration::from_secs(1));
            self.preview.last_inside = Some(Instant::now());
        }
    }

    fn pin_preview(&mut self, target: Selection, _pos: Pos2) {
        self.preview.target = target;
        self.preview.pinned = true;
        self.preview.just_pinned = true;
        self.preview.hover_since = Some(Instant::now());
        self.preview.last_inside = Some(Instant::now());
        self.hover = target;
    }

    fn pointer_over_preview(&self, ctx: &egui::Context) -> bool {
        let pos = Self::pointer_pos(ctx);
        pointer_in_rect(pos, self.preview.area_rect, 2.0) || self.preview.drag_in_popup
    }

    fn pointer_over_anchor(&self, ctx: &egui::Context) -> bool {
        pointer_in_rect(Self::pointer_pos(ctx), self.preview.anchor_rect, 2.0)
            || self.hover == self.preview.target && !matches!(self.hover, Selection::None)
    }

    fn pointer_over_bridge(&self, ctx: &egui::Context) -> bool {
        let (Some(pos), Some(anchor), Some(popup)) = (
            Self::pointer_pos(ctx),
            self.preview.anchor_rect,
            self.preview.area_rect,
        ) else {
            return false;
        };
        hover_bridge(anchor, popup).contains(pos)
    }

    fn preview_should_retain(&self, ctx: &egui::Context) -> bool {
        if matches!(self.preview.target, Selection::None) {
            return false;
        }
        let (keep, _) = keep_open(RetainInput {
            over_anchor: self.pointer_over_anchor(ctx),
            over_popup: self.pointer_over_preview(ctx),
            over_bridge: self.pointer_over_bridge(ctx),
            pointer_down_in_popup: self.preview.drag_in_popup,
            last_inside: self.preview.last_inside,
            now: Instant::now(),
            grace: GRACE,
            pinned: self.preview.pinned,
        });
        keep
    }

    fn maybe_graph_tooltip(&mut self, ctx: &egui::Context) {
        if self.show_records.grids_on_graph() {
            self.maybe_graph_card(ctx);
        } else {
            if !matches!(self.preview.target, Selection::None) {
                self.preview.clear();
            }
            self.maybe_peek_tooltip(ctx);
        }
    }

    fn maybe_peek_tooltip(&mut self, ctx: &egui::Context) {
        if self.over_badge || self.dragging_node.is_some() {
            return;
        }
        if matches!(self.hover, Selection::None) {
            return;
        }
        let ready = self.forced_hover
            || self
                .hover_since
                .map(|t| t.elapsed() >= PEEK_DELAY)
                .unwrap_or(false);
        if !ready {
            ctx.request_repaint_after(Duration::from_millis(40));
            return;
        }
        let Some(peek) = self.hover_peek_for(self.hover) else {
            return;
        };
        let forced_pos = self.forced_hover.then(|| {
            self.preview
                .anchor_rect
                .map(|r| r.right_top() + Vec2::new(8.0, 4.0))
        });
        let Some(mut pos) = forced_pos
            .flatten()
            .or_else(|| Self::pointer_pos(ctx).map(|p| p + Vec2::new(16.0, 18.0)))
            .or_else(|| {
                self.preview
                    .anchor_rect
                    .map(|r| r.right_top() + Vec2::new(8.0, 4.0))
            })
        else {
            return;
        };
        let screen = ctx.screen_rect();
        pos.x = pos.x.min(screen.right() - 200.0).max(screen.left() + 8.0);
        pos.y = pos.y.min(screen.bottom() - 80.0).max(screen.top() + 8.0);
        egui::Area::new(egui::Id::new("graph-peek"))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .constrain(false)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    crate::inspector::show_hover_peek(ui, &peek);
                });
            });
    }

    fn maybe_graph_card(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.preview.clear();
            return;
        }
        if egui::Popup::is_any_open(ctx) && !self.preview.pinned && !self.preview_should_retain(ctx)
        {
            self.preview.clear();
            return;
        }
        if self.dragging_node.is_some() && !self.preview.pinned {
            self.preview.clear();
            return;
        }

        if self.forced_hover && !matches!(self.hover, Selection::None) {
            self.preview.target = self.hover;
            self.show_preview_area(ctx);
            return;
        }

        let pos = Self::pointer_pos(ctx);
        let over_popup = self.pointer_over_preview(ctx);
        let over_anchor = self.pointer_over_anchor(ctx);
        let over_bridge = self.pointer_over_bridge(ctx);
        let any_down = ctx.input(|i| i.pointer.any_down());
        if any_down && over_popup {
            self.preview.drag_in_popup = true;
        }
        if !any_down {
            self.preview.drag_in_popup = over_popup;
        }

        let (keep, refresh) = keep_open(RetainInput {
            over_anchor,
            over_popup,
            over_bridge,
            pointer_down_in_popup: self.preview.drag_in_popup,
            last_inside: self.preview.last_inside,
            now: Instant::now(),
            grace: GRACE,
            pinned: self.preview.pinned,
        });
        if refresh {
            self.preview.last_inside = Some(Instant::now());
        }

        if ctx.input(|i| i.pointer.primary_clicked()) && !self.preview.just_pinned {
            if let Some(p) = pos {
                let on_popup = pointer_in_rect(Some(p), self.preview.area_rect, 2.0);
                let on_anchor = pointer_in_rect(Some(p), self.preview.anchor_rect, 2.0);
                if on_popup && !matches!(self.preview.target, Selection::None) {
                    self.preview.pinned = true;
                } else if !on_popup
                    && !on_anchor
                    && (self.preview.pinned || self.preview.area_rect.is_some())
                {
                    self.preview.clear();
                    return;
                }
            }
        }
        self.preview.just_pinned = false;

        if self.preview.pinned {
            if matches!(self.preview.target, Selection::None) {
                self.preview.clear();
                return;
            }
            self.show_preview_area(ctx);
            return;
        }
        if keep
            && !matches!(self.preview.target, Selection::None)
            && self.preview.area_rect.is_some()
        {
            self.show_preview_area(ctx);
            return;
        }
        if self.over_badge && self.preview.area_rect.is_none() {
            return;
        }
        match self.hover {
            Selection::None => {
                if !keep {
                    self.preview.clear();
                } else if !matches!(self.preview.target, Selection::None) {
                    self.show_preview_area(ctx);
                }
            }
            target => {
                if self.preview.target != target {
                    self.preview.target = target;
                    self.preview.hover_since = Some(Instant::now());
                    self.preview.last_inside = Some(Instant::now());
                    self.preview.area_rect = None;
                    self.preview.last_size = None;
                    self.preview.pinned = false;
                    ctx.request_repaint_after(Duration::from_millis(40));
                    return;
                }
                let ready = self
                    .preview
                    .hover_since
                    .map(|t| t.elapsed() >= OPEN_DELAY)
                    .unwrap_or(false);
                if !ready {
                    ctx.request_repaint_after(Duration::from_millis(40));
                    return;
                }
                self.show_preview_area(ctx);
            }
        }
    }

    fn show_preview_area(&mut self, ctx: &egui::Context) {
        let target = self.preview.target;
        if matches!(target, Selection::None) {
            return;
        }
        let (title, detail) = match target {
            Selection::Node(i) => {
                let n = self
                    .model
                    .graph
                    .as_ref()
                    .and_then(|g| g.nodes.iter().find(|n| n.index == i));
                let title = n
                    .map(|n| n.hover_name.clone())
                    .unwrap_or_else(|| "stage".into());
                let detail = n.map(|n| n.detail.clone()).unwrap_or_default();
                (title, detail)
            }
            Selection::Edge(i) => (edge_inspect_title(&self.model, i), String::new()),
            Selection::None => return,
        };
        let focus = self.model.focus_for(target);
        let labels = self.model.labels_for(target).unwrap_or_default();
        let title = if !labels.title.is_empty() {
            labels.title.clone()
        } else {
            title
        };
        let screen = ctx.screen_rect();
        let card_max = crate::hover::hover_card_max(screen);
        let size = self.preview.last_size.unwrap_or(PREVIEW_MAX);
        let pos = if let Some(anchor) = self.preview.anchor_rect {
            place_adjacent(anchor, size, screen)
        } else {
            let mut pos = self.preview.pos;
            if pos.x + size.x > screen.right() - 8.0 {
                pos.x = (screen.right() - size.x - 8.0).max(screen.left() + 8.0);
            }
            if pos.y + size.y > screen.bottom() - 8.0 {
                pos.y = (screen.bottom() - size.y - 8.0).max(screen.top() + 8.0);
            }
            pos
        };
        self.preview.pos = pos;
        let mut close = false;
        let inner = egui::Area::new(egui::Id::new("graph-hover"))
            .order(egui::Order::Foreground)
            .interactable(true)
            .constrain(false)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.set_max_width(card_max.x);
                ui.set_max_height(card_max.y);
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(card_max.x - 12.0);
                    ui.set_max_height(card_max.y - 12.0);
                    close = crate::inspector::show_hover_card(
                        ui,
                        self.model.session.as_ref(),
                        focus,
                        &labels,
                        &title,
                        &detail,
                        self.preview.pinned,
                    );
                });
            });
        if close {
            self.preview.clear();
            return;
        }
        self.preview.area_rect = Some(inner.response.rect);
        self.preview.last_size = Some(inner.response.rect.size());
        if inner.response.clicked() {
            self.preview.pinned = true;
        }
        if inner.response.contains_pointer() {
            self.preview.last_inside = Some(Instant::now());
        }
    }

    fn hover_peek_for(&self, target: Selection) -> Option<HoverPeek> {
        let view = self.model.graph.as_ref()?;
        match target {
            Selection::None => None,
            Selection::Node(i) => {
                let n = view.nodes.iter().find(|n| n.index == i)?;
                let (ended, waiting_in, holding, received, sent, discarded) =
                    match self.model.node_badges(i) {
                        Some(b) => (
                            b.ended,
                            b.waiting_in,
                            b.holding.is_some(),
                            b.received,
                            b.sent,
                            b.discarded,
                        ),
                        None => (false, false, false, 0, 0, 0),
                    };
                let mut peek = stage_hover_peek(
                    &n.hover_name,
                    &n.full_text,
                    ended,
                    waiting_in,
                    holding,
                    received,
                    sent,
                    discarded,
                );
                peek.warning = self.model.stage_notes(i).into_iter().next();
                Some(peek)
            }
            Selection::Edge(i) => {
                let e = view.edges.get(i)?;
                let writer = view
                    .nodes
                    .iter()
                    .find(|n| n.index == e.writer)
                    .map(|n| n.title.as_str())
                    .unwrap_or("stage");
                let reader = view
                    .nodes
                    .iter()
                    .find(|n| n.index == e.reader)
                    .map(|n| n.title.as_str())
                    .unwrap_or("stage");
                let sent = self
                    .model
                    .session
                    .as_ref()
                    .zip(self.model.focus_for(target))
                    .map(|(snap, focus)| edge_sent_count(snap, focus))
                    .unwrap_or(0);
                Some(edge_hover_peek(writer, reader, e.writer_stream, sent))
            }
        }
    }

    fn script_source_modal(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let Some(view) = self.model.script_source.clone() else {
            return;
        };
        egui::Modal::new(egui::Id::new("script-source")).show(ctx, |ui| {
            ui.label(
                RichText::new(format!("{} — {}", view.language, view.path.display()))
                    .strong()
                    .size(16.0),
            );
            ui.label(
                RichText::new(format!(
                    "Read from disk. Working directory: {}",
                    self.model.cwd.display()
                ))
                .small()
                .color(theme::MUTED),
            );
            ui.label(
                RichText::new("Not interpreter locals, and not a live step inside the script.")
                    .small()
                    .color(theme::MUTED),
            );
            ui.add_space(8.0);
            match &view.body {
                Ok(body) => {
                    egui::ScrollArea::both()
                        .max_height(420.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.label(RichText::new(body).monospace());
                        });
                }
                Err(e) => {
                    ui.colored_label(theme::ERROR, e);
                }
            }
            ui.add_space(8.0);
            if ui.button("Close").clicked() {
                close = true;
            }
        });
        if close {
            self.model.script_source = None;
        }
    }

    fn open_picker(&mut self, kind: PickerKind) {
        let start = self
            .model
            .document
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| default_browse_dir(&self.model.cwd));
        match kind {
            PickerKind::Open => {
                if let Some(p) = native_open(&start) {
                    self.open_path(&p);
                    return;
                }
            }
            PickerKind::SaveAs => {
                let name = self
                    .model
                    .document
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "pipeline.pipe".into());
                if let Some(p) = native_save(&start, &name) {
                    let p = ensure_pipe_extension(p);
                    if let Err(e) = self.model.document.save_to(&p) {
                        self.model.push_log(e.to_string());
                    }
                    return;
                }
            }
            PickerKind::Cwd => {}
        }
        self.model.picker.open = true;
        self.model.picker.kind = kind;
        self.model.picker.dir = start;
        self.model.picker.name.clear();
    }

    fn open_path(&mut self, path: &Path) {
        match self.model.load_path(path) {
            Ok(()) => self.fit_graph = true,
            Err(e) => self.model.push_log(e),
        }
    }

    fn save(&mut self) {
        if self.model.document.path.is_some() {
            if let Err(e) = self.model.document.save() {
                self.model.push_log(e.to_string());
            }
        } else {
            self.open_picker(PickerKind::SaveAs);
        }
    }

    fn picker_modal(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut chosen: Option<PathBuf> = None;
        let mut go_parent = false;
        let mut go_into: Option<PathBuf> = None;
        egui::Modal::new(egui::Id::new("file-picker")).show(ctx, |ui| {
            let title = match self.model.picker.kind {
                PickerKind::Open => "Open pipeline",
                PickerKind::SaveAs => "Save as",
                PickerKind::Cwd => "Working directory",
            };
            ui.label(RichText::new(title).strong().size(16.0));
            ui.monospace(self.model.picker.dir.display().to_string());
            ui.horizontal(|ui| {
                if ui.button("Parent").clicked() {
                    go_parent = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
            ui.add_space(6.0);
            let listing = list_dir(&self.model.picker.dir);
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| match listing {
                    Ok(rows) => {
                        for row in rows {
                            let label = if row.is_dir {
                                format!("📁 {}", row.name)
                            } else {
                                format!(" {}", row.name)
                            };
                            if ui.selectable_label(false, label).double_clicked() {
                                if row.is_dir {
                                    go_into = Some(row.path);
                                } else {
                                    chosen = Some(row.path);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        ui.colored_label(theme::ERROR, e);
                    }
                });
            if self.model.picker.kind == PickerKind::SaveAs {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.model.picker.name);
                    if ui.button("Save").clicked() {
                        let mut p = self.model.picker.dir.clone();
                        p.push(&self.model.picker.name);
                        chosen = Some(p);
                    }
                });
            }
            if self.model.picker.kind == PickerKind::Cwd {
                if ui.button("Use this folder").clicked() {
                    chosen = Some(self.model.picker.dir.clone());
                }
            }
        });
        if go_parent {
            if let Some(p) = self.model.picker.dir.parent() {
                self.model.picker.dir = p.to_path_buf();
            }
        }
        if let Some(p) = go_into {
            self.model.picker.dir = p;
        }
        if let Some(p) = chosen {
            match self.model.picker.kind {
                PickerKind::Open => self.open_path(&p),
                PickerKind::SaveAs => {
                    match self.model.document.save_to(&ensure_pipe_extension(p)) {
                        Ok(()) => {
                            if self.model.close_after_save {
                                self.model.confirm_saved();
                            }
                        }
                        Err(e) => {
                            self.model.push_log(e.to_string());
                            self.model.close_after_save = false;
                        }
                    }
                }
                PickerKind::Cwd => {
                    self.model.cwd = p;
                    self.model.push_log(
                        "Working directory applies on the next Run. It is process-wide while a session is live; the editor uses absolute paths.",
                    );
                }
            }
            close = true;
        }
        if close {
            self.model.picker.open = false;
            if !self.model.close_confirmed {
                self.model.close_after_save = false;
            }
        }
    }

    fn unsaved_modal(&mut self, ctx: &egui::Context, _quit: &mut bool) {
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.label(RichText::new("Unsaved changes").strong().size(16.0));
            ui.label("Save the Source text before closing?");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Discard").clicked() {
                    discard = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if save {
            self.model.pending_close = false;
            self.model.close_after_save = true;
            self.save();
            self.model.confirm_saved();
        }
        if discard {
            self.model.confirm_discard();
        }
        if cancel {
            self.model.cancel_close();
        }
    }
}

fn status_badge(ui: &mut Ui, status: SessionStatus) {
    let (label, color) = match status {
        SessionStatus::Idle => ("Idle", theme::MUTED),
        SessionStatus::Running => ("Running", theme::ACCENT),
        SessionStatus::Paused => ("Paused", theme::SELECT),
        SessionStatus::Ended { rc } => (if rc == 0 { "Ended" } else { "Ended (rc)" }, theme::OK),
        SessionStatus::Stopped { .. } => ("Stopped", theme::WARN),
        SessionStatus::Failed(_) => ("Failed", theme::ERROR),
    };
    ui.label(RichText::new(label).color(color).strong());
    if let SessionStatus::Ended { rc } | SessionStatus::Stopped { rc } = status {
        ui.label(format!("rc {rc}"));
    }
    if let SessionStatus::Failed(m) = status {
        ui.colored_label(theme::ERROR, m);
    }
}

fn paint_connection_points(
    painter: &egui::Painter,
    view: &GraphView,
    to_screen: impl Fn(f32, f32) -> Pos2,
    zoom: f32,
    focus_node: Option<usize>,
    focus_edge: Option<usize>,
    dimming: bool,
    drop: Option<&DropTarget>,
    extra_lit: &[(usize, bool, u32)],
) {
    let font = FontId::proportional((10.0 * zoom).clamp(7.0, 12.0));
    for p in &view.ports {
        let related = node_is_related(p.node_index, view, focus_node, focus_edge);
        let mut color = if p.stream == 0 {
            theme::ACCENT
        } else {
            theme::SECONDARY
        };
        if dimming && !related {
            color = color.linear_multiply(0.28);
        }
        let pos = to_screen(p.pos.0, p.pos.1);
        let lit = matches!(
            drop,
            Some(DropTarget::Port {
                node_index,
                output,
                stream,
            }) if *node_index == p.node_index && *output == p.output && *stream == p.stream
        ) || extra_lit
            .iter()
            .any(|(n, o, s)| *n == p.node_index && *o == p.output && *s == p.stream);
        if lit {
            color = theme::SELECT;
        }
        let pr = if lit {
            7.0 * zoom.clamp(0.7, 1.3)
        } else {
            3.6 * zoom.clamp(0.7, 1.3)
        };
        if lit {
            painter.circle_filled(pos, pr + 4.0, color.linear_multiply(0.35));
            painter.circle_stroke(pos, pr, Stroke::new(2.4_f32, color));
        } else if p.connected {
            painter.circle_stroke(
                pos,
                pr + 1.2,
                Stroke::new(1.0_f32, color.linear_multiply(0.7)),
            );
        } else {
            painter.circle_stroke(pos, pr, Stroke::new(1.6_f32, color));
        }
        // Connected streams are named on the connection, not again on the port.
        if p.connected {
            continue;
        }
        let show_label = !p.word.is_empty() || p.stream != 0;
        if !show_label {
            continue;
        }
        let text = if p.word.is_empty() {
            format!("{}", p.stream)
        } else {
            p.word.clone()
        };
        let vertical = view.direction.is_vertical();
        let (align, offset) = if vertical {
            if p.output {
                (Align2::CENTER_TOP, Vec2::new(0.0, 8.0 * zoom))
            } else {
                (Align2::CENTER_BOTTOM, Vec2::new(0.0, -8.0 * zoom))
            }
        } else if p.output {
            (Align2::LEFT_CENTER, Vec2::new(8.0 * zoom, 0.0))
        } else {
            (Align2::RIGHT_CENTER, Vec2::new(-8.0 * zoom, 0.0))
        };
        painter.text(pos + offset, align, text, font.clone(), color);
    }
}

fn highlight_source(
    ui: &egui::Ui,
    text: &str,
    wrap: f32,
    span: Option<(usize, usize)>,
    err_span: Option<(usize, usize)>,
    hint_spans: &[(usize, usize)],
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let base = egui::TextFormat {
        font_id: FontId::monospace(13.0),
        color: theme::TEXT,
        ..Default::default()
    };
    let mut cuts = vec![0, text.len()];
    if let Some((a, b)) = span {
        cuts.push(floor_char(text, a));
        cuts.push(floor_char(text, b.max(a)));
    }
    if let Some((a, b)) = err_span {
        cuts.push(floor_char(text, a));
        cuts.push(floor_char(text, b.max(a)));
    }
    for (a, b) in hint_spans {
        cuts.push(floor_char(text, *a));
        cuts.push(floor_char(text, (*b).max(*a)));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let sel = span.map(|(a, b)| (floor_char(text, a), floor_char(text, b.max(a))));
    let err = err_span.map(|(a, b)| (floor_char(text, a), floor_char(text, b.max(a))));
    let hints: Vec<(usize, usize)> = hint_spans
        .iter()
        .map(|(a, b)| (floor_char(text, *a), floor_char(text, (*b).max(*a))))
        .collect();
    for w in cuts.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        if lo >= hi || hi > text.len() {
            continue;
        }
        let mid = (lo + hi) / 2;
        let mut fmt = base.clone();
        if let Some((a, b)) = sel {
            if mid >= a && mid < b {
                fmt.background = theme::SELECT_BG;
                fmt.color = theme::SELECT;
            }
        }
        if let Some((a, b)) = err {
            if mid >= a && mid < b {
                fmt.color = theme::ERROR;
                fmt.underline = Stroke::new(1.0_f32, theme::ERROR);
            }
        } else if hints.iter().any(|(a, b)| mid >= *a && mid < *b) {
            fmt.color = theme::WARN;
            fmt.underline = Stroke::new(1.0_f32, theme::WARN);
        }
        job.append(&text[lo..hi], 0.0, fmt);
    }
    if job.sections.is_empty() {
        job.append(text, 0.0, base);
    }
    job.wrap.max_width = source_wrap_width(wrap);
    ui.fonts(|f| f.layout_job(job))
}

fn paint_squiggle(
    ui: &Ui,
    galley: &egui::Galley,
    galley_pos: Pos2,
    text: &str,
    span: (usize, usize),
    message: Option<&str>,
    color: Color32,
) {
    let mut a = byte_to_ccursor(text, span.0).index;
    let mut b = byte_to_ccursor(text, span.1).index;
    let n = text.chars().count();
    if b <= a {
        b = (a + 1).min(n);
    }
    if a >= n && n > 0 {
        a = n - 1;
        b = n;
    }
    let painter = ui.painter();
    let mut points: Vec<Pos2> = Vec::new();
    let mut last_y: Option<f32> = None;
    let flush = |painter: &egui::Painter, pts: &mut Vec<Pos2>| {
        paint_wave(painter, pts, color);
        pts.clear();
    };
    for i in a..b {
        let r = galley.pos_from_cursor(egui::text::CCursor::new(i));
        let p = galley_pos + Vec2::new(r.left(), r.bottom());
        if last_y.map(|y| (y - p.y).abs() > 3.0).unwrap_or(false) {
            flush(&painter, &mut points);
        }
        last_y = Some(p.y);
        points.push(p);
    }
    if b > a {
        let last = b.saturating_sub(1);
        let r = galley.pos_from_cursor(egui::text::CCursor::new(last));
        points.push(galley_pos + Vec2::new(r.right().max(r.left() + 6.0), r.bottom()));
    }
    let msg_at = points.last().copied().unwrap_or(galley_pos);
    paint_wave(&painter, &points, color);
    if let Some(message) = message {
        painter.text(
            Pos2::new(msg_at.x + 8.0, msg_at.y - 10.0),
            Align2::LEFT_CENTER,
            message,
            FontId::proportional(12.0),
            color,
        );
    }
}

fn paint_wave(painter: &egui::Painter, points: &[Pos2], color: Color32) {
    if points.is_empty() {
        return;
    }
    let stroke = Stroke::new(1.25_f32, color);
    if points.len() == 1 {
        let p = points[0];
        painter.line_segment([p, Pos2::new(p.x + 8.0, p.y)], stroke);
        return;
    }
    let amp = 2.4;
    let period = 5.0;
    let mut prev: Option<Pos2> = None;
    let mut dist = 0.0;
    for w in points.windows(2) {
        let a = w[0];
        let b = w[1];
        let delta = b - a;
        let len = delta.length().max(0.5);
        let steps = ((len / 2.0).ceil() as i32).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let p = a + delta * t;
            let yoff = ((dist + t * len) / period * std::f32::consts::TAU).sin() * amp;
            let q = Pos2::new(p.x, p.y + yoff);
            if let Some(prev) = prev {
                painter.line_segment([prev, q], stroke);
            }
            prev = Some(q);
        }
        dist += len;
    }
}

pub(crate) fn source_wrap_width(available: f32) -> f32 {
    if available.is_finite() && available > 16.0 {
        available
    } else {
        320.0
    }
}

fn byte_to_ccursor(text: &str, byte: usize) -> CCursor {
    let b = floor_char(text, byte.min(text.len()));
    CCursor::new(text[..b].chars().count())
}

fn badge_zoom(zoom: f32) -> f32 {
    zoom.clamp(0.75, 1.3)
}

fn count_badge_width(count: usize, zoom: f32) -> f32 {
    let z = badge_zoom(zoom);
    let icon = 11.0 * z;
    let digits = count.to_string().len() as f32;
    icon + 4.0 * z + 6.0 * digits * z
}

fn in_badge_reserve(received: usize, zoom: f32) -> f32 {
    if received == 0 {
        0.0
    } else {
        count_badge_width(received, zoom) + 6.0 * badge_zoom(zoom)
    }
}

fn out_badge_reserve(b: &StageBadges, zoom: f32) -> f32 {
    let z = badge_zoom(zoom);
    let icon = 11.0 * z;
    let mut w = 8.0 * z;
    let show_run = b.ended || !b.waiting_in;
    if show_run {
        w += icon + 6.0 * z;
    }
    if let Some(n) = b.holding {
        w += icon + 6.0 * z + 6.0 * n.to_string().len() as f32 * z;
    }
    if b.sent > 0 {
        w += icon + 6.0 * z + 6.0 * b.sent.to_string().len() as f32 * z;
    }
    if b.discarded > 0 {
        w += icon + 6.0 * z + 6.0 * b.discarded.to_string().len() as f32 * z;
    }
    w
}

fn paint_count_badge(
    painter: &egui::Painter,
    left: f32,
    y: f32,
    mark: RecordMark,
    text: &str,
    zoom: f32,
) -> Rect {
    let z = badge_zoom(zoom);
    let icon = 11.0 * z;
    let font = FontId::proportional((10.0 * z).max(9.0));
    let mark_c = Pos2::new(left + icon * 0.5, y);
    paint_mark(painter, mark_c, icon, mark);
    let text_x = left + icon;
    painter.text(
        Pos2::new(text_x, y),
        egui::Align2::LEFT_CENTER,
        text,
        font,
        crate::symbols::mark_color(mark),
    );
    let w = icon + 4.0 * z + 6.0 * text.len() as f32 * z;
    Rect::from_min_size(Pos2::new(left, y - icon * 0.55), Vec2::new(w, icon * 1.1))
}

fn paint_node_badges(
    painter: &egui::Painter,
    r: Rect,
    b: StageBadges,
    zoom: f32,
    hits: &mut Vec<(Rect, String)>,
) {
    let run = crate::symbols::stage_run_mark(b.ended, b.waiting_in);
    let show_run = b.ended || !b.waiting_in;
    let z = badge_zoom(zoom);
    let icon = 11.0 * z;
    let font = FontId::proportional((10.0 * z).max(9.0));
    let y = r.top() + 10.0;
    if b.received > 0 {
        let hit = paint_count_badge(
            painter,
            r.left() + 6.0,
            y,
            RecordMark::Received,
            &in_badge_label(b.received),
            zoom,
        );
        hits.push((hit, b.in_tip.clone()));
    }
    if b.sent == 0 && b.discarded == 0 && b.holding.is_none() && !show_run {
        return;
    }
    let mut x = r.right() - 6.0;
    let mut parts: Vec<(RecordMark, String, String, bool)> = Vec::new();
    if show_run {
        parts.push((run, String::new(), String::new(), false));
    }
    if let Some(n) = b.holding {
        parts.push((RecordMark::Holding, format!("{n}"), String::new(), false));
    }
    if b.sent > 0 {
        parts.push((
            RecordMark::Sent,
            out_badge_label(b.sent),
            b.out_tip.clone(),
            true,
        ));
    }
    if b.discarded > 0 {
        parts.push((
            RecordMark::Discarded,
            format!("{}", b.discarded),
            String::new(),
            false,
        ));
    }
    for (mark, text, tip, number_then_arrow) in parts.iter().rev() {
        let tw = 6.0 * text.len() as f32 * z;
        let hit = if *number_then_arrow {
            x -= icon * 0.55;
            paint_mark(painter, Pos2::new(x, y), icon, *mark);
            x -= tw;
            painter.text(
                Pos2::new(x, y),
                egui::Align2::LEFT_CENTER,
                text,
                font.clone(),
                crate::symbols::mark_color(*mark),
            );
            Rect::from_min_size(
                Pos2::new(x - 2.0 * z, y - icon * 0.55),
                Vec2::new(tw + icon + 4.0 * z, icon * 1.1),
            )
        } else {
            x -= tw;
            painter.text(
                Pos2::new(x, y),
                egui::Align2::LEFT_CENTER,
                text,
                font.clone(),
                crate::symbols::mark_color(*mark),
            );
            x -= icon * 0.55;
            paint_mark(painter, Pos2::new(x, y), icon, *mark);
            Rect::from_min_size(
                Pos2::new(x - icon * 0.5, y - icon * 0.55),
                Vec2::new(icon + tw + 4.0 * z, icon * 1.1),
            )
        };
        if !tip.is_empty() {
            hits.push((hit, tip.clone()));
        }
        x -= icon * 0.55 + 6.0 * z;
    }
}

fn edge_inspect_title(model: &GuiModel, edge_index: usize) -> String {
    model
        .labels_for(crate::model::Selection::Edge(edge_index))
        .map(|l| l.title)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "connection".into())
}

fn context_copy(model: &GuiModel, target: Selection) -> (String, String, bool, Option<String>) {
    match target {
        Selection::Node(i) => {
            if let Some(n) = model
                .graph
                .as_ref()
                .and_then(|g| g.nodes.iter().find(|n| n.index == i))
            {
                (
                    n.hover_name.clone(),
                    n.detail.clone(),
                    n.script,
                    Some(n.verb.clone()),
                )
            } else {
                ("stage".into(), String::new(), false, None)
            }
        }
        Selection::Edge(i) => {
            let title = model
                .graph
                .as_ref()
                .and_then(|g| g.edges.get(i))
                .map(|e| {
                    stream_caption(
                        e.writer_stream,
                        model
                            .graph
                            .as_ref()
                            .and_then(|g| g.nodes.iter().find(|n| n.index == e.reader))
                            .and_then(|n| n.label.as_deref()),
                        model.graph.as_ref().and_then(|g| {
                            g.ports
                                .iter()
                                .find(|p| {
                                    p.node_index == e.writer
                                        && p.output
                                        && p.stream == e.writer_stream
                                })
                                .map(|p| p.word.as_str())
                                .filter(|s| !s.is_empty())
                        }),
                    )
                    .unwrap_or_else(|| "primary connection".into())
                })
                .unwrap_or_else(|| "connection".into());
            (title, String::new(), false, None)
        }
        Selection::None => ("canvas".into(), String::new(), false, None),
    }
}

fn floor_char(s: &str, mut i: usize) -> usize {
    if i > s.len() {
        i = s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn screen_to_graph(pos: Pos2, rect: Rect, cam: Camera) -> egui::Pos2 {
    egui::Pos2::new(
        (pos.x - rect.left() - cam.pan.x) / cam.zoom,
        (pos.y - rect.top() - cam.pan.y) / cam.zoom,
    )
}

fn graph_to_screen(pos: egui::Pos2, rect: Rect, cam: Camera) -> Pos2 {
    Pos2::new(
        rect.left() + cam.pan.x + pos.x * cam.zoom,
        rect.top() + cam.pan.y + pos.y * cam.zoom,
    )
}

fn screen_anchor(
    view: &GraphView,
    target: Selection,
    rect: Rect,
    cam: Camera,
    orthogonal: bool,
) -> Option<Rect> {
    match target {
        Selection::Node(i) => view.nodes.iter().find(|n| n.index == i).map(|n| {
            Rect::from_min_size(
                graph_to_screen(egui::Pos2::new(n.rect.x, n.rect.y), rect, cam),
                Vec2::new(n.rect.w * cam.zoom, n.rect.h * cam.zoom),
            )
        }),
        Selection::Edge(i) => {
            let routes = route_all(view, orthogonal);
            routes.get(i).map(|pts| {
                let (x, y) = route_label_anchor(pts);
                let p = graph_to_screen(egui::Pos2::new(x, y), rect, cam);
                Rect::from_center_size(p, Vec2::new(28.0, 18.0))
            })
        }
        Selection::None => None,
    }
}

fn node_index_for_verb(view: &GraphView, verb: &str) -> Option<usize> {
    let key = verb.to_ascii_uppercase();
    let exact = view
        .nodes
        .iter()
        .find(|n| n.verb.eq_ignore_ascii_case(verb))
        .map(|n| n.index);
    let named = view.nodes.iter().find(|n| {
        n.verb.to_ascii_uppercase().contains(&key)
            || n.hover_name.to_ascii_uppercase().contains(&key)
            || n.label
                .as_deref()
                .is_some_and(|l| l.eq_ignore_ascii_case(verb))
    });
    exact.or_else(|| named.map(|n| n.index))
}

fn draw_grid(painter: &egui::Painter, rect: Rect, cam: Camera) {
    let step = 24.0 * cam.zoom;
    if step < 8.0 {
        return;
    }
    let mut x = rect.left() + (cam.pan.x % step);
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0_f32, Color32::from_rgb(24, 30, 38)),
        );
        x += step;
    }
    let mut y = rect.top() + (cam.pan.y % step);
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0_f32, Color32::from_rgb(24, 30, 38)),
        );
        y += step;
    }
}

/// Shrink `text` until it fits `max_width` pixels. File stages keep the
/// trailing path; other stages keep the start and end with "…".
fn fit_stage_label(
    ctx: &egui::Context,
    font: &FontId,
    text: &str,
    max_width: f32,
    keep_end: bool,
) -> String {
    if text.is_empty() {
        return String::new();
    }
    let width_of = |s: &str| {
        ctx.fonts(|f| {
            f.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        })
    };
    if width_of(text) <= max_width {
        return text.to_string();
    }
    let n = text.chars().count().max(1);
    let mut lo = 1usize;
    let mut hi = n;
    let mut best = LABEL_ELLIPSIS.to_string();
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let candidate = ellipsize_label(text, mid, keep_end);
        if width_of(&candidate) <= max_width {
            best = candidate;
            lo = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            hi = mid - 1;
        }
    }
    best
}

fn write_screenshot(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(file, image.width() as u32, image.height() as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
    for p in &image.pixels {
        bytes.extend_from_slice(&p.to_array());
    }
    w.write_image_data(&bytes).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod source_layout_tests {
    use super::source_wrap_width;

    #[test]
    fn wrap_width_is_never_infinite() {
        assert!(source_wrap_width(380.0).is_finite());
        assert!(source_wrap_width(f32::INFINITY).is_finite());
        assert!(source_wrap_width(f32::NAN).is_finite());
        assert!(source_wrap_width(0.0) > 16.0);
        assert!((source_wrap_width(240.0) - 240.0).abs() < f32::EPSILON);
    }
}
