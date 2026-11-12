//! Graph layout from `inspect_topology`. Dragged positions are session-only.

use std::collections::HashMap;
use std::path::Path;

use pipe_parser::SourceSpan;
use pipe_stages::{
    catalog_lookup, streams_to_show, NodeKind, PipelineTopology, StageRegistry, StreamBounds,
    TopoNode,
};

use crate::ids::NodeId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x <= self.x + self.w && y <= self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NodeFlags {
    pub unknown: bool,
    pub script: bool,
}

/// Visual-only offsets keyed by `(pipeline, stage)`, reapplied after every reparse.
pub type NodeNudges = HashMap<(u32, u32), (f32, f32)>;

/// Session-only top-left positions keyed by stable [`NodeId`].
pub type NodePositions = HashMap<NodeId, (f32, f32)>;

#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub id: NodeId,
    pub index: usize,
    pub rect: Rect,
    pub title: String,
    pub detail: String,
    pub hover_name: String,
    pub kind: NodeKind,
    pub verb: String,
    pub running: String,
    pub unknown: bool,
    pub script: bool,
    pub pipeline: u32,
    pub stage: u32,
    pub label: Option<String>,
    /// Full clause for the hover tooltip (`< examples/marina/vessels.jsonl`).
    pub full_text: String,
    /// File stages keep the end of the path when the subtitle does not fit.
    pub keep_end: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct PlacedEdge {
    pub writer: usize,
    pub reader: usize,
    pub writer_stream: u32,
    pub reader_stream: u32,
    pub from: (f32, f32),
    pub to: (f32, f32),
}

/// A numbered connection point on a stage, including unused catalog streams.
#[derive(Debug, Clone)]
pub struct PlacedPort {
    pub node_index: usize,
    pub output: bool,
    pub stream: u32,
    pub pos: (f32, f32),
    /// Display word from the catalog or a session rename (`matched`).
    pub word: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutDirection {
    /// Left to right; secondary streams take their own row.
    #[default]
    Horizontal,
    /// Top to bottom; secondary streams take their own column.
    Vertical,
}

impl LayoutDirection {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "horizontal" | "h" | "lr" | "left-to-right" => Some(Self::Horizontal),
            "vertical" | "v" | "tb" | "top-to-bottom" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphView {
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<PlacedEdge>,
    pub ports: Vec<PlacedPort>,
    pub width: f32,
    pub height: f32,
    pub direction: LayoutDirection,
}

pub const NODE_W: f32 = 196.0;
pub const NODE_H: f32 = 56.0;
pub const GAP_X: f32 = 120.0;
pub const GAP_Y: f32 = 128.0;
/// Vertical gap between connection points on a multi-stream stage.
pub const PORT_GAP: f32 = 28.0;
const MARGIN: f32 = 24.0;
const TITLE_CHAR_W: f32 = 8.5;
const BOX_PAD_X: f32 = 36.0;
const PORT_STUB: f32 = 18.0;
const BACK_EDGE_DROP: f32 = 36.0;
/// Perpendicular gap between parallel segments that would otherwise merge.
const LANE_GAP: f32 = 10.0;
const LANE_POS_EPS: f32 = 2.0;
const LANE_OVERLAP: f32 = 8.0;

/// Box width is at least [`NODE_W`] and never narrower than the stage name.
pub fn min_node_width(title: &str) -> f32 {
    let n = title.chars().count() as f32;
    (BOX_PAD_X + n * TITLE_CHAR_W + BOX_PAD_X).max(NODE_W)
}

/// Extra height so multiple connection points (and their words) can sit apart.
pub fn node_height_for_ports(n_ports: usize) -> f32 {
    NODE_H + n_ports.saturating_sub(1) as f32 * PORT_GAP
}

/// Width along the top/bottom edge so port words sit apart in vertical layout.
pub fn node_width_for_ports(n_ports: usize) -> f32 {
    if n_ports <= 1 {
        NODE_W
    } else {
        (BOX_PAD_X * 2.0 + n_ports as f32 * 88.0).max(NODE_W)
    }
}

pub fn node_size(title: &str, n_ports: usize, dir: LayoutDirection) -> (f32, f32) {
    let name_w = min_node_width(title);
    if dir.is_vertical() {
        (name_w.max(node_width_for_ports(n_ports)), NODE_H)
    } else {
        (name_w, node_height_for_ports(n_ports))
    }
}

/// True when two boxes overlap (not merely touch).
pub fn boxes_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w - 0.5 && b.x < a.x + a.w - 0.5 && a.y < b.y + b.h - 0.5 && b.y < a.y + a.h - 0.5
}

pub fn catalog_streams(
    verb: &str,
    connected_in: &[u32],
    connected_out: &[u32],
) -> (Vec<u32>, Vec<u32>) {
    let registry = StageRegistry::default();
    let entry = catalog_lookup(verb, &registry);
    let in_bounds = entry.map(|e| e.inputs).unwrap_or(StreamBounds {
        min: if connected_in.is_empty() { 0 } else { 1 },
        max: None,
    });
    let out_bounds = entry.map(|e| e.outputs).unwrap_or(StreamBounds {
        min: if connected_out.is_empty() { 0 } else { 1 },
        max: None,
    });
    let labeled_in = entry
        .map(|e| e.input_labels.iter().map(|(s, _)| *s).collect::<Vec<_>>())
        .unwrap_or_default();
    let labeled_out = entry
        .map(|e| e.output_labels.iter().map(|(s, _)| *s).collect::<Vec<_>>())
        .unwrap_or_default();
    let ins = streams_to_show(in_bounds, connected_in.iter().copied().chain(labeled_in));
    let outs = streams_to_show(out_bounds, connected_out.iter().copied().chain(labeled_out));
    (ins, outs)
}

/// Y offset of `stream` among the streams on that side of the box.
pub fn port_offset(h: f32, streams: &[u32], stream: u32) -> f32 {
    let mut s: Vec<u32> = streams.to_vec();
    if !s.contains(&stream) {
        s.push(stream);
    }
    s.sort();
    s.dedup();
    let n = s.len();
    let i = s.iter().position(|&x| x == stream).unwrap_or(0);
    if n <= 1 {
        return h * 0.5;
    }
    let m = 14.0_f32;
    m + (h - 2.0 * m) * (i as f32) / (n as f32 - 1.0)
}

pub fn layout(topo: &PipelineTopology, flags: &[NodeFlags]) -> GraphView {
    layout_dir(topo, flags, LayoutDirection::Horizontal)
}

pub fn layout_dir(topo: &PipelineTopology, flags: &[NodeFlags], dir: LayoutDirection) -> GraphView {
    let vertical = dir.is_vertical();
    let flow_gap = if vertical { GAP_Y } else { GAP_X };
    let cross_gap = if vertical { GAP_X } else { GAP_Y };
    let mut cin: HashMap<usize, Vec<u32>> = HashMap::new();
    let mut cout: HashMap<usize, Vec<u32>> = HashMap::new();
    for e in &topo.edges {
        cout.entry(e.writer).or_default().push(e.writer_stream);
        cin.entry(e.reader).or_default().push(e.reader_stream);
    }
    let mut nodes = Vec::new();
    let mut along: Vec<f32> = Vec::new();
    let mut lane_extent: Vec<f32> = Vec::new();
    let mut lane_pos: Vec<f32> = Vec::new();
    for (i, n) in topo.nodes.iter().enumerate() {
        let p = n.pipeline.saturating_sub(1) as usize;
        while along.len() <= p {
            along.push(0.0);
            lane_extent.push(if vertical { NODE_W } else { NODE_H });
            let pos = if p == 0 {
                MARGIN
            } else {
                lane_pos[p - 1] + lane_extent[p - 1] + cross_gap
            };
            lane_pos.push(pos);
        }
        if n.kind == NodeKind::LabelReference {
            continue;
        }
        let flag = flags.get(i).copied().unwrap_or_default();
        let title = node_title(n.kind, &n.verb, n.label.as_deref(), flag.script);
        let empty: Vec<u32> = Vec::new();
        let (ins, outs) = catalog_streams(
            &n.verb,
            cin.get(&i).unwrap_or(&empty),
            cout.get(&i).unwrap_or(&empty),
        );
        let (w, h) = node_size(&title, ins.len().max(outs.len()), dir);
        let cross = if vertical { w } else { h };
        if cross > lane_extent[p] {
            lane_extent[p] = cross;
            for q in (p + 1)..lane_pos.len() {
                lane_pos[q] = lane_pos[q - 1] + lane_extent[q - 1] + cross_gap;
            }
        }
        let flow = MARGIN + along[p];
        let (x, y) = if vertical {
            (lane_pos[p], flow)
        } else {
            (flow, lane_pos[p])
        };
        nodes.push(PlacedNode {
            id: 0,
            index: i,
            rect: Rect { x, y, w, h },
            title,
            detail: node_detail(n, flag),
            hover_name: node_hover_name(n),
            kind: n.kind,
            verb: n.verb.clone(),
            running: n.running.clone(),
            unknown: flag.unknown,
            script: flag.script,
            pipeline: n.pipeline,
            stage: n.stage,
            label: n.label.clone(),
            full_text: node_full_text(n),
            keep_end: is_file_stage_verb(&n.verb),
            span: n.span,
        });
        along[p] += if vertical { h } else { w } + flow_gap;
    }
    for n in &mut nodes {
        let p = n.pipeline.saturating_sub(1) as usize;
        if p < lane_pos.len() {
            if vertical {
                n.rect.x = lane_pos[p];
            } else {
                n.rect.y = lane_pos[p];
            }
        }
    }
    if vertical {
        let pairs: Vec<(usize, usize)> = topo.edges.iter().map(|e| (e.writer, e.reader)).collect();
        align_secondary_to_writers(&mut nodes, &pairs);
    }
    let mut by_index = vec![None; topo.nodes.len()];
    for (k, n) in nodes.iter().enumerate() {
        by_index[n.index] = Some(k);
    }
    let mut edges = Vec::new();
    for e in &topo.edges {
        let Some(si) = by_index.get(e.writer).copied().flatten() else {
            continue;
        };
        let Some(di) = by_index.get(e.reader).copied().flatten() else {
            continue;
        };
        let s = &nodes[si];
        let d = &nodes[di];
        let empty: Vec<u32> = Vec::new();
        let (_, s_out) = catalog_streams(
            &s.verb,
            cin.get(&e.writer).unwrap_or(&empty),
            cout.get(&e.writer).unwrap_or(&empty),
        );
        let (d_in, _) = catalog_streams(
            &d.verb,
            cin.get(&e.reader).unwrap_or(&empty),
            cout.get(&e.reader).unwrap_or(&empty),
        );
        let from = port_xy(&s.rect, true, e.writer_stream, &s_out, dir);
        let to = port_xy(&d.rect, false, e.reader_stream, &d_in, dir);
        edges.push(PlacedEdge {
            writer: e.writer,
            reader: e.reader,
            writer_stream: e.writer_stream,
            reader_stream: e.reader_stream,
            from,
            to,
        });
    }
    let width = nodes
        .iter()
        .map(|n| n.rect.x + n.rect.w)
        .fold(MARGIN, f32::max)
        + MARGIN;
    let height = nodes
        .iter()
        .map(|n| n.rect.y + n.rect.h)
        .fold(240.0, f32::max)
        + 80.0;
    GraphView {
        nodes,
        edges,
        ports: Vec::new(),
        width: width.max(400.0),
        height: height.max(240.0),
        direction: dir,
    }
}

/// In vertical layout, sit a secondary pipeline beside the stage that writes into it.
fn align_secondary_to_writers(nodes: &mut [PlacedNode], edges: &[(usize, usize)]) {
    let mut first_in_pipe: HashMap<u32, usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        first_in_pipe.entry(n.pipeline).or_insert(i);
    }
    let mut shift: HashMap<u32, f32> = HashMap::new();
    for (&pipe, &ni) in &first_in_pipe {
        if pipe <= 1 {
            continue;
        }
        let idx = nodes[ni].index;
        let Some(&(w_idx, _)) = edges.iter().find(|(_, r)| *r == idx) else {
            continue;
        };
        let Some(w) = nodes.iter().find(|n| n.index == w_idx) else {
            continue;
        };
        let dy = w.rect.y - nodes[ni].rect.y;
        if dy.abs() > 0.5 {
            shift.insert(pipe, dy);
        }
    }
    for n in nodes.iter_mut() {
        if let Some(&dy) = shift.get(&n.pipeline) {
            n.rect.y += dy;
        }
    }
}

