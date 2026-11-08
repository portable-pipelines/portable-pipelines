//! View-only graph layout from `inspect_topology`. Drag offsets never rewrite the spec.

use std::collections::HashMap;
use std::path::Path;

use pipe_stages::{NodeKind, PipelineTopology, TopoNode};

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

#[derive(Debug, Clone)]
pub struct PlacedNode {
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

#[derive(Debug, Clone)]
pub struct GraphView {
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<PlacedEdge>,
    pub width: f32,
    pub height: f32,
}

const NODE_W: f32 = 164.0;
const NODE_H: f32 = 56.0;
const GAP_X: f32 = 92.0;
const GAP_Y: f32 = 108.0;
const MARGIN: f32 = 24.0;
const PORT_STUB: f32 = 18.0;
const BACK_EDGE_DROP: f32 = 36.0;

pub fn layout(topo: &PipelineTopology, flags: &[NodeFlags]) -> GraphView {
    let mut nodes = Vec::new();
    let mut col: Vec<f32> = Vec::new();
    let mut row_used: Vec<f32> = Vec::new();
    for (i, n) in topo.nodes.iter().enumerate() {
        let p = n.pipeline.saturating_sub(1) as usize;
        while col.len() <= p {
            col.push(0.0);
            row_used.push(0.0);
        }
        if n.kind == NodeKind::LabelReference {
            continue;
        }
        let flag = flags.get(i).copied().unwrap_or_default();
        let title = node_title(n.kind, &n.verb, n.label.as_deref(), flag.script);
        let x = MARGIN + col[p];
        let y = MARGIN + p as f32 * (NODE_H + GAP_Y);
        nodes.push(PlacedNode {
            index: i,
            rect: Rect {
                x,
                y,
                w: NODE_W,
                h: NODE_H,
            },
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
        });
        col[p] += NODE_W + GAP_X;
        row_used[p] = col[p];
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
        let from = (
            s.rect.x + s.rect.w,
            s.rect.y + port_y(s.rect.h, e.writer_stream),
        );
        let to = (d.rect.x, d.rect.y + port_y(d.rect.h, e.reader_stream));
        edges.push(PlacedEdge {
            writer: e.writer,
            reader: e.reader,
            writer_stream: e.writer_stream,
            reader_stream: e.reader_stream,
            from,
            to,
        });
    }
    let width = row_used.into_iter().fold(MARGIN, f32::max) + MARGIN;
    let height = if topo.nodes.is_empty() {
        200.0
    } else {
        let max_p = topo.nodes.iter().map(|n| n.pipeline).max().unwrap_or(1);
        MARGIN + max_p as f32 * (NODE_H + GAP_Y)
    };
    GraphView {
        nodes,
        edges,
        width: width.max(400.0),
        height: height.max(240.0),
    }
}

pub fn apply_nudges(view: &mut GraphView, nudges: &NodeNudges) {
    for n in &mut view.nodes {
        if let Some(&(dx, dy)) = nudges.get(&(n.pipeline, n.stage)) {
            n.rect.x += dx;
            n.rect.y += dy;
        }
    }
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
    for e in &mut view.edges {
        let Some(&si) = by_index.get(&e.writer) else {
            continue;
        };
        let Some(&di) = by_index.get(&e.reader) else {
            continue;
        };
        let s = &view.nodes[si];
        let d = &view.nodes[di];
        e.from = (
            s.rect.x + s.rect.w,
            s.rect.y + port_y(s.rect.h, e.writer_stream),
        );
        e.to = (d.rect.x, d.rect.y + port_y(d.rect.h, e.reader_stream));
    }
}

pub fn node_nudge_key(n: &PlacedNode) -> (u32, u32) {
    (n.pipeline, n.stage)
}

fn port_y(h: f32, stream: u32) -> f32 {
    match stream {
        0 => h * 0.5,
        1 => h * 0.22,
        _ => h * 0.78,
    }
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
            .chars()
            .take(28)
            .collect()
    }
}

fn node_hover_name(n: &TopoNode) -> String {
    match n.label.as_deref() {
        Some(l) if !n.verb.is_empty() => format!("{l}: {}", n.verb),
        Some(l) => l.to_string(),
        None => n.verb.clone(),
    }
}

/// Orthogonal elbows stay in the column gutter (writer stub, then vertical, then into the reader).
/// Direct mode is a single segment. Topology is unchanged.
pub fn edge_points(from: (f32, f32), to: (f32, f32), orthogonal: bool) -> Vec<(f32, f32)> {
    if !orthogonal {
        return vec![from, to];
    }
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

pub fn stream_caption(writer_stream: u32, reader_label: Option<&str>) -> Option<String> {
    match writer_stream {
        0 => None,
        1 => Some(
            reader_label
                .map(|l| format!("{l} (secondary)"))
                .unwrap_or_else(|| "secondary".into()),
        ),
        n => Some(
            reader_label
                .map(|l| format!("{l} (stream {n})"))
                .unwrap_or_else(|| format!("stream {n}")),
        ),
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

pub fn hit_edge(view: &GraphView, x: f32, y: f32) -> Option<usize> {
    hit_edge_route(view, x, y, true)
}

pub fn hit_edge_route(view: &GraphView, x: f32, y: f32, orthogonal: bool) -> Option<usize> {
    let mut best = None;
    let mut best_d = 10.0f32;
    for (i, e) in view.edges.iter().enumerate() {
        let pts = edge_points(e.from, e.to, orthogonal);
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
}
