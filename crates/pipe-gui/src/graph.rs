//! Graph layout from `inspect_topology`. No execution.

use pipe_stages::{NodeKind, PipelineTopology};

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

#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub index: usize,
    pub rect: Rect,
    pub title: String,
    pub detail: String,
    pub kind: NodeKind,
    pub unknown: bool,
    pub pipeline: u32,
    pub stage: u32,
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

const NODE_W: f32 = 150.0;
const NODE_H: f32 = 52.0;
const GAP_X: f32 = 44.0;
const GAP_Y: f32 = 84.0;
const MARGIN: f32 = 20.0;

pub fn layout(topo: &PipelineTopology, unknown: &[bool]) -> GraphView {
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
        let title = node_title(n.kind, &n.verb, n.label.as_deref());
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
            detail: format!("p{} s{}", n.pipeline, n.stage),
            kind: n.kind,
            unknown: unknown.get(i).copied().unwrap_or(false),
            pipeline: n.pipeline,
            stage: n.stage,
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

fn port_y(h: f32, stream: u32) -> f32 {
    match stream {
        0 => h * 0.5,
        1 => h * 0.28,
        _ => h * 0.72,
    }
}

fn node_title(kind: NodeKind, verb: &str, label: Option<&str>) -> String {
    match kind {
        NodeKind::LabelReference => label.unwrap_or("ref").to_string(),
        NodeKind::LabelDefinition => format!("{}:", label.unwrap_or("label")),
        NodeKind::Connector => verb.to_string(),
        NodeKind::Stage => match label {
            Some(l) => format!("{l}: {verb}"),
            None => verb.to_string(),
        },
    }
}

pub fn hit_node(view: &GraphView, x: f32, y: f32) -> Option<usize> {
    view.nodes
        .iter()
        .rev()
        .find(|n| n.rect.contains(x, y))
        .map(|n| n.index)
}

pub fn hit_edge(view: &GraphView, x: f32, y: f32) -> Option<usize> {
    let mut best = None;
    let mut best_d = 10.0f32;
    for (i, e) in view.edges.iter().enumerate() {
        let d = dist_to_segment(x, y, e.from, e.to);
        if d < best_d {
            best_d = d;
            best = Some(i);
        }
    }
    best
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
        let view = layout(&topo, &vec![false; topo.nodes.len()]);
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
    }
}