fn port_xy(
    rect: &Rect,
    output: bool,
    stream: u32,
    streams: &[u32],
    dir: LayoutDirection,
) -> (f32, f32) {
    if dir.is_vertical() {
        let x = rect.x + port_offset(rect.w, streams, stream);
        if output {
            (x, rect.y + rect.h)
        } else {
            (x, rect.y)
        }
    } else {
        let y = rect.y + port_offset(rect.h, streams, stream);
        if output {
            (rect.x + rect.w, y)
        } else {
            (rect.x, y)
        }
    }
}

pub fn apply_nudges(view: &mut GraphView, nudges: &NodeNudges) {
    for n in &mut view.nodes {
        if let Some(&(dx, dy)) = nudges.get(&(n.pipeline, n.stage)) {
            n.rect.x += dx;
            n.rect.y += dy;
        }
    }
    finish_placement(view);
}

/// Move boxes to stored top-left positions. Unknown ids keep the auto-layout spot.
pub fn apply_positions(view: &mut GraphView, positions: &NodePositions) {
    for n in &mut view.nodes {
        if let Some(&(x, y)) = positions.get(&n.id) {
            n.rect.x = x;
            n.rect.y = y;
        }
    }
    finish_placement(view);
}

fn finish_placement(view: &mut GraphView) {
    recompute_ports(view);
    view.width = view
        .nodes
        .iter()
        .map(|n| n.rect.x + n.rect.w)
        .fold(400.0, f32::max)
        + 80.0;
    view.height = view
        .nodes
        .iter()
        .map(|n| n.rect.y + n.rect.h)
        .fold(240.0, f32::max)
        + 80.0;
}

pub fn recompute_ports(view: &mut GraphView) {
    let mut by_index: HashMap<usize, usize> = HashMap::new();
    for (k, n) in view.nodes.iter().enumerate() {
        by_index.insert(n.index, k);
    }
    let mut side: HashMap<(usize, bool), Vec<u32>> = HashMap::new();
    for p in &view.ports {
        side.entry((p.node_index, p.output))
            .or_default()
            .push(p.stream);
    }
    for e in &mut view.edges {
        let Some(&si) = by_index.get(&e.writer) else {
            continue;
        };
        let Some(&di) = by_index.get(&e.reader) else {
            continue;
        };
        let s = &view.nodes[si];
        let d = &view.nodes[di];
        let empty: Vec<u32> = Vec::new();
        let s_out = side.get(&(e.writer, true)).unwrap_or(&empty);
        let d_in = side.get(&(e.reader, false)).unwrap_or(&empty);
        e.from = port_xy(&s.rect, true, e.writer_stream, s_out, view.direction);
        e.to = port_xy(&d.rect, false, e.reader_stream, d_in, view.direction);
    }
    for p in &mut view.ports {
        let Some(&ni) = by_index.get(&p.node_index) else {
            continue;
        };
        let n = &view.nodes[ni];
        let empty: Vec<u32> = Vec::new();
        let streams = side.get(&(p.node_index, p.output)).unwrap_or(&empty);
        p.pos = port_xy(&n.rect, p.output, p.stream, streams, view.direction);
    }
}

/// Numbered connection points from the catalog, including unused streams.
///
/// `word_for(id, verb, output, stream)` is the display word (catalog default
/// or a session rename). It must not rewrite the pipe text.
pub fn attach_connection_points(
    view: &mut GraphView,
    word_for: &dyn Fn(NodeId, &str, bool, u32) -> String,
) {
    view.ports.clear();
    let registry = StageRegistry::default();
    for n in &view.nodes {
        let entry = catalog_lookup(&n.verb, &registry);
        let connected_in: Vec<u32> = view
            .edges
            .iter()
            .filter(|e| e.reader == n.index)
            .map(|e| e.reader_stream)
            .collect();
        let connected_out: Vec<u32> = view
            .edges
            .iter()
            .filter(|e| e.writer == n.index)
            .map(|e| e.writer_stream)
            .collect();
        let in_bounds = entry.map(|e| e.inputs).unwrap_or(StreamBounds {
            min: if connected_in.is_empty() { 0 } else { 1 },
            max: None,
        });
        let out_bounds = entry.map(|e| e.outputs).unwrap_or(StreamBounds {
            min: if connected_out.is_empty() { 0 } else { 1 },
            max: None,
        });
        let labeled_in = entry
            .map(|e| e.input_labels.iter().map(|(s, _)| *s).collect::<Vec<_>>())
            .unwrap_or_default();
        let labeled_out = entry
            .map(|e| e.output_labels.iter().map(|(s, _)| *s).collect::<Vec<_>>())
            .unwrap_or_default();
        for stream in streams_to_show(in_bounds, connected_in.iter().copied().chain(labeled_in)) {
            let connected = connected_in.contains(&stream);
            view.ports.push(PlacedPort {
                node_index: n.index,
                output: false,
                stream,
                pos: (0.0, 0.0),
                word: word_for(n.id, &n.verb, false, stream),
                connected,
            });
        }
        for stream in streams_to_show(out_bounds, connected_out.iter().copied().chain(labeled_out))
        {
            let connected = connected_out.contains(&stream);
            view.ports.push(PlacedPort {
                node_index: n.index,
                output: true,
                stream,
                pos: (0.0, 0.0),
                word: word_for(n.id, &n.verb, true, stream),
                connected,
            });
        }
    }
    recompute_ports(view);
}

pub fn node_nudge_key(n: &PlacedNode) -> (u32, u32) {
    (n.pipeline, n.stage)
}

fn vert_overlap(a: &Rect, b: &Rect) -> bool {
    a.y < b.y + b.h && b.y < a.y + a.h
}

fn horiz_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w
}

fn node_port_count(view: &GraphView, n: &PlacedNode) -> usize {
    let cin: Vec<u32> = view
        .edges
        .iter()
        .filter(|e| e.reader == n.index)
        .map(|e| e.reader_stream)
        .collect();
    let cout: Vec<u32> = view
        .edges
        .iter()
        .filter(|e| e.writer == n.index)
        .map(|e| e.writer_stream)
        .collect();
    let (ins, outs) = catalog_streams(&n.verb, &cin, &cout);
    ins.len().max(outs.len()).max(1)
}

fn resize_nodes(view: &mut GraphView) {
    let dir = view.direction;
    let counts: Vec<usize> = view
        .nodes
        .iter()
        .map(|n| node_port_count(view, n))
        .collect();
    for (n, n_ports) in view.nodes.iter_mut().zip(counts) {
        let (w, h) = node_size(&n.title, n_ports, dir);
        n.rect.w = n.rect.w.max(w);
        n.rect.h = n.rect.h.max(h);
    }
}

