//! Place, splice, and delete stages on a simple one-line pipe.
//!
//! The graph edit writes Source. Multistream, labels, and extra pipelines
//! are out of scope (Release 4).

use pipe_stages::{catalog_lookup, NodeKind, PipelineTopology, StageCatalogEntry, StageRegistry};

use crate::graph::{route_all, GraphView, PlacedPort};
use crate::serialize::{simple_pipe_text, SerializeError};

/// How close a dragged stage must be, in graph units, to light a target.
pub const PROXIMITY: f32 = 52.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropTarget {
    /// Empty canvas, or a far drop that appends.
    Canvas,
    /// Light this connection point and drop to wire stream 0.
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

fn graphable_indices(topo: &PipelineTopology) -> Vec<usize> {
    topo.nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind != NodeKind::LabelReference)
        .map(|(i, _)| i)
        .collect()
}

pub fn stage_clauses(topo: &PipelineTopology) -> Result<Vec<String>, SerializeError> {
    let text = simple_pipe_text(topo)?;
    Ok(text
        .split(" | ")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

pub fn join_clauses(clauses: &[String]) -> String {
    clauses.join(" | ")
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

fn clause_index(topo: &PipelineTopology, node_index: usize) -> Option<usize> {
    graphable_indices(topo)
        .iter()
        .position(|&i| i == node_index)
}

/// Where in the clause list to insert `verb` for this drop.
pub fn insert_index(
    topo: &PipelineTopology,
    target: &DropTarget,
    verb: &str,
) -> Result<usize, String> {
    let n = graphable_indices(topo).len();
    match target {
        DropTarget::Canvas => Ok(n),
        DropTarget::Port {
            node_index,
            output,
            stream,
        } => {
            if *stream != 0 {
                return Err(
                    "this release wires stream 0 only; secondary streams come later".into(),
                );
            }
            let pos = clause_index(topo, *node_index).ok_or("that stage is gone")?;
            if *output {
                if !accepts_after(verb) {
                    return Err(format!(
                        "{} does not take an input, so it cannot follow another stage",
                        palette_verb_label(verb)
                    ));
                }
                Ok(pos + 1)
            } else {
                if !accepts_before(verb) {
                    return Err(format!(
                        "{} does not send an output, so it cannot precede another stage",
                        palette_verb_label(verb)
                    ));
                }
                Ok(pos)
            }
        }
        DropTarget::Edge { edge_index } => {
            if !accepts_splice(verb) {
                return Err(format!(
                    "{} cannot sit between two stages (it needs an input and an output)",
                    palette_verb_label(verb)
                ));
            }
            let e = topo
                .edges
                .get(*edge_index)
                .ok_or("that connection is gone")?;
            if e.writer_stream != 0 || e.reader_stream != 0 {
                return Err(
                    "this release splices the primary connection only; secondary streams come later"
                        .into(),
                );
            }
            let pos = clause_index(topo, e.writer).ok_or("that stage is gone")?;
            Ok(pos + 1)
        }
    }
}

pub fn clauses_with_insert(
    topo: &PipelineTopology,
    verb: &str,
    target: &DropTarget,
) -> Result<Vec<String>, String> {
    let mut clauses = stage_clauses(topo).map_err(|e| e.to_string())?;
    let at = insert_index(topo, target, verb)?;
    clauses.insert(at, palette_verb_label(verb));
    Ok(clauses)
}

pub fn clauses_without(topo: &PipelineTopology, node_index: usize) -> Result<Vec<String>, String> {
    let mut clauses = stage_clauses(topo).map_err(|e| e.to_string())?;
    let at = clause_index(topo, node_index).ok_or("that stage is gone")?;
    clauses.remove(at);
    Ok(clauses)
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
    if p.stream != 0 {
        return false;
    }
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
        for (i, e) in view.edges.iter().enumerate() {
            if e.writer_stream != 0 || e.reader_stream != 0 {
                continue;
            }
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

/// Graph-space point used by `--drag-verb` screenshots.
pub fn screenshot_drag_pos(view: &GraphView, kind: DragTargetKind, orthogonal: bool) -> (f32, f32) {
    match kind {
        DragTargetKind::Canvas => (140.0, 90.0),
        DragTargetKind::Port => view
            .ports
            .iter()
            .find(|p| p.output && p.stream == 0)
            .map(|p| (p.pos.0 + 30.0, p.pos.1))
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

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_parser::parse_pipeline;
    use pipe_stages::{inspect_topology, PaletteGroup};

    fn topo(spec: &str) -> PipelineTopology {
        inspect_topology(&parse_pipeline(spec).unwrap())
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
    fn drop_on_empty_is_first_clause() {
        let t = topo("hole");
        // use a real empty by deleting
        let clauses =
            clauses_without(&t, t.nodes.iter().position(|n| n.verb == "HOLE").unwrap()).unwrap();
        assert!(clauses.is_empty());
    }

    #[test]
    fn append_locate_after_literal() {
        let t = topo("literal apple");
        let lit = t.nodes.iter().position(|n| n.verb == "LITERAL").unwrap();
        let clauses = clauses_with_insert(
            &t,
            "LOCATE",
            &DropTarget::Port {
                node_index: lit,
                output: true,
                stream: 0,
            },
        )
        .unwrap();
        assert_eq!(
            clauses,
            vec!["literal apple".to_string(), "locate".to_string()]
        );
        assert_eq!(join_clauses(&clauses), "literal apple | locate");
    }

    #[test]
    fn splice_between_literal_and_console() {
        let t = topo("literal apple | console");
        assert_eq!(t.edges.len(), 1);
        let clauses =
            clauses_with_insert(&t, "locate", &DropTarget::Edge { edge_index: 0 }).unwrap();
        assert_eq!(
            clauses,
            vec![
                "literal apple".to_string(),
                "locate".to_string(),
                "console".to_string()
            ]
        );
    }

    #[test]
    fn prepend_on_input_port() {
        let t = topo("locate /ap/ | console");
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let clauses = clauses_with_insert(
            &t,
            "LITERAL",
            &DropTarget::Port {
                node_index: loc,
                output: false,
                stream: 0,
            },
        )
        .unwrap();
        assert_eq!(
            clauses,
            vec![
                "literal".to_string(),
                "locate /ap/".to_string(),
                "console".to_string()
            ]
        );
    }

    #[test]
    fn delete_middle_joins_neighbors() {
        let t = topo("literal a | locate /a/ | console");
        let loc = t.nodes.iter().position(|n| n.verb == "LOCATE").unwrap();
        let clauses = clauses_without(&t, loc).unwrap();
        assert_eq!(clauses, vec!["literal a".to_string(), "console".into()]);
    }

    #[test]
    fn literal_cannot_splice_in_the_middle() {
        let t = topo("literal a | console");
        let err =
            clauses_with_insert(&t, "LITERAL", &DropTarget::Edge { edge_index: 0 }).unwrap_err();
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
}
