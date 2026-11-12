//! Place, splice, wire, and delete stages. The graph edit writes Source,
//! including labels and ENDCHAR when secondary streams are used.

use pipe_stages::{catalog_lookup, NodeKind, PipelineTopology, StageCatalogEntry, StageRegistry};

use crate::graph::{
    route_all, GraphView, LayoutDirection, PlacedPort, GAP_X, GAP_Y, NODE_H, NODE_W,
};
use crate::ids::NodeId;
use crate::serialize::{
    graph_from_topo, graphable, pipe_text, GraphEdge, GraphSpec, GraphStage, SerializeError,
};

/// How close a dragged stage must be, in graph units, to light a target.
pub const PROXIMITY: f32 = 52.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropTarget {
    /// Empty canvas, or a far drop that appends.
    Canvas,
    /// Light this connection point and drop to wire it.
    Port {
        node_index: usize,
        output: bool,
        stream: u32,
    },
    /// Light this connection and splice the new stage between its ends.
    Edge { edge_index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTargetKind {
    Port,
    Edge,
    Canvas,
}

impl DragTargetKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "port" | "point" | "connection-point" => Some(Self::Port),
            "edge" | "connection" | "splice" => Some(Self::Edge),
            "canvas" | "empty" => Some(Self::Canvas),
            _ => None,
        }
    }
}

/// One end of a connection-point drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireEnd {
    pub node_index: usize,
    pub output: bool,
    pub stream: u32,
}

/// Catalog rows grouped in palette order. User buckets are present and empty.
pub fn grouped_palette() -> Vec<(pipe_stages::PaletteGroup, Vec<&'static StageCatalogEntry>)> {
    pipe_stages::PaletteGroup::ALL
        .iter()
        .copied()
        .map(|g| {
            let rows: Vec<_> = pipe_stages::all_catalog_entries()
                .iter()
                .filter(|e| e.group == g)
                .collect();
            (g, rows)
        })
        .collect()
}

pub fn palette_verb_label(name: &str) -> String {
    name.to_ascii_lowercase()
}

pub fn column_pitch() -> f32 {
    NODE_W + GAP_X
}

pub fn lane_pitch(dir: LayoutDirection) -> f32 {
    if dir.is_vertical() {
        NODE_H + GAP_Y
    } else {
        NODE_W + GAP_X
    }
}

pub fn can_take_input(entry: &StageCatalogEntry) -> bool {
    entry.inputs.max != Some(0)
}

pub fn can_give_output(entry: &StageCatalogEntry) -> bool {
    entry.outputs.max != Some(0)
}

fn entry_for(verb: &str) -> Option<&'static StageCatalogEntry> {
    catalog_lookup(verb, &StageRegistry::default())
}

pub fn accepts_splice(verb: &str) -> bool {
    match entry_for(verb) {
        Some(e) => can_take_input(e) && can_give_output(e),
        None => true,
    }
}

pub fn accepts_after(verb: &str) -> bool {
    entry_for(verb).map(can_take_input).unwrap_or(true)
}

pub fn accepts_before(verb: &str) -> bool {
    entry_for(verb).map(can_give_output).unwrap_or(true)
}

fn graph_index(topo: &PipelineTopology, topo_idx: usize) -> Option<usize> {
    graphable(topo).iter().position(|&i| i == topo_idx)
}

fn add_stage(g: &mut GraphSpec, verb: &str) -> usize {
    g.stages.push(GraphStage {
        verb: verb.to_ascii_uppercase(),
        running: palette_verb_label(verb),
        label: None,
        kind: NodeKind::Stage,
    });
    g.stages.len() - 1
}

fn port_taken_out(g: &GraphSpec, writer: usize, stream: u32) -> Option<usize> {
    g.edges
        .iter()
        .position(|e| e.writer == writer && e.writer_stream == stream)
}

fn port_taken_in(g: &GraphSpec, reader: usize, stream: u32) -> Option<usize> {
    g.edges
        .iter()
        .position(|e| e.reader == reader && e.reader_stream == stream)
}

