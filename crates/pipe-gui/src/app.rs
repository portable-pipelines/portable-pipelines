use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, FontId, Frame, Key, Layout, Margin, PointerButton, Pos2, Rect, RichText,
    Sense, Stroke, TextEdit, Ui, Vec2,
};
use pipe_core::{SessionStatus, StageId};
use pipe_stages::NodeKind;

use crate::files::{default_browse_dir, list_dir, native_open, native_save};
use crate::graph::{edge_points, hit_edge_route, hit_node, stream_caption, GraphView};
use crate::model::{GuiModel, PickerKind, Selection};
use crate::report_error;
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
    quit_after_ms: Option<u64>,
    started: Instant,
    shutting_down: bool,
    shutdown_at: Option<Instant>,
    panning: bool,
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

pub struct LaunchCfg {
    pub file: Option<PathBuf>,
    pub spec: Option<String>,
    pub cwd: PathBuf,
    pub screenshot: Option<PathBuf>,
    pub screenshot_frames: u32,
    pub auto_run: bool,
    pub quit_after_ms: Option<u64>,
    pub show_unsaved: bool,
    pub select_verb: Option<String>,
    pub orthogonal: Option<bool>,
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
            model.push_log("Open a .pipe file or paste a specification. Run is never automatic.");
        }
        if let Some(ortho) = cfg.orthogonal {
            model.orthogonal_edges = ortho;
        }
        if let Some(verb) = &cfg.select_verb {
            if let Some(topo) = &model.topology {
                let key = verb.to_ascii_uppercase();
                if let Some(i) = topo.nodes.iter().position(|n| {
                    n.verb.eq_ignore_ascii_case(verb)
                        || n.verb.to_ascii_uppercase().contains(&key)
                        || n.running.to_ascii_uppercase().contains(&key)
                }) {
                    model.selection = Selection::Node(i);
                }
            }
        }
        if cfg.show_unsaved {
            model.document.text.push_str("\n");
            model.pending_close = true;
        }
        Self {
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
            quit_after_ms: cfg.quit_after_ms,
            started: Instant::now(),
            shutting_down: false,
            shutdown_at: None,
            panning: false,
        }
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
        self.model.session_spec = Some(self.model.document.text.clone());
        self.worker.send(WorkerCmd::Prepare {
            spec: self.model.document.text.clone(),
            cwd: self.model.cwd.clone(),
        });
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
        let restart = self.model.session.is_none()
            || self.model.source_changed_since_session()
            || self.model.status().is_terminal();
        if restart {
            self.prepare_session();
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
        let restart = self.model.session.is_none() || self.model.source_changed_since_session();
        if restart {
            self.prepare_session();
        }
        let id = self
            .model
            .selected_runtime_stage()
            .or_else(|| first_runtime_id(self.model.session.as_ref()))
            .or_else(|| self.model.first_topology_stage_id());
        if let Some(id) = id {
            self.worker.send(WorkerCmd::Step { stage: id });
            if restart {
                self.model.push_log(
                    "Starting a session and stepping now (this click is the first step, not a wasted prepare).",
                );
            }
        } else {
            self.model
                .push_log("Select a graph node to step, or paste a specification.");
        }
    }
}