/// Place boxes like Auto layout: primary flow along the direction, each
/// pipeline on its own lane (row in horizontal, column in vertical).
fn place_like_auto(view: &mut GraphView) {
    resize_nodes(view);
    let vertical = view.direction.is_vertical();
    let flow_gap = if vertical { GAP_Y } else { GAP_X };
    let cross_gap = if vertical { GAP_X } else { GAP_Y };
    let mut lanes: Vec<(u32, Vec<usize>)> = Vec::new();
    for (i, n) in view.nodes.iter().enumerate() {
        if let Some(slot) = lanes.iter_mut().find(|(p, _)| *p == n.pipeline) {
            slot.1.push(i);
        } else {
            lanes.push((n.pipeline, vec![i]));
        }
    }
    lanes.sort_by_key(|(p, _)| *p);
    for (_, idxs) in &mut lanes {
        idxs.sort_by_key(|&i| view.nodes[i].stage);
    }
    let mut lane_coord = MARGIN;
    for (_, idxs) in &lanes {
        let mut extent = if vertical { NODE_W } else { NODE_H };
        for &i in idxs {
            let n = &view.nodes[i];
            extent = extent.max(if vertical { n.rect.w } else { n.rect.h });
        }
        let mut along = MARGIN;
        for &i in idxs {
            let n = &mut view.nodes[i];
            if vertical {
                n.rect.x = lane_coord;
                n.rect.y = along;
                along += n.rect.h + flow_gap;
            } else {
                n.rect.x = along;
                n.rect.y = lane_coord;
                along += n.rect.w + flow_gap;
            }
        }
        lane_coord += extent + cross_gap;
    }
    if vertical {
        let pairs: Vec<(usize, usize)> = view.edges.iter().map(|e| (e.writer, e.reader)).collect();
        align_secondary_to_writers(&mut view.nodes, &pairs);
    }
}

fn would_overlap_others(view: &GraphView, id: crate::ids::NodeId, cand: &Rect) -> bool {
    view.nodes
        .iter()
        .filter(|n| n.id != id)
        .any(|n| boxes_overlap(cand, &n.rect))
}

fn primary_flow_ok(view: &GraphView, id: crate::ids::NodeId, cand: &Rect) -> bool {
    let vertical = view.direction.is_vertical();
    let Some(n) = view.nodes.iter().find(|n| n.id == id) else {
        return true;
    };
    let idx = n.index;
    for e in &view.edges {
        if e.writer_stream != 0 || e.reader_stream != 0 {
            continue;
        }
        if e.writer == idx {
            if let Some(r) = view.nodes.iter().find(|o| o.index == e.reader) {
                if r.id == id {
                    continue;
                }
                if vertical {
                    if cand.y + cand.h + GAP_Y - 0.5 > r.rect.y {
                        return false;
                    }
                } else if cand.x + cand.w + GAP_X - 0.5 > r.rect.x {
                    return false;
                }
            }
        }
        if e.reader == idx {
            if let Some(w) = view.nodes.iter().find(|o| o.index == e.writer) {
                if w.id == id {
                    continue;
                }
                if vertical {
                    if cand.y + 0.5 < w.rect.y + w.rect.h + GAP_Y {
                        return false;
                    }
                } else if cand.x + 0.5 < w.rect.x + w.rect.w + GAP_X {
                    return false;
                }
            }
        }
    }
    true
}

fn enforce_readable(view: &mut GraphView) {
    let vertical = view.direction.is_vertical();
    let flow_gap = if vertical { GAP_Y } else { GAP_X };
    let cross_gap = if vertical { GAP_X } else { GAP_Y };
    let by_topo: HashMap<usize, usize> = view
        .nodes
        .iter()
        .enumerate()
        .map(|(k, n)| (n.index, k))
        .collect();
    for e in view.edges.clone() {
        if e.writer_stream == 0 && e.reader_stream == 0 {
            continue;
        }
        let Some(&wi) = by_topo.get(&e.writer) else {
            continue;
        };
        let Some(&ri) = by_topo.get(&e.reader) else {
            continue;
        };
        if wi == ri {
            continue;
        }
        if vertical {
            let wx = view.nodes[wi].rect.x;
            let ww = view.nodes[wi].rect.w;
            if (view.nodes[ri].rect.x - wx).abs() < NODE_W * 0.8 {
                view.nodes[ri].rect.x = wx + ww + cross_gap;
            }
        } else {
            let wy = view.nodes[wi].rect.y;
            let wh = view.nodes[wi].rect.h;
            if (view.nodes[ri].rect.y - wy).abs() < NODE_H * 0.8 {
                view.nodes[ri].rect.y = wy + wh + cross_gap;
            }
        }
    }
    for e in view.edges.clone() {
        if e.writer_stream != 0 || e.reader_stream != 0 {
            continue;
        }
        let Some(&wi) = by_topo.get(&e.writer) else {
            continue;
        };
        let Some(&ri) = by_topo.get(&e.reader) else {
            continue;
        };
        if wi == ri {
            continue;
        }
        if vertical {
            let min_y = view.nodes[wi].rect.y + view.nodes[wi].rect.h + flow_gap;
            if view.nodes[ri].rect.y < min_y - 0.5 {
                view.nodes[ri].rect.y = min_y;
            }
        } else {
            let min_x = view.nodes[wi].rect.x + view.nodes[wi].rect.w + flow_gap;
            if view.nodes[ri].rect.x < min_x - 0.5 {
                view.nodes[ri].rect.x = min_x;
            }
        }
    }
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 64 {
            break;
        }
        let mut moved = false;
        let mut order: Vec<usize> = (0..view.nodes.len()).collect();
        order.sort_by(|a, b| {
            let (aa, bb) = if vertical {
                (view.nodes[*a].rect.y, view.nodes[*b].rect.y)
            } else {
                (view.nodes[*a].rect.x, view.nodes[*b].rect.x)
            };
            aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in 0..order.len() {
            for j in i + 1..order.len() {
                let a = order[i];
                let b = order[j];
                let ar = view.nodes[a].rect;
                let br = view.nodes[b].rect;
                if vertical {
                    if !horiz_overlap(&ar, &br) {
                        continue;
                    }
                    let min_y = ar.y + ar.h + flow_gap;
                    if view.nodes[b].rect.y < min_y - 0.5 {
                        view.nodes[b].rect.y = min_y;
                        moved = true;
                    }
                } else {
                    if !vert_overlap(&ar, &br) {
                        continue;
                    }
                    let min_x = ar.x + ar.w + flow_gap;
                    if view.nodes[b].rect.x < min_x - 0.5 {
                        view.nodes[b].rect.x = min_x;
                        moved = true;
                    }
                }
            }
        }
        if !moved {
            break;
        }
    }
}

/// After a graph edit, re-run Auto layout, then keep a dragged box only when
/// it still leaves a normal gap and does not reverse the primary stream.
pub fn tidy_session_layout(view: &mut GraphView, positions: &mut NodePositions) {
    place_like_auto(view);
    let mut order: Vec<usize> = (0..view.nodes.len()).collect();
    order.sort_by(|a, b| {
        view.nodes[*a]
            .pipeline
            .cmp(&view.nodes[*b].pipeline)
            .then(view.nodes[*a].stage.cmp(&view.nodes[*b].stage))
    });
    for i in order {
        let id = view.nodes[i].id;
        let Some(&(x, y)) = positions.get(&id) else {
            continue;
        };
        let cand = Rect {
            x,
            y,
            w: view.nodes[i].rect.w,
            h: view.nodes[i].rect.h,
        };
        if would_overlap_others(view, id, &cand) {
            continue;
        }
        if !primary_flow_ok(view, id, &cand) {
            continue;
        }
        view.nodes[i].rect.x = x;
        view.nodes[i].rect.y = y;
    }
    enforce_readable(view);
    for n in &view.nodes {
        positions.insert(n.id, (n.rect.x, n.rect.y));
    }
    finish_placement(view);
}

fn node_detail(n: &TopoNode, flag: NodeFlags) -> String {
    if flag.unknown {
        return "unknown verb".into();
    }
    if flag.script {
        let lang = script_language(&n.verb).unwrap_or("script");
        let args = script_args(&n.running, &n.verb);
        if args.is_empty() {
            return lang.to_string();
        }
        return format!("{lang}  {args}");
    }
    let running = n.running.trim();
    if running.is_empty() || running.eq_ignore_ascii_case(&n.verb) {
        match n.kind {
            NodeKind::LabelDefinition => "label (COPY at run)".into(),
            NodeKind::Connector => "CALLPIPE connector".into(),
            _ => String::new(),
        }
    } else {
        running
            .strip_prefix(&n.verb)
            .unwrap_or(running)
            .trim()
            .to_string()
    }
}

fn node_hover_name(n: &TopoNode) -> String {
    match n.label.as_deref() {
        Some(l) if !n.verb.is_empty() => format!("{l}: {}", n.verb),
        Some(l) => l.to_string(),
        None => n.verb.clone(),
    }
}

fn node_full_text(n: &TopoNode) -> String {
    let running = n.running.trim();
    if !running.is_empty() {
        running.to_string()
    } else {
        node_hover_name(n)
    }
}

/// Visible cut marker when stage argument text does not fit the box.
pub const LABEL_ELLIPSIS: char = '…';

/// True for `<`, `>`, `>>`, and the DISK/FILE family: the path's file name
/// matters more than its leading directories.
pub fn is_file_stage_verb(verb: &str) -> bool {
    let v = verb.trim();
    if matches!(v, "<" | ">" | ">>" | ">>>") {
        return true;
    }
    matches!(
        v.to_ascii_uppercase().as_str(),
        "DISK"
            | "FILE"
            | "FILEFAST"
            | "DISKFAST"
            | "FILEBACK"
            | "DISKBACK"
            | "FILERAND"
            | "DISKRAND"
            | "FILESLOW"
            | "DISKSLOW"
            | "FILEUPDATE"
            | "DISKUPDATE"
    )
}

/// Fit `text` into `max_chars` Unicode scalars. `keep_end` drops a leading
/// path prefix so a file name such as `vessels.jsonl` stays visible.
pub fn ellipsize_label(text: &str, max_chars: usize, keep_end: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return LABEL_ELLIPSIS.to_string();
    }
    if keep_end {
        ellipsize_keep_end(&chars, max_chars)
    } else {
        let mut s: String = chars.iter().take(max_chars - 1).copied().collect();
        s.push(LABEL_ELLIPSIS);
        s
    }
}