pub fn connect_ports(
    g: &mut GraphSpec,
    w: usize,
    ws: u32,
    r: usize,
    rs: u32,
) -> Result<(), String> {
    if w == r {
        return Err("a stage cannot connect to itself".into());
    }
    if w >= g.stages.len() || r >= g.stages.len() {
        return Err("that stage is gone".into());
    }
    g.edges.retain(|e| {
        !(e.writer == w && e.writer_stream == ws) && !(e.reader == r && e.reader_stream == rs)
    });
    g.edges.push(GraphEdge {
        writer: w,
        writer_stream: ws,
        reader: r,
        reader_stream: rs,
    });
    Ok(())
}

pub fn disconnect_port(g: &mut GraphSpec, node: usize, output: bool, stream: u32) {
    g.edges.retain(|e| {
        if output {
            !(e.writer == node && e.writer_stream == stream)
        } else {
            !(e.reader == node && e.reader_stream == stream)
        }
    });
}

/// Remove a stage. If it has exactly one incoming connection and one outgoing
/// connection, those two ends are wired together.
pub fn remove_stage(g: &mut GraphSpec, idx: usize) -> Result<(), String> {
    if idx >= g.stages.len() {
        return Err("that stage is gone".into());
    }
    let ins: Vec<_> = g
        .edges
        .iter()
        .filter(|e| e.reader == idx)
        .cloned()
        .collect();
    let outs: Vec<_> = g
        .edges
        .iter()
        .filter(|e| e.writer == idx)
        .cloned()
        .collect();
    g.edges.retain(|e| e.writer != idx && e.reader != idx);
    if ins.len() == 1 && outs.len() == 1 {
        let i = &ins[0];
        let o = &outs[0];
        if i.writer != o.reader {
            g.edges.push(GraphEdge {
                writer: i.writer,
                writer_stream: i.writer_stream,
                reader: o.reader,
                reader_stream: o.reader_stream,
            });
        }
    }
    g.stages.remove(idx);
    for e in &mut g.edges {
        if e.writer > idx {
            e.writer -= 1;
        }
        if e.reader > idx {
            e.reader -= 1;
        }
    }
    Ok(())
}

fn main_chain_end(g: &GraphSpec) -> Option<usize> {
    if g.stages.is_empty() {
        return None;
    }
    let n = g.stages.len();
    let mut succ = vec![None; n];
    let mut pred = vec![false; n];
    for e in &g.edges {
        if e.writer_stream == 0 && e.reader_stream == 0 && succ[e.writer].is_none() {
            succ[e.writer] = Some(e.reader);
            pred[e.reader] = true;
        }
    }
    let mut best: Option<(usize, usize)> = None;
    for i in 0..n {
        if pred[i] {
            continue;
        }
        let mut len = 1;
        let mut last = i;
        let mut cur = succ[i];
        let mut guard = 0;
        while let Some(c) = cur {
            last = c;
            len += 1;
            cur = succ[c];
            guard += 1;
            if guard > n {
                break;
            }
        }
        let better = match best {
            None => true,
            Some((bl, bi)) => len > bl || (len == bl && i < bi),
        };
        if better {
            best = Some((len, last));
        }
    }
    best.map(|(_, last)| last).or_else(|| Some(n - 1))
}

fn splice_on_edge(g: &mut GraphSpec, ei: usize, new: usize) -> Result<(), String> {
    if ei >= g.edges.len() {
        return Err("that connection is gone".into());
    }
    let e = g.edges.remove(ei);
    connect_ports(g, e.writer, e.writer_stream, new, 0)?;
    connect_ports(g, new, 0, e.reader, e.reader_stream)?;
    Ok(())
}

/// Insert `verb` at `target`. Returns the new stage's index in the graph spec.
pub fn place_on_graph(
    topo: &PipelineTopology,
    verb: &str,
    target: &DropTarget,
) -> Result<(GraphSpec, usize), String> {
    let mut g = graph_from_topo(topo).map_err(|e| e.to_string())?;
    place_on_spec(&mut g, topo, verb, target)?;
    let new = g.stages.len() - 1;
    Ok((g, new))
}