fn first_runtime_id(snap: Option<&crate::worker::Snapshot>) -> Option<StageId> {
    snap.and_then(|s| s.live.first().map(|l| l.id))
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::apply(ctx);
        self.drain_worker();
        self.frames = self.frames.saturating_add(1);
        ctx.request_repaint_after(std::time::Duration::from_millis(50));

        let mut quit = false;
        egui::TopBottomPanel::top("chrome")
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(10)))
            .show(ctx, |ui| {
                self.chrome(ui, &mut quit);
            });

        egui::TopBottomPanel::bottom("console")
            .resizable(true)
            .default_height(160.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                self.console_panel(ui);
            });

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

        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(320.0)
            .min_width(240.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                self.inspector(ui);
            });

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(0)))
            .show(ctx, |ui| {
                self.graph_panel(ui);
            });

        if self.model.picker.open {
            self.picker_modal(ctx);
        }

        if self.model.pending_close {
            self.unsaved_modal(ctx, &mut quit);
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
                    "pipe-gui: debug session did not stop in time after Quit; exiting so pipe --gui can return",
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
            "pipe-gui: debug session thread did not stop after Quit. An in-flight host file write or helper may still be running. Exiting so pipe --gui is not stuck. Already-written files stay written; a write in flight may be truncated.",
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
    }

    fn chrome(&mut self, ui: &mut Ui, quit: &mut bool) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Portable Pipelines")
                    .font(FontId::proportional(18.0))
                    .color(theme::TEXT)
                    .strong(),
            );
            ui.label(
                RichText::new("native debugger")
                    .color(theme::MUTED)
                    .italics(),
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
                .unwrap_or_else(|| "untitled buffer".into());
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
            let run = ui.add(
                egui::Button::new(RichText::new("  Run  ").strong().color(Color32::WHITE))
                    .fill(theme::ACCENT_DIM),
            );
            if run.clicked() {
                self.on_run();
            }
            if ui.button("Pause").clicked() {
                self.on_pause();
            }
            if ui.button("Step one record").clicked() {
                self.on_step();
            }
            if ui.button("Stop").clicked() {
                self.on_stop();
            }
            if ui.button("Fit graph").clicked() {
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
            ui.separator();
            status_badge(ui, self.model.status());
            if self.model.source_changed_since_session() {
                ui.label(
                    RichText::new("Source changed. Stop and Run to apply. The live session still uses the previous spec.")
                        .color(theme::WARN),
                );
            }
        });
    }

    fn source_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Source").strong());
            ui.label(RichText::new("authoritative").color(theme::MUTED).small());
        });
        ui.add_space(4.0);
        let span = self.selected_span();
        let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            highlight_source(ui, buf.as_str(), wrap, span)
        };
        let edit = TextEdit::multiline(&mut self.model.document.text)
            .code_editor()
            .desired_width(f32::INFINITY)
            .desired_rows(28)
            .frame(false)
            .layouter(&mut layouter);
        let resp = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| ui.add_sized(ui.available_size(), edit))
            .inner;
        if resp.changed() {
            self.model.reparse();
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
                        self.cam.zoom = zx.min(zy).clamp(0.55, 1.35);
                        self.cam.pan = Vec2::new(16.0, 20.0);
                    }
                    self.fit_graph = false;
                }
                draw_grid(&painter, rect, self.cam);

                if let Some(view) = self.model.graph.clone() {
                    if resp.drag_started_by(PointerButton::Primary)
                        && !ui.input(|i| i.modifiers.alt)
                    {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let gp = screen_to_graph(pos, rect, self.cam);
                            self.panning = hit_node(&view, gp.x, gp.y).is_none()
                                && hit_edge_route(
                                    &view,
                                    gp.x,
                                    gp.y,
                                    self.model.orthogonal_edges,
                                )
                                .is_none();
                        }
                    }
                    if resp.drag_stopped() {
                        self.panning = false;
                    }
                    let pan = resp.dragged_by(PointerButton::Middle)
                        || resp.dragged_by(PointerButton::Secondary)
                        || (resp.dragged_by(PointerButton::Primary)
                            && (self.panning || ui.input(|i| i.modifiers.alt)));
                    if pan {
                        self.cam.pan += resp.drag_delta();
                    }
                    if resp.hovered() {
                        let delta = ui.input(|i| i.smooth_scroll_delta.y);
                        if delta.abs() > 0.1 {
                            let factor = if delta > 0.0 { 1.08 } else { 0.92 };
                            self.cam.zoom = (self.cam.zoom * factor).clamp(0.45, 2.8);
                        }
                    }
                    self.paint_graph(&painter, rect, &view);
                    if resp.clicked_by(PointerButton::Primary) && !self.panning {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let gp = screen_to_graph(pos, rect, self.cam);
                            if let Some(i) = hit_node(&view, gp.x, gp.y) {
                                self.model.selection = Selection::Node(i);
                            } else if let Some(i) =
                                hit_edge_route(&view, gp.x, gp.y, self.model.orthogonal_edges)
                            {
                                self.model.selection = Selection::Edge(i);
                            } else {
                                self.model.selection = Selection::None;
                            }
                        }
                    }
                } else if let Some(err) = &self.model.parse_error {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        err,
                        FontId::proportional(15.0),
                        theme::ERROR,
                    );
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Paste a specification or open a file. The graph is drawn from source without running it.",
                        FontId::proportional(15.0),
                        theme::MUTED,
                    );
                }
            });
    }

    fn paint_graph(&self, painter: &egui::Painter, rect: Rect, view: &GraphView) {
        let to_screen = |x: f32, y: f32| -> Pos2 {
            Pos2::new(
                rect.left() + self.cam.pan.x + x * self.cam.zoom,
                rect.top() + self.cam.pan.y + y * self.cam.zoom,
            )
        };
        for e in &view.edges {
            let pts = edge_points(e.from, e.to, self.model.orthogonal_edges);
            let color = if e.writer_stream == 0 {
                theme::ACCENT
            } else {
                theme::SECONDARY
            };
            let selected = matches!(self.model.selection, Selection::Edge(i) if view.edges.get(i).is_some_and(|x| x.writer == e.writer && x.reader == e.reader && x.writer_stream == e.writer_stream));
            let width = if selected { 3.5_f32 } else { 1.8_f32 };
            for w in pts.windows(2) {
                painter.line_segment(
                    [to_screen(w[0].0, w[0].1), to_screen(w[1].0, w[1].1)],
                    Stroke::new(width, color),
                );
            }
            let a = to_screen(e.from.0, e.from.1);
            let b = to_screen(e.to.0, e.to.1);
            painter.circle_filled(a, 3.4 * self.cam.zoom.clamp(0.7, 1.3), color);
            painter.circle_filled(b, 3.4 * self.cam.zoom.clamp(0.7, 1.3), color);
            let reader_label = view
                .nodes
                .iter()
                .find(|n| n.index == e.reader)
                .and_then(|n| n.label.as_deref());
            if let Some(cap) = stream_caption(e.writer_stream, reader_label) {
                painter.text(
                    Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5 - 10.0),
                    egui::Align2::CENTER_CENTER,
                    cap,
                    FontId::proportional(11.0 * self.cam.zoom.clamp(0.75, 1.15)),
                    color,
                );
            }
        }
        for n in &view.nodes {
            let r = Rect::from_min_size(
                to_screen(n.rect.x, n.rect.y),
                Vec2::new(n.rect.w * self.cam.zoom, n.rect.h * self.cam.zoom),
            );
            let selected = matches!(self.model.selection, Selection::Node(i) if i == n.index);
            let fill = if n.unknown {
                theme::NODE_UNKNOWN
            } else if n.script {
                theme::NODE_SCRIPT
            } else {
                theme::NODE
            };
            painter.rect(
                r,
                8.0,
                fill,
                Stroke::new(
                    if selected { 2.4_f32 } else { 1.0_f32 },
                    if selected {
                        theme::SELECT
                    } else {
                        theme::BORDER
                    },
                ),
                egui::StrokeKind::Inside,
            );
            let title_size = (14.0 * self.cam.zoom.clamp(0.75, 1.25)).max(11.0);
            painter.text(
                r.left_top() + Vec2::new(12.0, 10.0),
                egui::Align2::LEFT_TOP,
                &n.title,
                FontId::proportional(title_size),
                theme::TEXT,
            );
            let sub = match n.kind {
                NodeKind::LabelDefinition => "label (COPY at run)",
                NodeKind::Connector => "CALLPIPE connector",
                _ if n.unknown => "unknown verb — not a registered stage or script",
                _ if n.script => "external script",
                _ => n.detail.as_str(),
            };
            painter.text(
                r.left_top() + Vec2::new(12.0, 32.0 * self.cam.zoom.max(0.85)),
                egui::Align2::LEFT_TOP,
                sub,
                FontId::proportional((11.5 * self.cam.zoom.clamp(0.75, 1.1)).max(10.0)),
                if n.unknown {
                    theme::ERROR
                } else {
                    theme::MUTED
                },
            );
        }
    }

    fn inspector(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Inspector").strong());
        if let Some(snap) = &self.model.session {
            ui.label(RichText::new(crate::inspector::live_status_line(snap)).color(theme::MUTED));
        } else {
            ui.label(RichText::new("No live session").color(theme::MUTED));
        }
        ui.add_space(6.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            match self.model.selection {
                Selection::None => {
                    ui.label(RichText::new("Select a stage or connection.").color(theme::MUTED));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Record stepping").strong());
                    ui.label(
                        RichText::new("A step is one observable I/O cycle on the selected stage, not a proven 1-in-1-out mapping. FANIN, COLLATE, REXX, Python, and COMMAND may read several streams. CALLPIPE is not one record. RUNPIPE nested text is data.")
                            .small()
                            .color(theme::MUTED),
                    );
                }
                Selection::Node(i) => {
                    if let Some(topo) = &self.model.topology {
                        if let Some(n) = topo.nodes.get(i) {
                            ui.label(RichText::new(&n.verb).strong().color(theme::ACCENT));
                            if let Some(l) = &n.label {
                                ui.label(format!("label {l}"));
                            }
                            ui.monospace(&n.running);
                            let flags = self
                                .model
                                .graph
                                .as_ref()
                                .and_then(|g| g.nodes.iter().find(|p| p.index == i));
                            if flags.is_some_and(|p| p.unknown) {
                                ui.colored_label(
                                    theme::ERROR,
                                    "Unknown verb: not a built-in stage and not a .py/.rexx script.",
                                );
                            } else if flags.is_some_and(|p| p.script) {
                                ui.label(
                                    RichText::new("External script stage (not an unknown builtin).")
                                        .color(theme::ACCENT),
                                );
                            }
                        }
                    }
                    if let Some(snap) = &self.model.session {
                        if let Some(id) = self.model.selected_runtime_stage() {
                            let live = snap.live.iter().find(|s| s.id == id);
                            crate::inspector::show_stage(
                                ui,
                                snap,
                                id,
                                live,
                                snap.last_step.as_deref(),
                            );
                        }
                    }
                }
                Selection::Edge(i) => {
                    if let Some(view) = &self.model.graph {
                        if let Some(e) = view.edges.get(i) {
                            let cap = stream_caption(
                                e.writer_stream,
                                view.nodes
                                    .iter()
                                    .find(|n| n.index == e.reader)
                                    .and_then(|n| n.label.as_deref()),
                            );
                            ui.label(RichText::new(cap.unwrap_or_else(|| "primary".into()))
                                .color(theme::SECONDARY));
                            ui.label("Records move from the writer port to the reader port.");
                            if let (Some(snap), Some(topo)) =
                                (&self.model.session, &self.model.topology)
                            {
                                if let Some(id) = crate::model::topology_stage_id(topo, e.writer)
                                {
                                    crate::inspector::show_stage(
                                        ui,
                                        snap,
                                        id,
                                        snap.live.iter().find(|s| s.id == id),
                                        None,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Notes").strong());
            if let Some(err) = &self.model.parse_error {
                ui.colored_label(theme::ERROR, err);
            }
            for w in &self.model.warnings {
                ui.label(RichText::new(w).small().color(theme::WARN));
            }
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
                PickerKind::SaveAs => match self.model.document.save_to(&p) {
                    Ok(()) => {
                        if self.model.close_after_save {
                            self.model.confirm_saved();
                        }
                    }
                    Err(e) => {
                        self.model.push_log(e.to_string());
                        self.model.close_after_save = false;
                    }
                },
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
            ui.label("Save this buffer before closing?");
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

fn highlight_source(
    ui: &egui::Ui,
    text: &str,
    wrap: f32,
    span: Option<(usize, usize)>,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    let base = egui::TextFormat {
        font_id: FontId::monospace(13.0),
        color: theme::TEXT,
        ..Default::default()
    };
    if let Some((a, b)) = span {
        let a = floor_char(text, a);
        let b = floor_char(text, b.max(a));
        if a > 0 {
            job.append(&text[..a], 0.0, base.clone());
        }
        let mut hi = base.clone();
        hi.background = theme::SELECT_BG;
        hi.color = theme::SELECT;
        if b > a && b <= text.len() {
            job.append(&text[a..b], 0.0, hi);
            job.append(&text[b..], 0.0, base);
        } else {
            job.append(&text[a..], 0.0, base);
        }
    } else {
        job.append(text, 0.0, base);
    }
    let _ = wrap;
    job.wrap.max_width = f32::INFINITY;
    ui.fonts(|f| f.layout_job(job))
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