fn ellipsize_keep_end(chars: &[char], max_chars: usize) -> String {
    let budget = max_chars - 1;
    let min_start = chars.len().saturating_sub(budget);
    let start = (min_start..chars.len())
        .find(|&i| chars[i] == '/' || chars[i] == '\\')
        .unwrap_or(min_start);
    let mut s = String::from(LABEL_ELLIPSIS);
    s.extend(chars[start..].iter().copied());
    s
}

/// Orthogonal elbows stay in the gutter (writer stub, then across, then into the reader).
/// Direct mode is a single segment. Topology is unchanged. Unlaned: used as the base polyline
/// before `route_all` spreads parallel segments.
pub fn edge_points(from: (f32, f32), to: (f32, f32), orthogonal: bool) -> Vec<(f32, f32)> {
    edge_points_dir(from, to, orthogonal, LayoutDirection::Horizontal)
}

pub fn edge_points_dir(
    from: (f32, f32),
    to: (f32, f32),
    orthogonal: bool,
    dir: LayoutDirection,
) -> Vec<(f32, f32)> {
    if !orthogonal {
        return vec![from, to];
    }
    if dir.is_vertical() {
        let a = (from.0, from.1 + PORT_STUB);
        let b = (to.0, to.1 - PORT_STUB);
        let dx = (from.0 - to.0).abs();
        if dx < 1.5 && a.1 <= b.1 + 1.0 {
            if (b.1 - a.1).abs() < 1.0 {
                return vec![from, to];
            }
            return vec![from, a, b, to];
        }
        if a.1 + 4.0 <= b.1 {
            vec![from, a, (b.0, a.1), b, to]
        } else {
            let x = from.0.max(to.0) + BACK_EDGE_DROP;
            vec![from, a, (x, a.1), (x, b.1), b, to]
        }
    } else {
        let a = (from.0 + PORT_STUB, from.1);
        let b = (to.0 - PORT_STUB, to.1);
        let dy = (from.1 - to.1).abs();
        if dy < 1.5 && a.0 <= b.0 + 1.0 {
            if (b.0 - a.0).abs() < 1.0 {
                return vec![from, to];
            }
            return vec![from, a, b, to];
        }
        if a.0 + 4.0 <= b.0 {
            vec![from, a, (a.0, b.1), b, to]
        } else {
            let y = from.1.max(to.1) + BACK_EDGE_DROP;
            vec![from, a, (a.0, y), (b.0, y), b, to]
        }
    }
}

/// Routed polylines for every edge. Parallel segments that would sit on top of
/// each other are given distinct lanes; ports stay pinned to the node boxes.
pub fn route_all(view: &GraphView, orthogonal: bool) -> Vec<Vec<(f32, f32)>> {
    if orthogonal {
        orthogonal_routes(view)
    } else {
        direct_routes(view)
    }
}

pub fn edge_route(view: &GraphView, edge_index: usize, orthogonal: bool) -> Vec<(f32, f32)> {
    route_all(view, orthogonal)
        .into_iter()
        .nth(edge_index)
        .unwrap_or_default()
}

/// Point on the longest segment, for stream labels and edge marks.
pub fn route_label_anchor(pts: &[(f32, f32)]) -> (f32, f32) {
    if pts.len() < 2 {
        return pts.first().copied().unwrap_or((0.0, 0.0));
    }
    let mut best_len = -1.0f32;
    let mut best = pts[0];
    for w in pts.windows(2) {
        let len = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        if len > best_len {
            best_len = len;
            best = ((w[0].0 + w[1].0) * 0.5, (w[0].1 + w[1].1) * 0.5);
        }
    }
    best
}

fn orthogonal_routes(view: &GraphView) -> Vec<Vec<(f32, f32)>> {
    let mut routes: Vec<Vec<(f32, f32)>> = view
        .edges
        .iter()
        .map(|e| orthogonal_route_one(view, e))
        .collect();
    spread_axis_lanes(&mut routes, false);
    spread_axis_lanes(&mut routes, true);
    push_routes_out_of_nodes(view, &mut routes);
    spread_axis_lanes(&mut routes, false);
    spread_axis_lanes(&mut routes, true);
    routes
}

fn node_by_topo(view: &GraphView, topo: usize) -> Option<&PlacedNode> {
    view.nodes.iter().find(|n| n.index == topo)
}

fn orthogonal_route_one(view: &GraphView, e: &PlacedEdge) -> Vec<(f32, f32)> {
    let pts = edge_points_dir(e.from, e.to, true, view.direction);
    if !interior_hits_foreign(view, e, &pts) {
        return pts;
    }
    if view.direction.is_vertical() {
        let bar_x = pick_clear_bar_x(view, e);
        let ay = e.from.1 + PORT_STUB;
        let by = e.to.1 - PORT_STUB;
        vec![
            e.from,
            (e.from.0, ay),
            (bar_x, ay),
            (bar_x, by),
            (e.to.0, by),
            e.to,
        ]
    } else {
        let bar_y = pick_clear_bar_y(view, e);
        let ax = e.from.0 + PORT_STUB;
        let bx = e.to.0 - PORT_STUB;
        vec![
            e.from,
            (ax, e.from.1),
            (ax, bar_y),
            (bx, bar_y),
            (bx, e.to.1),
            e.to,
        ]
    }
}

fn interior_hits_foreign(view: &GraphView, e: &PlacedEdge, pts: &[(f32, f32)]) -> bool {
    if pts.len() < 2 {
        return false;
    }
    for w in pts.windows(2) {
        for n in &view.nodes {
            if n.index == e.writer || n.index == e.reader {
                continue;
            }
            if aa_seg_hits_rect(w[0], w[1], n.rect) {
                return true;
            }
        }
    }
    false
}

fn pick_clear_bar_y(view: &GraphView, e: &PlacedEdge) -> f32 {
    let dest = node_by_topo(view, e.reader);
    let src = node_by_topo(view, e.writer);
    let from_below = e.from.1 > e.to.1 + 4.0;
    let mut y = if let Some(d) = dest {
        if from_below {
            d.rect.y + d.rect.h + 12.0
        } else if e.from.1 + 4.0 < e.to.1 {
            d.rect.y - 12.0
        } else {
            d.rect.y + d.rect.h + 12.0
        }
    } else {
        e.to.1.max(e.from.1) + BACK_EDGE_DROP
    };
    if y < 8.0 {
        y = dest
            .map(|d| d.rect.y + d.rect.h + 12.0)
            .unwrap_or(MARGIN + NODE_H + 12.0);
    }
    let x0 = e.from.0.min(e.to.0);
    let x1 = e.from.0.max(e.to.0);
    let hits = |y: f32| {
        view.nodes.iter().any(|n| {
            n.index != e.writer
                && n.index != e.reader
                && y >= n.rect.y - 4.0
                && y <= n.rect.y + n.rect.h + 4.0
                && x0 < n.rect.x + n.rect.w
                && x1 > n.rect.x
        })
    };
    if !hits(y) {
        return y;
    }
    if let Some(d) = dest {
        for cand in [
            d.rect.y - 12.0,
            d.rect.y + d.rect.h + 12.0,
            src.map(|s| s.rect.y + s.rect.h + 12.0).unwrap_or(y),
            src.map(|s| s.rect.y - 12.0).unwrap_or(y),
        ] {
            if cand >= 8.0 && !hits(cand) {
                return cand;
            }
        }
    }
    y.max(8.0)
}

fn pick_clear_bar_x(view: &GraphView, e: &PlacedEdge) -> f32 {
    let dest = node_by_topo(view, e.reader);
    let src = node_by_topo(view, e.writer);
    let from_right = e.from.0 > e.to.0 + 4.0;
    let mut x = if let Some(d) = dest {
        if from_right {
            d.rect.x + d.rect.w + 12.0
        } else if e.from.0 + 4.0 < e.to.0 {
            d.rect.x - 12.0
        } else {
            d.rect.x + d.rect.w + 12.0
        }
    } else {
        e.to.0.max(e.from.0) + BACK_EDGE_DROP
    };
    if x < 8.0 {
        x = dest
            .map(|d| d.rect.x + d.rect.w + 12.0)
            .unwrap_or(MARGIN + NODE_W + 12.0);
    }
    let y0 = e.from.1.min(e.to.1);
    let y1 = e.from.1.max(e.to.1);
    let hits = |x: f32| {
        view.nodes.iter().any(|n| {
            n.index != e.writer
                && n.index != e.reader
                && x >= n.rect.x - 4.0
                && x <= n.rect.x + n.rect.w + 4.0
                && y0 < n.rect.y + n.rect.h
                && y1 > n.rect.y
        })
    };
    if !hits(x) {
        return x;
    }
    if let Some(d) = dest {
        for cand in [
            d.rect.x - 12.0,
            d.rect.x + d.rect.w + 12.0,
            src.map(|s| s.rect.x + s.rect.w + 12.0).unwrap_or(x),
            src.map(|s| s.rect.x - 12.0).unwrap_or(x),
        ] {
            if cand >= 8.0 && !hits(cand) {
                return cand;
            }
        }
    }
    x.max(8.0)
}