fn place_on_spec(
    g: &mut GraphSpec,
    topo: &PipelineTopology,
    verb: &str,
    target: &DropTarget,
) -> Result<(), String> {
    match target {
        DropTarget::Canvas => {
            let new = add_stage(g, verb);
            if new == 0 {
                return Ok(());
            }
            if let Some(end) = main_chain_end(g) {
                if end != new {
                    let _ = connect_ports(g, end, 0, new, 0);
                }
            }
            Ok(())
        }
        DropTarget::Port {
            node_index,
            output,
            stream,
        } => {
            let gi = graph_index(topo, *node_index).ok_or("that stage is gone")?;
            if *output {
                if !accepts_after(verb) {
                    return Err(format!(
                        "{} does not take an input, so it cannot follow another stage",
                        palette_verb_label(verb)
                    ));
                }
                if let Some(ei) = port_taken_out(g, gi, *stream) {
                    if !accepts_splice(verb) {
                        return Err(format!(
                            "{} cannot sit between two stages (it needs an input and an output)",
                            palette_verb_label(verb)
                        ));
                    }
                    let new = add_stage(g, verb);
                    splice_on_edge(g, ei, new)?;
                } else {
                    let new = add_stage(g, verb);
                    connect_ports(g, gi, *stream, new, 0)?;
                }
            } else {
                if !accepts_before(verb) {
                    return Err(format!(
                        "{} does not send an output, so it cannot precede another stage",
                        palette_verb_label(verb)
                    ));
                }
                if let Some(ei) = port_taken_in(g, gi, *stream) {
                    if !accepts_splice(verb) {
                        return Err(format!(
                            "{} cannot sit between two stages (it needs an input and an output)",
                            palette_verb_label(verb)
                        ));
                    }
                    let new = add_stage(g, verb);
                    splice_on_edge(g, ei, new)?;
                } else {
                    let new = add_stage(g, verb);
                    connect_ports(g, new, 0, gi, *stream)?;
                }
            }
            Ok(())
        }
        DropTarget::Edge { edge_index } => {
            if !accepts_splice(verb) {
                return Err(format!(
                    "{} cannot sit between two stages (it needs an input and an output)",
                    palette_verb_label(verb)
                ));
            }
            let view_edge = topo
                .edges
                .get(*edge_index)
                .ok_or("that connection is gone")?;
            let w = graph_index(topo, view_edge.writer).ok_or("that stage is gone")?;
            let r = graph_index(topo, view_edge.reader).ok_or("that stage is gone")?;
            let ei = g
                .edges
                .iter()
                .position(|e| {
                    e.writer == w
                        && e.reader == r
                        && e.writer_stream == view_edge.writer_stream
                        && e.reader_stream == view_edge.reader_stream
                })
                .ok_or("that connection is gone")?;
            let new = add_stage(g, verb);
            splice_on_edge(g, ei, new)?;
            Ok(())
        }
    }
}

pub fn delete_from_graph(topo: &PipelineTopology, node_index: usize) -> Result<GraphSpec, String> {
    let mut g = graph_from_topo(topo).map_err(|e| e.to_string())?;
    let gi = graph_index(topo, node_index).ok_or("that stage is gone")?;
    remove_stage(&mut g, gi)?;
    Ok(g)
}

pub fn wire_graph(
    topo: &PipelineTopology,
    from: WireEnd,
    to: WireEnd,
) -> Result<GraphSpec, String> {
    if from.output == to.output {
        return Err("connect an output to an input".into());
    }
    let (out, inn) = if from.output { (from, to) } else { (to, from) };
    let mut g = graph_from_topo(topo).map_err(|e| e.to_string())?;
    let w = graph_index(topo, out.node_index).ok_or("that stage is gone")?;
    let r = graph_index(topo, inn.node_index).ok_or("that stage is gone")?;
    connect_ports(&mut g, w, out.stream, r, inn.stream)?;
    Ok(g)
}

pub fn unwire_port(
    topo: &PipelineTopology,
    node_index: usize,
    output: bool,
    stream: u32,
) -> Result<GraphSpec, String> {
    let mut g = graph_from_topo(topo).map_err(|e| e.to_string())?;
    let gi = graph_index(topo, node_index).ok_or("that stage is gone")?;
    disconnect_port(&mut g, gi, output, stream);
    Ok(g)
}

