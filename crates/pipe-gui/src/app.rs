use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Color32, FontId, Frame, Key, Layout, Margin, PointerButton, Pos2, Rect, RichText,
    Sense, Stroke, TextEdit, Ui, Vec2,
};
use pipe_core::{SessionStatus, StageId};
use pipe_stages::NodeKind;

use crate::files::{default_browse_dir, list_dir, native_open, native_save};
use crate::graph::{hit_edge, hit_node, GraphView};
use crate::model::{GuiModel, PickerKind, Selection};
use crate::theme;
use crate::worker::{SessionWorker, WorkerCmd, WorkerEvent};

pub struct GuiApp {
    pub model: GuiModel,
    worker: SessionWorker,
    cam: Camera,
    screenshot: Option<PathBuf>,
    screenshot_frames: u32,
    screenshot_done: bool,
    frames: u32,
    fit_graph: bool,
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
        Self {
            model,
            worker: SessionWorker::start(),
            cam: Camera::default(),
            screenshot: cfg.screenshot,
            screenshot_frames: cfg.screenshot_frames.max(2),
            screenshot_done: false,
            frames: 0,
            fit_graph: true,
        }
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
        self.prepare_session();
        self.worker.send(WorkerCmd::Run);
        self.model
            .push_log("Run requested. Side effects happen now if the spec writes files.");
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
        if self.model.session.is_none() || self.model.source_changed_since_session() {
            self.prepare_session();
        }
        if let Some(id) = self.model.selected_runtime_stage() {
            self.worker.send(WorkerCmd::Step { stage: id });
        } else if let Some(id) = first_runtime_id(self.model.session.as_ref()) {
            self.worker.send(WorkerCmd::Step { stage: id });
            self.model
                .push_log("Step used the first live stage. Select a graph node to choose.");
        } else {
            self.model
                .push_log("Step needs a session. Preparing, then step the first stage.");
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

        if quit {
            if self.model.dirty() && !self.model.pending_close {
                self.model.pending_close = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.worker.send(WorkerCmd::Shutdown);
        self.worker.request_stop();
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
        let resp = ui.add_sized(ui.available_size(), edit);
        if resp.changed() {
            self.model.reparse();
            self.fit_graph = true;
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
                        self.cam.zoom = zx.min(zy).clamp(0.38, 1.15);
                        self.cam.pan = Vec2::new(16.0, 20.0);
                    }
                    self.fit_graph = false;
                }
                draw_grid(&painter, rect, self.cam);

                if let Some(view) = self.model.graph.clone() {
                    if resp.dragged_by(PointerButton::Middle)
                        || (resp.dragged_by(PointerButton::Primary) && ui.input(|i| i.modifiers.alt))
                    {
                        self.cam.pan += resp.drag_delta();
                    }
                    if resp.hovered() {
                        let delta = ui.input(|i| i.smooth_scroll_delta.y);
                        if delta.abs() > 0.1 {
                            let factor = if delta > 0.0 { 1.08 } else { 0.92 };
                            self.cam.zoom = (self.cam.zoom * factor).clamp(0.35, 2.8);
                        }
                    }
                    if resp.dragged_by(PointerButton::Secondary) {
                        self.cam.pan += resp.drag_delta();
                    }
                    self.paint_graph(&painter, rect, &view);
                    if resp.clicked_by(PointerButton::Primary) {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let gp = screen_to_graph(pos, rect, self.cam);
                            if let Some(i) = hit_node(&view, gp.x, gp.y) {
                                self.model.selection = Selection::Node(i);
                            } else if let Some(i) = hit_edge(&view, gp.x, gp.y) {
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
            let a = to_screen(e.from.0, e.from.1);
            let b = to_screen(e.to.0, e.to.1);
            let color = if e.writer_stream == 0 {
                theme::ACCENT
            } else {
                theme::SECONDARY
            };
            let selected = matches!(self.model.selection, Selection::Edge(i) if view.edges.get(i).is_some_and(|x| x.writer == e.writer && x.reader == e.reader && x.writer_stream == e.writer_stream));
            let width = if selected { 3.5_f32 } else { 1.8_f32 };
            let mid = Pos2::new((a.x + b.x) * 0.5, a.y);
            painter.line_segment([a, mid], Stroke::new(width, color));
            painter.line_segment([mid, b], Stroke::new(width, color));
            if e.writer_stream > 0 {
                painter.text(
                    Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5 - 10.0),
                    egui::Align2::CENTER_CENTER,
                    if e.writer_stream == 1 {
                        "secondary"
                    } else {
                        "stream"
                    },
                    FontId::proportional(10.0),
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
            painter.text(
                r.left_top() + Vec2::new(12.0, 10.0),
                egui::Align2::LEFT_TOP,
                &n.title,
                FontId::proportional(13.0 * self.cam.zoom.clamp(0.7, 1.2)),
                theme::TEXT,
            );
            let sub = match n.kind {
                NodeKind::LabelDefinition => "label definition (COPY at run)",
                NodeKind::Connector => "CALLPIPE connector",
                _ if n.unknown => "unknown verb",
                _ => n.detail.as_str(),
            };
            painter.text(
                r.left_top() + Vec2::new(12.0, 30.0 * self.cam.zoom.max(0.8)),
                egui::Align2::LEFT_TOP,
                sub,
                FontId::proportional(11.0),
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
        ui.add_space(6.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            match self.model.selection {
                Selection::None => {
                    ui.label(RichText::new("Select a stage or connection.").color(theme::MUTED));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Record stepping").strong());
                    ui.label(
                        RichText::new("A step is one observable I/O cycle on the selected stage, not a proven 1-in-1-out mapping. FANIN, COLLATE, REXX, Python, and COMMAND may read several streams. CALLPIPE is BlockedOnCallPipe. RUNPIPE nested text is data.")
                            .small()
                            .color(theme::MUTED),
                    );
                }
                Selection::Node(i) => {
                    if let Some(topo) = &self.model.topology {
                        if let Some(n) = topo.nodes.get(i) {
                            ui.label(RichText::new(&n.verb).strong().color(theme::ACCENT));
                            if let Some(l) = &n.label {
                                ui.label(format!("label {l}  occurrence {}", n.label_occurrence));
                            }
                            ui.label(format!("pipeline {}  stage {}", n.pipeline, n.stage));
                            ui.label(format!("kind {:?}", n.kind));
                            ui.monospace(&n.running);
                        }
                    }
                    if let Some(snap) = &self.model.session {
                        if let Some(id) = self.model.selected_runtime_stage() {
                            if let Some(live) = snap.live.iter().find(|s| s.id == id) {
                                ui.separator();
                                ui.label(format!(
                                    "live {}  ended={}  rc={}  callpipe={}  helper={}",
                                    live.name, live.ended, live.return_code, live.waiting_callpipe, live.waiting_helper
                                ));
                            }
                            ui.separator();
                            ui.label(RichText::new("Records on connected slots").strong());
                            for slot in snap.slots.iter().filter(|s| s.writer == id || s.reader == id) {
                                record_slot(ui, slot);
                            }
                        }
                        if let Some(note) = &snap.last_step {
                            ui.separator();
                            ui.label(RichText::new("Last step").strong());
                            ui.label(note);
                        }
                    }
                }
                Selection::Edge(i) => {
                    if let Some(view) = &self.model.graph {
                        if let Some(e) = view.edges.get(i) {
                            ui.label(format!(
                                "connection  writer node {} stream {} → reader node {} stream {}",
                                e.writer, e.writer_stream, e.reader, e.reader_stream
                            ));
                            let label = if e.writer_stream == 0 {
                                "primary"
                            } else if e.writer_stream == 1 {
                                "secondary"
                            } else {
                                "numbered stream"
                            };
                            ui.label(RichText::new(label).color(theme::SECONDARY));
                            if let Some(snap) = &self.model.session {
                                for slot in &snap.slots {
                                    if slot.writer.0 == e.writer && slot.writer_stream.0 == e.writer_stream {
                                        record_slot(ui, slot);
                                    }
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
                PickerKind::SaveAs => {
                    if let Err(e) = self.model.document.save_to(&p) {
                        self.model.push_log(e.to_string());
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
        }
    }

    fn unsaved_modal(&mut self, ctx: &egui::Context, quit: &mut bool) {
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.label(RichText::new("Unsaved changes").strong().size(16.0));
            ui.label("Save this buffer before closing?");
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    self.save();
                    if !self.model.dirty() {
                        self.model.pending_close = false;
                        *quit = true;
                    }
                }
                if ui.button("Discard").clicked() {
                    self.model.pending_close = false;
                    *quit = true;
                }
                if ui.button("Cancel").clicked() {
                    self.model.pending_close = false;
                }
            });
        });
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

fn record_slot(ui: &mut Ui, slot: &pipe_core::SlotSnapshot) {
    ui.add_space(4.0);
    ui.label(format!(
        "slot writer {}/{} → reader {}/{}  severed={} writer_done={}",
        slot.writer.0,
        slot.writer_stream.0,
        slot.reader.0,
        slot.reader_stream.0,
        slot.severed,
        slot.writer_done
    ));
    if let Some(rec) = &slot.record {
        ui.label(format!("length {} bytes  peeked={}", rec.len, slot.peeked));
        match std::str::from_utf8(&rec.bytes) {
            Ok(s) => {
                ui.monospace(s);
            }
            Err(_) => {
                ui.monospace(hex_preview(&rec.bytes));
            }
        }
    } else {
        ui.label(RichText::new("empty").color(theme::MUTED).italics());
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(64)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
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
    job.wrap.max_width = wrap;
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