fn direct_routes(view: &GraphView) -> Vec<Vec<(f32, f32)>> {
    let n = view.edges.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let find = |parent: &mut [usize], mut i: usize| {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    };
    for i in 0..n {
        for j in (i + 1)..n {
            if direct_parallel_overlap(
                view.edges[i].from,
                view.edges[i].to,
                view.edges[j].from,
                view.edges[j].to,
            )
            .is_some()
            {
                let a = find(&mut parent, i);
                let b = find(&mut parent, j);
                if a != b {
                    parent[b] = a;
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        groups.entry(find(&mut parent, i)).or_default().push(i);
    }
    let mut off = vec![0.0f32; n];
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let mut order = members.clone();
        order.sort_by(|a, b| {
            let ay = view.edges[*a].from.1 + view.edges[*a].to.1;
            let by = view.edges[*b].from.1 + view.edges[*b].to.1;
            ay.partial_cmp(&by)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        });
        let k = order.len() as f32;
        for (lane, &ei) in order.iter().enumerate() {
            off[ei] = (lane as f32 - (k - 1.0) * 0.5) * LANE_GAP;
        }
    }
    let mut routes = Vec::with_capacity(n);
    for (i, e) in view.edges.iter().enumerate() {
        if off[i].abs() < 0.5 {
            routes.push(vec![e.from, e.to]);
            continue;
        }
        let dx = e.to.0 - e.from.0;
        let dy = e.to.1 - e.from.1;
        let len = dx.hypot(dy).max(1.0);
        let ux = dx / len;
        let uy = dy / len;
        let px = -uy;
        let py = ux;
        let o = off[i];
        let stub = PORT_STUB.min(len * 0.25);
        routes.push(vec![
            e.from,
            (e.from.0 + ux * stub + px * o, e.from.1 + uy * stub + py * o),
            (e.to.0 - ux * stub + px * o, e.to.1 - uy * stub + py * o),
            e.to,
        ]);
    }
    push_routes_out_of_nodes(view, &mut routes);
    routes
}

fn spread_axis_lanes(routes: &mut [Vec<(f32, f32)>], horiz: bool) {
    #[derive(Clone, Copy)]
    struct Item {
        ei: usize,
        si: usize,
        pos: f32,
        lo: f32,
        hi: f32,
    }
    let mut items = Vec::new();
    for (ei, pts) in routes.iter().enumerate() {
        if pts.len() < 3 {
            continue;
        }
        for si in 0..pts.len().saturating_sub(1) {
            if si == 0 || si + 1 == pts.len() - 1 {
                continue;
            }
            let a = pts[si];
            let b = pts[si + 1];
            let is_h = (a.1 - b.1).abs() < 0.75 && (a.0 - b.0).abs() >= LANE_OVERLAP;
            let is_v = (a.0 - b.0).abs() < 0.75 && (a.1 - b.1).abs() >= LANE_OVERLAP;
            if horiz && is_h {
                items.push(Item {
                    ei,
                    si,
                    pos: (a.1 + b.1) * 0.5,
                    lo: a.0.min(b.0),
                    hi: a.0.max(b.0),
                });
            } else if !horiz && is_v {
                items.push(Item {
                    ei,
                    si,
                    pos: (a.0 + b.0) * 0.5,
                    lo: a.1.min(b.1),
                    hi: a.1.max(b.1),
                });
            }
        }
    }
    if items.len() < 2 {
        return;
    }
    let mut buckets: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        let key = (it.pos / LANE_POS_EPS).round() as i32;
        buckets.entry(key).or_default().push(i);
    }
    for idxs in buckets.values() {
        if idxs.len() < 2 {
            continue;
        }
        let mut parent: Vec<usize> = (0..idxs.len()).collect();
        let find = |parent: &mut [usize], mut i: usize| {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        };
        for a in 0..idxs.len() {
            for b in (a + 1)..idxs.len() {
                let ia = items[idxs[a]];
                let ib = items[idxs[b]];
                if ia.lo < ib.hi - 1.0 && ib.lo < ia.hi - 1.0 {
                    let pa = find(&mut parent, a);
                    let pb = find(&mut parent, b);
                    if pa != pb {
                        parent[pb] = pa;
                    }
                }
            }
        }
        let mut comps: HashMap<usize, Vec<usize>> = HashMap::new();
        for a in 0..idxs.len() {
            comps.entry(find(&mut parent, a)).or_default().push(idxs[a]);
        }
        for members in comps.values() {
            if members.len() < 2 {
                continue;
            }
            let mut order = members.clone();
            order.sort_by(|a, b| {
                items[*a]
                    .lo
                    .partial_cmp(&items[*b].lo)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(items[*a].ei.cmp(&items[*b].ei))
            });
            let lanes = interval_lanes(
                &order
                    .iter()
                    .map(|i| (items[*i].lo, items[*i].hi))
                    .collect::<Vec<_>>(),
            );
            let n_lanes = lanes.iter().copied().max().unwrap_or(0) + 1;
            if n_lanes < 2 {
                continue;
            }
            let base = items[order[0]].pos;
            for (k, &ii) in order.iter().enumerate() {
                let it = items[ii];
                let lane = lanes[k] as f32;
                let raw = base + (lane - (n_lanes as f32 - 1.0) * 0.5) * LANE_GAP;
                let mut pos = raw;
                if let (Some(from), Some(to)) = (
                    routes[it.ei].first().copied(),
                    routes[it.ei].last().copied(),
                ) {
                    if !horiz && to.0 - from.0 > 16.0 {
                        pos = pos.clamp(from.0 + 6.0, to.0 - 6.0);
                    }
                    if horiz && to.1 - from.1 > 16.0 {
                        pos = pos.clamp(from.1 + 6.0, to.1 - 6.0);
                    }
                }
                pos = pos.clamp(base - 40.0, base + 40.0);
                let pts = &mut routes[it.ei];
                if it.si + 1 >= pts.len() {
                    continue;
                }
                if horiz {
                    pts[it.si].1 = pos;
                    pts[it.si + 1].1 = pos;
                } else {
                    pts[it.si].0 = pos;
                    pts[it.si + 1].0 = pos;
                }
            }
        }
    }
}

fn interval_lanes(spans: &[(f32, f32)]) -> Vec<usize> {
    let mut lanes = vec![0usize; spans.len()];
    let mut lane_end: Vec<f32> = Vec::new();
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|a, b| {
        spans[*a]
            .0
            .partial_cmp(&spans[*b].0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for i in order {
        let (s, e) = spans[i];
        let mut found = None;
        for (li, le) in lane_end.iter().enumerate() {
            if s >= *le - 1.0 {
                found = Some(li);
                break;
            }
        }
        let li = if let Some(li) = found {
            lane_end[li] = lane_end[li].max(e);
            li
        } else {
            lane_end.push(e);
            lane_end.len() - 1
        };
        lanes[i] = li;
    }
    lanes
}

fn push_routes_out_of_nodes(view: &GraphView, routes: &mut [Vec<(f32, f32)>]) {
    for (ei, pts) in routes.iter_mut().enumerate() {
        let e = &view.edges[ei];
        if pts.len() < 3 {
            continue;
        }
        for si in 1..pts.len().saturating_sub(1) {
            let a = pts[si];
            let b = pts[si + 1];
            for n in &view.nodes {
                if n.index == e.writer || n.index == e.reader {
                    continue;
                }
                if !aa_seg_hits_rect(a, b, n.rect) {
                    continue;
                }
                let vert = (a.0 - b.0).abs() < 0.75;
                if vert {
                    let left = n.rect.x - 8.0;
                    let right = n.rect.x + n.rect.w + 8.0;
                    let x = if (a.0 - left).abs() < (a.0 - right).abs() {
                        left
                    } else {
                        right
                    };
                    pts[si].0 = x;
                    pts[si + 1].0 = x;
                } else {
                    let top = n.rect.y - 8.0;
                    let bot = n.rect.y + n.rect.h + 8.0;
                    let y = if (a.1 - top).abs() < (a.1 - bot).abs() {
                        top
                    } else {
                        bot
                    };
                    pts[si].1 = y;
                    pts[si + 1].1 = y;
                }
            }
        }
    }
}

fn aa_seg_hits_rect(a: (f32, f32), b: (f32, f32), r: Rect) -> bool {
    let vert = (a.0 - b.0).abs() < 0.75;
    if vert {
        a.0 >= r.x && a.0 <= r.x + r.w && overlap_1d(a.1, b.1, r.y, r.y + r.h)
    } else {
        let horiz = (a.1 - b.1).abs() < 0.75;
        if !horiz {
            return false;
        }
        a.1 >= r.y && a.1 <= r.y + r.h && overlap_1d(a.0, b.0, r.x, r.x + r.w)
    }
}

fn overlap_1d(a0: f32, a1: f32, b0: f32, b1: f32) -> bool {
    a0.min(a1) < b1 - 1.0 && b0 < a0.max(a1) - 1.0
}

fn direct_parallel_overlap(
    a0: (f32, f32),
    a1: (f32, f32),
    b0: (f32, f32),
    b1: (f32, f32),
) -> Option<f32> {
    let (adx, ady) = (a1.0 - a0.0, a1.1 - a0.1);
    let (bdx, bdy) = (b1.0 - b0.0, b1.1 - b0.1);
    let al = adx.hypot(ady);
    let bl = bdx.hypot(bdy);
    if al < 8.0 || bl < 8.0 {
        return None;
    }
    let dot = (adx * bdx + ady * bdy) / (al * bl);
    if dot.abs() < 0.97 {
        return None;
    }
    let ux = adx / al;
    let uy = ady / al;
    let perp = (b0.0 - a0.0) * -uy + (b0.1 - a0.1) * ux;
    if perp.abs() > LANE_GAP * 0.65 {
        return None;
    }
    let a_lo: f32 = 0.0;
    let a_hi = al;
    let t0 = (b0.0 - a0.0) * ux + (b0.1 - a0.1) * uy;
    let t1 = (b1.0 - a0.0) * ux + (b1.1 - a0.1) * uy;
    let b_lo = t0.min(t1);
    let b_hi = t0.max(t1);
    let ov = a_hi.min(b_hi) - a_lo.max(b_lo);
    if ov < LANE_OVERLAP {
        return None;
    }
    Some(perp)
}

/// Pairs of edges whose polylines have collinear overlapping body segments.
pub fn collinear_overlapping_pairs(
    routes: &[Vec<(f32, f32)>],
    pos_eps: f32,
    min_overlap: f32,
) -> Vec<(usize, usize)> {
    let mut segs: Vec<(usize, (f32, f32), (f32, f32))> = Vec::new();
    for (ei, pts) in routes.iter().enumerate() {
        for w in pts.windows(2) {
            let len = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            if len >= min_overlap {
                segs.push((ei, w[0], w[1]));
            }
        }
    }
    let mut pairs = Vec::new();
    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            let (ei, a0, a1) = segs[i];
            let (ej, b0, b1) = segs[j];
            if ei == ej {
                continue;
            }
            let hit = aa_collinear_overlap(a0, a1, b0, b1, pos_eps, min_overlap)
                || direct_parallel_overlap(a0, a1, b0, b1).is_some_and(|d| d.abs() <= pos_eps);
            if hit {
                let p = if ei < ej { (ei, ej) } else { (ej, ei) };
                if !pairs.contains(&p) {
                    pairs.push(p);
                }
            }
        }
    }
    pairs
}

fn aa_collinear_overlap(
    a0: (f32, f32),
    a1: (f32, f32),
    b0: (f32, f32),
    b1: (f32, f32),
    pos_eps: f32,
    min_overlap: f32,
) -> bool {
    let adx = (a1.0 - a0.0).abs();
    let ady = (a1.1 - a0.1).abs();
    let bdx = (b1.0 - b0.0).abs();
    let bdy = (b1.1 - b0.1).abs();
    if ady < pos_eps && bdy < pos_eps && adx >= min_overlap && bdx >= min_overlap {
        if (a0.1 - b0.1).abs() > pos_eps {
            return false;
        }
        let ov = a0.0.max(a1.0).min(b0.0.max(b1.0)) - a0.0.min(a1.0).max(b0.0.min(b1.0));
        return ov >= min_overlap;
    }
    if adx < pos_eps && bdx < pos_eps && ady >= min_overlap && bdy >= min_overlap {
        if (a0.0 - b0.0).abs() > pos_eps {
            return false;
        }
        let ov = a0.1.max(a1.1).min(b0.1.max(b1.1)) - a0.1.min(a1.1).max(b0.1.min(b1.1));
        return ov >= min_overlap;
    }
    false
}

pub fn stream_caption(
    writer_stream: u32,
    _reader_label: Option<&str>,
    stream_word: Option<&str>,
) -> Option<String> {
    if writer_stream == 0 {
        return None;
    }
    let word = stream_word.filter(|s| !s.is_empty());
    match word {
        Some(w) => Some(w.to_string()),
        None => Some(format!("stream {writer_stream}")),
    }
}

fn node_title(kind: NodeKind, verb: &str, label: Option<&str>, script: bool) -> String {
    let shown = if script {
        script_file_name(verb)
    } else {
        verb.to_string()
    };
    match kind {
        NodeKind::LabelReference => label.unwrap_or("ref").to_string(),
        NodeKind::LabelDefinition => format!("{}:", label.unwrap_or("label")),
        NodeKind::Connector => shown,
        NodeKind::Stage => match label {
            Some(l) => format!("{l}: {shown}"),
            None => shown,
        },
    }
}

pub fn script_file_name(verb: &str) -> String {
    Path::new(verb)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(verb)
        .to_string()
}

pub fn script_language(verb: &str) -> Option<&'static str> {
    let v = verb.to_ascii_lowercase();
    if v.ends_with(".py") {
        Some("Python")
    } else if v.ends_with(".rexx") || v.ends_with(".rex") || verb.eq_ignore_ascii_case("REXX") {
        Some("REXX")
    } else {
        None
    }
}