pub fn unwire_edge(topo: &PipelineTopology, edge_index: usize) -> Result<GraphSpec, String> {
    let mut g = graph_from_topo(topo).map_err(|e| e.to_string())?;
    let e = topo
        .edges
        .get(edge_index)
        .ok_or("that connection is gone")?;
    let w = graph_index(topo, e.writer).ok_or("that stage is gone")?;
    let r = graph_index(topo, e.reader).ok_or("that stage is gone")?;
    g.edges.retain(|ge| {
        !(ge.writer == w
            && ge.reader == r
            && ge.writer_stream == e.writer_stream
            && ge.reader_stream == e.reader_stream)
    });
    Ok(g)
}

pub fn spec_text(g: &GraphSpec) -> Result<String, String> {
    if g.stages.is_empty() {
        return Ok(String::new());
    }
    pipe_text(g).map_err(|e: SerializeError| e.to_string())
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn dist_to_segment(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (ax, ay) = a;
    let (bx, by) = b;
    let vx = bx - ax;
    let vy = by - ay;
    let len2 = vx * vx + vy * vy;
    if len2 < f32::EPSILON {
        return (px - ax).hypot(py - ay);
    }
    let t = ((px - ax) * vx + (py - ay) * vy) / len2;
    let t = t.clamp(0.0, 1.0);
    let qx = ax + t * vx;
    let qy = ay + t * vy;
    (px - qx).hypot(py - qy)
}

fn port_ok_for_verb(p: &PlacedPort, verb: &str) -> bool {
    if p.output {
        accepts_after(verb)
    } else {
        accepts_before(verb)
    }
}

/// Nearest lit drop target for a dragged `verb` at graph coordinates.
pub fn nearest_target(
    view: &GraphView,
    x: f32,
    y: f32,
    verb: &str,
    orthogonal: bool,
) -> DropTarget {
    let mut best_port: Option<(f32, DropTarget)> = None;
    for p in &view.ports {
        if !port_ok_for_verb(p, verb) {
            continue;
        }
        let d = dist((x, y), p.pos);
        if d <= PROXIMITY {
            let better = best_port.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true);
            if better {
                best_port = Some((
                    d,
                    DropTarget::Port {
                        node_index: p.node_index,
                        output: p.output,
                        stream: p.stream,
                    },
                ));
            }
        }
    }
    let mut best_edge: Option<(f32, DropTarget)> = None;
    if accepts_splice(verb) {
        let routes = route_all(view, orthogonal);
        for (i, _e) in view.edges.iter().enumerate() {
            let Some(pts) = routes.get(i) else {
                continue;
            };
            let mut d = f32::MAX;
            for w in pts.windows(2) {
                d = d.min(dist_to_segment(x, y, w[0], w[1]));
            }
            if d <= PROXIMITY {
                let better = best_edge.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true);
                if better {
                    best_edge = Some((d, DropTarget::Edge { edge_index: i }));
                }
            }
        }
    }
    match (best_port, best_edge) {
        (Some((pd, p)), Some((ed, e))) => {
            if ed + 4.0 < pd {
                e
            } else {
                p
            }
        }
        (Some((_, p)), None) => p,
        (None, Some((_, e))) => e,
        (None, None) => DropTarget::Canvas,
    }
}