pub fn script_args(running: &str, verb: &str) -> String {
    let t = running.trim();
    if let Some(rest) = t.strip_prefix(verb) {
        return rest.trim().to_string();
    }
    Path::new(verb)
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|base| t.find(base).map(|i| t[i + base.len()..].trim().to_string()))
        .unwrap_or_default()
}

pub fn hit_node(view: &GraphView, x: f32, y: f32) -> Option<usize> {
    view.nodes
        .iter()
        .rev()
        .find(|n| n.rect.contains(x, y))
        .map(|n| n.index)
}

/// Nearest connection point within `radius` graph units. Returns an index into `view.ports`.
pub fn hit_port(view: &GraphView, x: f32, y: f32, radius: f32) -> Option<usize> {
    let mut best = None;
    let mut best_d = radius;
    for (i, p) in view.ports.iter().enumerate() {
        let d = (p.pos.0 - x).hypot(p.pos.1 - y);
        if d <= best_d {
            best_d = d;
            best = Some(i);
        }
    }
    best
}

pub fn hit_edge(view: &GraphView, x: f32, y: f32) -> Option<usize> {
    hit_edge_route(view, x, y, true)
}

pub fn hit_edge_route(view: &GraphView, x: f32, y: f32, orthogonal: bool) -> Option<usize> {
    let mut best = None;
    let mut best_d = 10.0f32;
    for (i, pts) in route_all(view, orthogonal).iter().enumerate() {
        for w in pts.windows(2) {
            let d = dist_to_segment(x, y, w[0], w[1]);
            if d < best_d {
                best_d = d;
                best = Some(i);
            }
        }
    }
    best
}

pub fn edges_touching(view: &GraphView, topo_index: usize) -> Vec<usize> {
    view.edges
        .iter()
        .enumerate()
        .filter(|(_, e)| e.writer == topo_index || e.reader == topo_index)
        .map(|(i, _)| i)
        .collect()
}

pub fn edge_is_related(
    edge: &PlacedEdge,
    focus_node: Option<usize>,
    focus_edge: Option<usize>,
    ei: usize,
) -> bool {
    if let Some(fe) = focus_edge {
        return ei == fe;
    }
    if let Some(n) = focus_node {
        return edge.writer == n || edge.reader == n;
    }
    true
}

pub fn node_is_related(
    topo_index: usize,
    view: &GraphView,
    focus_node: Option<usize>,
    focus_edge: Option<usize>,
) -> bool {
    if let Some(n) = focus_node {
        if topo_index == n {
            return true;
        }
        return view.edges.iter().any(|e| {
            (e.writer == n && e.reader == topo_index) || (e.reader == n && e.writer == topo_index)
        });
    }
    if let Some(ei) = focus_edge {
        let e = &view.edges[ei];
        return topo_index == e.writer || topo_index == e.reader;
    }
    true
}