pub fn nearest_wire_target(view: &GraphView, x: f32, y: f32, from: WireEnd) -> Option<WireEnd> {
    let mut best: Option<(f32, WireEnd)> = None;
    for p in &view.ports {
        if p.output == from.output {
            continue;
        }
        if p.node_index == from.node_index {
            continue;
        }
        let d = dist((x, y), p.pos);
        if d <= PROXIMITY {
            let better = best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true);
            if better {
                best = Some((
                    d,
                    WireEnd {
                        node_index: p.node_index,
                        output: p.output,
                        stream: p.stream,
                    },
                ));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Graph-space point used by `--drag-verb` screenshots.
pub fn screenshot_drag_pos(view: &GraphView, kind: DragTargetKind, orthogonal: bool) -> (f32, f32) {
    match kind {
        DragTargetKind::Canvas => (140.0, 90.0),
        DragTargetKind::Port => view
            .ports
            .iter()
            .find(|p| p.output && p.stream == 0)
            .map(|p| {
                if view.direction.is_vertical() {
                    (p.pos.0, p.pos.1 + 30.0)
                } else {
                    (p.pos.0 + 30.0, p.pos.1)
                }
            })
            .unwrap_or((140.0, 90.0)),
        DragTargetKind::Edge => view
            .edges
            .first()
            .and_then(|e| {
                let routes = route_all(view, orthogonal);
                let pts = routes.first()?;
                if pts.len() >= 2 {
                    let a = pts[pts.len() / 2 - 1];
                    let b = pts[pts.len() / 2];
                    Some(((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5))
                } else {
                    Some(((e.from.0 + e.to.0) * 0.5, (e.from.1 + e.to.1) * 0.5))
                }
            })
            .unwrap_or((180.0, 90.0)),
    }
}

pub fn screenshot_wire_pos(view: &GraphView, from: WireEnd, to: Option<WireEnd>) -> (f32, f32) {
    let start = view.ports.iter().find(|p| {
        p.node_index == from.node_index && p.output == from.output && p.stream == from.stream
    });
    let dest = to.and_then(|t| {
        view.ports
            .iter()
            .find(|p| p.node_index == t.node_index && p.output == t.output && p.stream == t.stream)
    });
    match (start, dest) {
        (Some(a), Some(b)) => ((a.pos.0 + b.pos.0) * 0.5, (a.pos.1 + b.pos.1) * 0.5),
        (Some(a), None) => (a.pos.0 + 40.0, a.pos.1),
        _ => (180.0, 90.0),
    }
}

/// Shift boxes on the same row whose left edge is at or past `min_x`.
pub fn shift_row(
    positions: &mut std::collections::HashMap<NodeId, (f32, f32)>,
    view: &GraphView,
    min_x: f32,
    row_y: f32,
    dx: f32,
    skip: Option<NodeId>,
) {
    shift_lane(
        positions,
        view,
        min_x,
        row_y,
        dx,
        skip,
        LayoutDirection::Horizontal,
    )
}

/// Shift boxes on the same lane (row or column) from `min_along` onward.
pub fn shift_lane(
    positions: &mut std::collections::HashMap<NodeId, (f32, f32)>,
    view: &GraphView,
    min_along: f32,
    lane: f32,
    delta: f32,
    skip: Option<NodeId>,
    dir: LayoutDirection,
) {
    let vertical = dir.is_vertical();
    for n in &view.nodes {
        if skip == Some(n.id) {
            continue;
        }
        if vertical {
            if (n.rect.x - lane).abs() > NODE_W * 0.75 {
                continue;
            }
            if n.rect.y + 4.0 < min_along {
                continue;
            }
            if let Some(p) = positions.get_mut(&n.id) {
                p.1 += delta;
            }
        } else {
            if (n.rect.y - lane).abs() > NODE_H * 0.75 {
                continue;
            }
            if n.rect.x + 4.0 < min_along {
                continue;
            }
            if let Some(p) = positions.get_mut(&n.id) {
                p.0 += delta;
            }
        }
    }
}

pub fn parse_wire_end(s: &str) -> Option<(String, bool, u32)> {
    // locate:out:1  or locate:in:0
    let mut parts = s.split(':');
    let verb = parts.next()?.trim();
    if verb.is_empty() {
        return None;
    }
    let dir = parts.next()?.trim().to_ascii_lowercase();
    let output = match dir.as_str() {
        "out" | "output" => true,
        "in" | "input" => false,
        _ => return None,
    };
    let stream = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((verb.to_string(), output, stream))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_parser::parse_pipeline;
    use pipe_stages::{inspect_topology, PaletteGroup};

    fn topo(spec: &str) -> PipelineTopology {
        inspect_topology(&parse_pipeline(spec).unwrap())
    }

    fn text_after_place(spec: &str, verb: &str, target: DropTarget) -> String {
        let t = topo(spec);
        let (g, _) = place_on_graph(&t, verb, &target).unwrap();
        spec_text(&g).unwrap()
    }

    #[test]
    fn six_palette_groups_and_user_buckets_empty() {
        let groups = grouped_palette();
        assert_eq!(groups.len(), 6);
        assert_eq!(groups[0].0, PaletteGroup::DeviceDrivers);
        assert_eq!(groups[1].0, PaletteGroup::Selection);
        assert_eq!(groups[2].0, PaletteGroup::FiltersAndReformatting);
        assert_eq!(groups[3].0, PaletteGroup::Multistream);
        assert_eq!(groups[4].0, PaletteGroup::MyRexxAndPython);
        assert_eq!(groups[5].0, PaletteGroup::MyPipes);
        assert!(groups[0].1.iter().any(|e| e.name == "LITERAL"));
        assert!(groups[1].1.iter().any(|e| e.name == "LOCATE"));
        assert!(groups[4].1.is_empty());
        assert!(groups[5].1.is_empty());
        for (g, rows) in &groups {
            assert_eq!(g.label().contains("lane"), false);
            for e in rows {
                let blob = format!("{} {}", e.help, e.syntax).to_ascii_lowercase();
                assert!(!blob.contains("debugger"), "{}", e.name);
                assert!(!blob.contains("ibm"), "{}", e.name);
            }
        }
    }

    #[test]
    fn append_locate_after_literal() {
        let t = topo("literal apple");
        let lit = t.nodes.iter().position(|n| n.verb == "LITERAL").unwrap();
        let (g, _) = place_on_graph(
            &t,
            "LOCATE",
            &DropTarget::Port {
                node_index: lit,
                output: true,
                stream: 0,
            },
        )
        .unwrap();
        assert_eq!(spec_text(&g).unwrap(), "literal apple | locate");
    }

    #[test]
    fn splice_between_literal_and_console() {
        let t = topo("literal apple | console");
        assert_eq!(t.edges.len(), 1);
        let (g, _) = place_on_graph(&t, "locate", &DropTarget::Edge { edge_index: 0 }).unwrap();
        assert_eq!(spec_text(&g).unwrap(), "literal apple | locate | console");
    }

    #[test]
    fn prepend_on_input_port() {
        let t = topo("locate /ap/ | console");
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let (g, _) = place_on_graph(
            &t,
            "LITERAL",
            &DropTarget::Port {
                node_index: loc,
                output: false,
                stream: 0,
            },
        )
        .unwrap();
        assert_eq!(spec_text(&g).unwrap(), "literal | locate /ap/ | console");
    }

    #[test]
    fn delete_middle_joins_neighbors() {
        let t = topo("literal a | locate /a/ | console");
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let g = delete_from_graph(&t, loc).unwrap();
        assert_eq!(spec_text(&g).unwrap(), "literal a | console");
    }

    #[test]
    fn literal_cannot_splice_in_the_middle() {
        let t = topo("literal a | console");
        let err = place_on_graph(&t, "LITERAL", &DropTarget::Edge { edge_index: 0 }).unwrap_err();
        assert!(err.contains("between"), "{err}");
    }

    #[test]
    fn nearest_port_lights_up_beside_literal() {
        use crate::graph::{attach_connection_points, layout};
        let t = topo("literal apple");
        let flags = vec![crate::graph::NodeFlags::default(); t.nodes.len()];
        let mut view = layout(&t, &flags);
        attach_connection_points(&mut view, &|_, _, _, _| String::new());
        let lit = t.nodes.iter().position(|n| n.verb == "LITERAL").unwrap();
        let out = view
            .ports
            .iter()
            .find(|p| p.node_index == lit && p.output && p.stream == 0)
            .expect("output");
        let target = nearest_target(&view, out.pos.0 + 20.0, out.pos.1, "LOCATE", true);
        assert_eq!(
            target,
            DropTarget::Port {
                node_index: lit,
                output: true,
                stream: 0
            }
        );
    }

    #[test]
    fn nearest_edge_lights_up_for_splice() {
        use crate::graph::{attach_connection_points, layout};
        let t = topo("literal apple | console");
        let flags = vec![crate::graph::NodeFlags::default(); t.nodes.len()];
        let mut view = layout(&t, &flags);
        attach_connection_points(&mut view, &|_, _, _, _| String::new());
        let e = &view.edges[0];
        let mx = (e.from.0 + e.to.0) * 0.5;
        let my = (e.from.1 + e.to.1) * 0.5;
        let target = nearest_target(&view, mx, my, "LOCATE", true);
        assert_eq!(target, DropTarget::Edge { edge_index: 0 });
    }

    #[test]
    fn far_from_stages_is_a_canvas_drop() {
        use crate::graph::{attach_connection_points, layout};
        let t = topo("literal apple");
        let flags = vec![crate::graph::NodeFlags::default(); t.nodes.len()];
        let mut view = layout(&t, &flags);
        attach_connection_points(&mut view, &|_, _, _, _| String::new());
        let target = nearest_target(&view, 800.0, 600.0, "LOCATE", true);
        assert_eq!(target, DropTarget::Canvas);
    }

    #[test]
    fn drop_on_unmatched_writes_endchar() {
        let t = topo("literal apple | locate /ap/ | console");
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let (g, _) = place_on_graph(
            &t,
            "HOLE",
            &DropTarget::Port {
                node_index: loc,
                output: true,
                stream: 1,
            },
        )
        .unwrap();
        let text = spec_text(&g).unwrap();
        assert!(text.contains("ENDCHAR"), "{text}");
        assert!(text.contains("locate"), "{text}");
        assert!(text.contains("hole"), "{text}");
        let t2 = inspect_topology(&parse_pipeline(&text).unwrap());
        assert!(t2
            .edges
            .iter()
            .any(|e| e.writer_stream == 1 && t2.nodes[e.reader].verb == "HOLE"));
    }

    #[test]
    fn wire_locate_unmatched_to_hole() {
        let spec = "(ENDCHAR ?) literal apple | locate /ap/ | console ? hole";
        let t = topo(spec);
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let hole = t.nodes.iter().position(|n| n.verb == "HOLE").unwrap();
        let g = wire_graph(
            &t,
            WireEnd {
                node_index: loc,
                output: true,
                stream: 1,
            },
            WireEnd {
                node_index: hole,
                output: false,
                stream: 0,
            },
        )
        .unwrap();
        let text = spec_text(&g).unwrap();
        assert!(text.contains("ENDCHAR"), "{text}");
        let t2 = inspect_topology(&parse_pipeline(&text).unwrap());
        assert!(t2.edges.iter().any(|e| {
            e.writer_stream == 1
                && t2.nodes[e.writer].verb == "LOCATE"
                && t2.nodes[e.reader].verb == "HOLE"
        }));
    }

    #[test]
    fn disconnect_unmatched_drops_the_secondary_pipeline() {
        let spec = "(ENDCHAR ?) literal apple | L: locate /ap/ | console ? L: | hole";
        let t = topo(spec);
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let g = unwire_port(&t, loc, true, 1).unwrap();
        let text = spec_text(&g).unwrap();
        assert!(text.contains("locate"), "{text}");
        assert!(text.contains("hole"), "{text}");
        let t2 = inspect_topology(&parse_pipeline(&text).unwrap());
        assert!(!t2.edges.iter().any(|e| e.writer_stream == 1), "{text}");
    }

    #[test]
    fn delete_reconnects_when_one_in_one_out() {
        let spec = "(ENDCHAR ?) literal apple | L: locate /ap/ | console ? L: | hole";
        let t = topo(spec);
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let g = delete_from_graph(&t, loc).unwrap();
        let text = spec_text(&g).unwrap();
        // locate had primary in+out (unambiguous) plus unmatched — 2 outs, so no join.
        assert!(text.contains("literal apple"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("locate"), "{text}");
    }

    #[test]
    fn delete_filter_on_a_one_line_pipe_joins() {
        let t = topo("literal a | spec 1. | console");
        let spec = t.nodes.iter().position(|n| n.verb == "SPEC").unwrap();
        let g = delete_from_graph(&t, spec).unwrap();
        assert_eq!(spec_text(&g).unwrap(), "literal a | console");
    }

    #[test]
    fn parse_wire_end_reads_out_and_in() {
        assert_eq!(
            parse_wire_end("locate:out:1"),
            Some(("locate".into(), true, 1))
        );
        assert_eq!(parse_wire_end("hole:in:0"), Some(("hole".into(), false, 0)));
        assert!(parse_wire_end("lane:out:0").is_some());
        assert!(parse_wire_end("debugger").is_none());
    }

    #[test]
    fn canvas_drop_still_appends() {
        let text = text_after_place("literal apple", "LOCATE", DropTarget::Canvas);
        assert_eq!(text, "literal apple | locate");
    }
}