fn dist_to_segment(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (ax, ay) = a;
    let (bx, by) = b;
    let vx = bx - ax;
    let vy = by - ay;
    let len2 = vx * vx + vy * vy;
    if len2 < f32::EPSILON {
        return ((px - ax).hypot(py - ay)).abs();
    }
    let t = ((px - ax) * vx + (py - ay) * vy) / len2;
    let t = t.clamp(0.0, 1.0);
    let qx = ax + t * vx;
    let qy = ay + t * vy;
    (px - qx).hypot(py - qy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_parser::parse_pipeline;
    use pipe_stages::inspect_topology;

    #[test]
    fn branched_locate_has_primary_and_secondary() {
        let spec =
            "(ENDCHAR ?) literal ax|literal by|literal az | L: locate /a/ | hole ? L: | hole";
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        let view = layout(&topo, &vec![NodeFlags::default(); topo.nodes.len()]);
        let loc = topo.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let outs: Vec<_> = view
            .edges
            .iter()
            .filter(|e| e.writer == loc)
            .map(|e| e.writer_stream)
            .collect();
        assert!(outs.contains(&0), "{outs:?}");
        assert!(outs.contains(&1), "{outs:?}");
        assert_eq!(
            view.nodes
                .iter()
                .filter(|n| n.title.contains("LOCATE"))
                .count(),
            1
        );
        let loc_node = view
            .nodes
            .iter()
            .find(|n| n.title.contains("LOCATE"))
            .unwrap();
        for e in view.edges.iter().filter(|e| e.writer == loc) {
            assert!(
                (e.from.0 - (loc_node.rect.x + loc_node.rect.w)).abs() < 0.01,
                "writer port must sit on the node box"
            );
        }
        let ortho = edge_points((0.0, 10.0), (40.0, 50.0), true);
        assert!(ortho.len() >= 3);
        let direct = edge_points((0.0, 10.0), (40.0, 50.0), false);
        assert_eq!(direct.len(), 2);
    }

    #[test]
    fn locate_shows_unused_unmatched_output() {
        let spec = "literal apple | locate /ap/ | console";
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        let mut view = layout(&topo, &vec![NodeFlags::default(); topo.nodes.len()]);
        attach_connection_points(&mut view, &|_, verb, output, stream| {
            pipe_stages::catalog_lookup(verb, &StageRegistry::default())
                .and_then(|e| pipe_stages::default_stream_word(e, output, stream))
                .unwrap_or("")
                .to_string()
        });
        let loc = topo.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let outs: Vec<_> = view
            .ports
            .iter()
            .filter(|p| p.node_index == loc && p.output)
            .collect();
        assert_eq!(outs.len(), 2, "{outs:?}");
        let unmatched = outs.iter().find(|p| p.stream == 1).expect("output 1");
        assert!(!unmatched.connected, "unmatched is unused in a linear pipe");
        assert_eq!(unmatched.word, "unmatched");
        let matched = outs.iter().find(|p| p.stream == 0).expect("output 0");
        assert!(matched.connected);
        assert_eq!(matched.word, "matched");
    }

    #[test]
    fn script_node_uses_file_name_not_full_path() {
        let spec = r#"literal x | ../examples/marina/tuck.py owner"#;
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        let flags: Vec<NodeFlags> = topo
            .nodes
            .iter()
            .map(|n| NodeFlags {
                script: n.verb.to_ascii_lowercase().ends_with(".py"),
                unknown: false,
            })
            .collect();
        let view = layout(&topo, &flags);
        let n = view.nodes.iter().find(|n| n.script).expect("script");
        assert_eq!(n.title, "tuck.py");
        assert!(n.detail.contains("Python"), "{}", n.detail);
        assert!(n.detail.contains("owner"), "{}", n.detail);
        assert!(n.hover_name.contains("tuck.py"), "{}", n.hover_name);
        assert!(!n.title.contains("examples"), "clipped path was the bug");
        assert_eq!(script_language(&n.verb), Some("Python"));
        assert_eq!(script_args(&n.running, &n.verb), "owner");
    }

    #[test]
    fn orthogonal_forward_edge_stays_in_writer_gutter() {
        let pts = edge_points((100.0, 20.0), (250.0, 80.0), true);
        assert!(pts.len() >= 4);
        assert!((pts[1].0 - 118.0).abs() < 0.1, "{pts:?}");
        assert!(
            (pts[2].0 - pts[1].0).abs() < 0.1,
            "vertical in gutter {pts:?}"
        );
        assert!((pts[2].1 - 80.0).abs() < 0.1);
    }

    #[test]
    fn nudge_moves_box_and_ports_not_topology() {
        let spec = "literal a | hole";
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        let mut view = layout(&topo, &vec![NodeFlags::default(); topo.nodes.len()]);
        let x0 = view.nodes[0].rect.x;
        let mut nudges = NodeNudges::new();
        nudges.insert(node_nudge_key(&view.nodes[0]), (40.0, 12.0));
        apply_nudges(&mut view, &nudges);
        assert!((view.nodes[0].rect.x - (x0 + 40.0)).abs() < 0.01);
        assert!(
            (view.edges[0].from.0 - (view.nodes[0].rect.x + view.nodes[0].rect.w)).abs() < 0.01
        );
        assert_eq!(topo.nodes.len(), 2);
    }

    #[test]
    fn related_edges_are_incident_only() {
        let spec = "literal a | hole | hole";
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        let view = layout(&topo, &vec![NodeFlags::default(); topo.nodes.len()]);
        let mid = view.nodes[1].index;
        let rel = edges_touching(&view, mid);
        assert!(!rel.is_empty());
        for i in rel {
            assert!(view.edges[i].writer == mid || view.edges[i].reader == mid);
        }
    }

    fn sample_view(spec: &str) -> GraphView {
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        layout(&topo, &vec![NodeFlags::default(); topo.nodes.len()])
    }

    fn marina_view() -> GraphView {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/marina/to-nested.pipe");
        let spec =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        sample_view(&spec)
    }

    #[test]
    fn unlaned_marina_has_collinear_overlaps() {
        let view = marina_view();
        let raw: Vec<_> = view
            .edges
            .iter()
            .map(|e| edge_points(e.from, e.to, true))
            .collect();
        assert!(
            !collinear_overlapping_pairs(&raw, 1.5, 8.0).is_empty(),
            "precondition: marina secondaries share a gutter before lanes"
        );
    }

    #[test]
    fn routed_marina_and_locate_have_no_collinear_overlaps() {
        let marina = marina_view();
        for ortho in [true, false] {
            let routed = route_all(&marina, ortho);
            let pairs = collinear_overlapping_pairs(&routed, 1.5, 8.0);
            assert!(
                pairs.is_empty(),
                "marina ortho={ortho} collinear pairs {pairs:?}"
            );
        }
        let locate = sample_view(
            "(ENDCHAR ?) literal ax|literal by|literal az | L: locate /a/ | hole ? L: | hole",
        );
        for ortho in [true, false] {
            let routed = route_all(&locate, ortho);
            let pairs = collinear_overlapping_pairs(&routed, 1.5, 8.0);
            assert!(
                pairs.is_empty(),
                "locate ortho={ortho} collinear pairs {pairs:?}"
            );
        }
        let stacked =
            sample_view("(ENDCHAR ?) literal a | v: hole ? literal b | v: ? literal c | v:");
        let raw: Vec<_> = stacked
            .edges
            .iter()
            .map(|e| edge_points(e.from, e.to, true))
            .collect();
        assert!(
            !collinear_overlapping_pairs(&raw, 1.5, 8.0).is_empty(),
            "three feeders into one hole must overlap before lanes"
        );
        let routed = route_all(&stacked, true);
        let pairs = collinear_overlapping_pairs(&routed, 1.5, 8.0);
        assert!(pairs.is_empty(), "stacked feeders after lanes {pairs:?}");
        let routed = route_all(&stacked, false);
        let pairs = collinear_overlapping_pairs(&routed, 1.5, 8.0);
        assert!(pairs.is_empty(), "stacked feeders direct {pairs:?}");
    }

    #[test]
    fn marina_routes_do_not_run_through_foreign_nodes() {
        let view = marina_view();
        let routed = route_all(&view, true);
        for (ei, pts) in routed.iter().enumerate() {
            let e = &view.edges[ei];
            for w in pts.windows(2) {
                for n in &view.nodes {
                    if n.index == e.writer || n.index == e.reader {
                        continue;
                    }
                    assert!(
                        !aa_seg_hits_rect(w[0], w[1], n.rect),
                        "edge {ei} {:?} hits node {} {:?}",
                        pts,
                        n.title,
                        n.rect
                    );
                }
            }
        }
    }

    #[test]
    fn hit_test_picks_the_laned_edge() {
        let view = sample_view("(ENDCHAR ?) literal a | v: hole ? literal b | v: ? literal c | v:");
        let routed = route_all(&view, true);
        assert!(routed.len() >= 2);
        let mut samples = Vec::new();
        for (ei, pts) in routed.iter().enumerate() {
            for w in pts.windows(2) {
                if (w[0].0 - w[1].0).abs() < 0.75 && (w[0].1 - w[1].1).abs() >= 16.0 {
                    samples.push((ei, (w[0].0 + w[1].0) * 0.5, (w[0].1 + w[1].1) * 0.5));
                    break;
                }
            }
        }
        assert!(
            samples.len() >= 2,
            "need two verticals to hit-test {samples:?}"
        );
        for (ei, x, y) in samples {
            assert_eq!(
                hit_edge_route(&view, x, y, true),
                Some(ei),
                "click on lane of edge {ei} at ({x},{y})"
            );
        }
    }

    #[test]
    fn file_path_ellipsis_keeps_the_file_name() {
        assert_eq!(
            ellipsize_label("examples/marina/vessels.jsonl", 28, true),
            "…/marina/vessels.jsonl"
        );
        assert_eq!(
            ellipsize_label("examples/marina/out/vessels.csv", 28, true),
            "…/marina/out/vessels.csv"
        );
        assert_eq!(
            ellipsize_label(r"examples\marina\owners.csv", 20, true),
            r"…\marina\owners.csv"
        );
        assert_eq!(
            ellipsize_label("examples/marina/vessels.jsonl", 80, true),
            "examples/marina/vessels.jsonl"
        );
        let tight = ellipsize_label("examples/marina/vessels.jsonl", 12, true);
        assert!(tight.starts_with('…'), "{tight}");
        assert!(tight.ends_with("jsonl"), "{tight}");
        assert_eq!(tight.chars().count(), 12);
    }

    #[test]
    fn argument_ellipsis_keeps_the_start() {
        let src = "json keep $vessel_id $name $type $berth_id $owner_id";
        let out = ellipsize_label(src, 28, false);
        assert_eq!(out.chars().count(), 28);
        assert!(out.ends_with('…'), "{out}");
        assert!(out.starts_with("json keep"), "{out}");
        assert!(!out.contains("jsonl"), "must not look like a cut file name");
    }

    #[test]
    fn marina_file_details_keep_the_extension() {
        let spec = "< examples/marina/vessels.jsonl | json keep $vessel_id $name $type $berth_id $owner_id | > examples/marina/out/vessels.csv";
        let view = sample_view(spec);
        let read = view
            .nodes
            .iter()
            .find(|n| n.verb == "<")
            .expect("read file stage");
        assert!(
            read.detail.contains("vessels.jsonl"),
            "hard-clipped to vessels.json: {}",
            read.detail
        );
        assert!(
            read.full_text.contains("vessels.jsonl"),
            "{}",
            read.full_text
        );
        assert!(read.keep_end);
        let write = view
            .nodes
            .iter()
            .find(|n| n.verb == ">")
            .expect("write file stage");
        assert!(write.detail.contains("vessels.csv"), "{}", write.detail);
        assert!(write.keep_end);
        let json = view
            .nodes
            .iter()
            .find(|n| n.verb.eq_ignore_ascii_case("json"))
            .expect("json");
        assert!(
            json.detail.contains("$owner_id") || json.full_text.contains("$owner_id"),
            "json args were clipped: detail={} full={}",
            json.detail,
            json.full_text
        );
        assert!(!json.keep_end);
        assert!(is_file_stage_verb("<"));
        assert!(is_file_stage_verb(">>"));
        assert!(is_file_stage_verb("DISK"));
        assert!(!is_file_stage_verb("json"));
    }

    #[test]
    fn locate_box_is_taller_than_literal_and_as_wide_as_its_name() {
        let view = sample_view("literal hello | locate bleh | console");
        let lit = view.nodes.iter().find(|n| n.verb == "LITERAL").unwrap();
        let loc = view.nodes.iter().find(|n| n.verb == "LOCATE").unwrap();
        assert!(
            loc.rect.h > lit.rect.h + 8.0,
            "locate h {} literal h {}",
            loc.rect.h,
            lit.rect.h
        );
        assert!(loc.rect.w + 0.1 >= min_node_width(&loc.title));
        assert!(loc.rect.w + 0.1 >= min_node_width("LOCATE"));
    }

    #[test]
    fn locate_connection_points_are_spread_apart() {
        let spec = "literal hello | locate bleh | console";
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        let mut view = layout(&topo, &vec![NodeFlags::default(); topo.nodes.len()]);
        attach_connection_points(&mut view, &|_, verb, output, stream| {
            pipe_stages::catalog_lookup(verb, &StageRegistry::default())
                .and_then(|e| pipe_stages::default_stream_word(e, output, stream))
                .unwrap_or("")
                .to_string()
        });
        let loc = topo.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let outs: Vec<_> = view
            .ports
            .iter()
            .filter(|p| p.node_index == loc && p.output)
            .collect();
        assert_eq!(outs.len(), 2);
        let dy = (outs[0].pos.1 - outs[1].pos.1).abs();
        assert!(dy >= PORT_GAP - 1.0, "port dy {dy}");
    }

    #[test]
    fn auto_layout_puts_unmatched_on_its_own_row_with_a_gap() {
        let spec =
            "(ENDCHAR ?) literal hello | literal bleh | L: locate bleh | console ? L: | hole";
        let view = sample_view(spec);
        let loc = view.nodes.iter().find(|n| n.verb == "LOCATE").unwrap();
        let cons = view.nodes.iter().find(|n| n.verb == "CONSOLE").unwrap();
        let hole = view.nodes.iter().find(|n| n.verb == "HOLE").unwrap();
        assert!(
            hole.rect.y >= loc.rect.y + loc.rect.h + GAP_Y - 1.0,
            "hole y {} locate bottom {}",
            hole.rect.y,
            loc.rect.y + loc.rect.h
        );
        assert!(
            cons.rect.x >= loc.rect.x + loc.rect.w + GAP_X - 1.0,
            "console x {} locate right {}",
            cons.rect.x,
            loc.rect.x + loc.rect.w
        );
        assert!(cons.rect.w + 0.1 >= min_node_width(&cons.title));
    }

    #[test]
    fn stream_caption_is_the_word_only() {
        assert_eq!(
            stream_caption(1, Some("L"), Some("unmatched")).as_deref(),
            Some("unmatched")
        );
        assert!(stream_caption(0, Some("L"), Some("matched")).is_none());
    }

    #[test]
    fn tidy_moves_a_same_row_secondary_and_opens_a_gap() {
        let spec = "(ENDCHAR ?) literal hello | L: locate bleh | console ? L: | hole";
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        let mut view = layout(&topo, &vec![NodeFlags::default(); topo.nodes.len()]);
        for (i, n) in view.nodes.iter_mut().enumerate() {
            n.id = i as u64 + 1;
        }
        let loc = view
            .nodes
            .iter()
            .find(|n| n.verb == "LOCATE")
            .unwrap()
            .clone();
        let hole_i = view.nodes.iter().position(|n| n.verb == "HOLE").unwrap();
        view.nodes[hole_i].rect.x = loc.rect.x + loc.rect.w + 4.0;
        view.nodes[hole_i].rect.y = loc.rect.y;
        let cons_i = view.nodes.iter().position(|n| n.verb == "CONSOLE").unwrap();
        view.nodes[cons_i].rect.x = loc.rect.x + loc.rect.w + 8.0;
        view.nodes[cons_i].rect.y = loc.rect.y;
        let mut positions = NodePositions::new();
        for n in &view.nodes {
            positions.insert(n.id, (n.rect.x, n.rect.y));
        }
        tidy_session_layout(&mut view, &mut positions);
        let loc = view.nodes.iter().find(|n| n.verb == "LOCATE").unwrap();
        let hole = view.nodes.iter().find(|n| n.verb == "HOLE").unwrap();
        let cons = view.nodes.iter().find(|n| n.verb == "CONSOLE").unwrap();
        assert!(
            hole.rect.y >= loc.rect.y + loc.rect.h + GAP_Y - 1.0,
            "hole y {} locate {}",
            hole.rect.y,
            loc.rect.y
        );
        assert!(
            cons.rect.x >= loc.rect.x + loc.rect.w + GAP_X - 1.0,
            "console x {} locate right {}",
            cons.rect.x,
            loc.rect.x + loc.rect.w
        );
    }

    fn sample_view_dir(spec: &str, dir: LayoutDirection) -> GraphView {
        let topo = inspect_topology(&parse_pipeline(spec).unwrap());
        layout_dir(&topo, &vec![NodeFlags::default(); topo.nodes.len()], dir)
    }

    fn assert_no_box_overlap(view: &GraphView) {
        for i in 0..view.nodes.len() {
            for j in i + 1..view.nodes.len() {
                assert!(
                    !boxes_overlap(&view.nodes[i].rect, &view.nodes[j].rect),
                    "{} overlaps {}",
                    view.nodes[i].title,
                    view.nodes[j].title
                );
            }
        }
    }

    #[test]
    fn vertical_layout_stacks_primary_and_puts_secondary_in_a_column() {
        let spec =
            "(ENDCHAR ?) literal hello | literal bleh | L: locate bleh | console ? L: | hole";
        let mut view = sample_view_dir(spec, LayoutDirection::Vertical);
        attach_connection_points(&mut view, &|_, verb, output, stream| {
            pipe_stages::catalog_lookup(verb, &StageRegistry::default())
                .and_then(|e| pipe_stages::default_stream_word(e, output, stream))
                .unwrap_or("")
                .to_string()
        });
        assert_no_box_overlap(&view);
        let loc = view.nodes.iter().find(|n| n.verb == "LOCATE").unwrap();
        let cons = view.nodes.iter().find(|n| n.verb == "CONSOLE").unwrap();
        let hole = view.nodes.iter().find(|n| n.verb == "HOLE").unwrap();
        let lit = view.nodes.iter().find(|n| n.verb == "LITERAL").unwrap();
        assert!(
            loc.rect.y >= lit.rect.y + lit.rect.h + GAP_Y - 1.0,
            "locate y {} literal bottom {}",
            loc.rect.y,
            lit.rect.y + lit.rect.h
        );
        assert!(
            cons.rect.y >= loc.rect.y + loc.rect.h + GAP_Y - 1.0,
            "console y {} locate bottom {}",
            cons.rect.y,
            loc.rect.y + loc.rect.h
        );
        assert!(
            hole.rect.x >= loc.rect.x + loc.rect.w + GAP_X - 1.0,
            "hole x {} locate right {}",
            hole.rect.x,
            loc.rect.x + loc.rect.w
        );
        assert!(
            (hole.rect.y - loc.rect.y).abs() < loc.rect.h + 1.0,
            "hole should sit beside locate, hole y {} locate y {}",
            hole.rect.y,
            loc.rect.y
        );
        assert!(loc.rect.w + 0.1 >= min_node_width(&loc.title));
        let loc_i = loc.index;
        let outs: Vec<_> = view
            .ports
            .iter()
            .filter(|p| p.node_index == loc_i && p.output)
            .collect();
        assert_eq!(outs.len(), 2);
        assert!(
            (outs[0].pos.1 - (loc.rect.y + loc.rect.h)).abs() < 0.5,
            "outputs sit on the bottom edge"
        );
        let dx = (outs[0].pos.0 - outs[1].pos.0).abs();
        assert!(dx >= 40.0, "vertical outputs spread along the bottom {dx}");
        let ins: Vec<_> = view
            .ports
            .iter()
            .filter(|p| p.node_index == loc_i && !p.output)
            .collect();
        assert!(
            ins.iter().all(|p| (p.pos.1 - loc.rect.y).abs() < 0.5),
            "inputs sit on the top edge"
        );
    }

    #[test]
    fn layout_parse_accepts_horizontal_and_vertical() {
        assert_eq!(
            LayoutDirection::parse("horizontal"),
            Some(LayoutDirection::Horizontal)
        );
        assert_eq!(
            LayoutDirection::parse("Vertical"),
            Some(LayoutDirection::Vertical)
        );
        assert_eq!(LayoutDirection::parse("sideways"), None);
        assert_eq!(LayoutDirection::default(), LayoutDirection::Horizontal);
    }
}
