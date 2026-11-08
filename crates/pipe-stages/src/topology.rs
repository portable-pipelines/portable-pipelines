//! Static pipeline topology from a parsed specification.
//!
//! This module does not look up stages, does not create `StageFn` closures,
//! and does not run the dispatcher. Unknown verbs still appear as nodes.

use std::collections::HashMap;

use pipe_core::StreamId;
use pipe_parser::{ParsedPipeline, ParsedStage, SourceSpan};

/// How a topology node relates to the specification text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A stage verb (including `.rexx` / `.py` paths).
    Stage,
    /// Label definition with no verb (`| j: |`). Runtime uses COPY; the graph does not.
    LabelDefinition,
    /// Later label occurrence with no verb (`? L: | hole`). Stream attachment only.
    LabelReference,
    /// `*:` / `*.input` / `*.output` connector. Top-level run still rejects these.
    Connector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopoNode {
    /// 1-based pipeline index (CMS numbering).
    pub pipeline: u32,
    /// 1-based stage slot in that pipeline, including label-only slots.
    pub stage: u32,
    pub kind: NodeKind,
    pub label: Option<String>,
    pub label_occurrence: u32,
    /// Empty for label-only reference/definition.
    pub verb: String,
    pub running: String,
    pub span: SourceSpan,
    pub verb_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopoEdge {
    pub writer: usize,
    pub writer_stream: u32,
    pub reader: usize,
    pub reader_stream: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineTopology {
    pub nodes: Vec<TopoNode>,
    pub edges: Vec<TopoEdge>,
}

/// True when the verb is a language-stage path (`*.py`, `*.rexx`, `*.rex`) or
/// the `REXX` keyword. Inspection-only: does not look at the host filesystem.
pub fn is_language_stage_verb(verb: &str) -> bool {
    let lower = verb.to_ascii_lowercase();
    lower.ends_with(".py")
        || lower.ends_with(".rexx")
        || lower.ends_with(".rex")
        || verb.eq_ignore_ascii_case("REXX")
}

/// Inspect stages and connections without instantiating or running them.
///
/// Unknown verbs remain nodes. Label references are not copied into COPY
/// stages. CALLPIPE connectors appear as [`NodeKind::Connector`]. RUNPIPE
/// is one node; nested PIPE text in records is not a subgraph.
pub fn inspect_topology(parsed: &ParsedPipeline) -> PipelineTopology {
    let mut nodes = Vec::new();
    // (pipeline, slot) -> node index
    let mut at: Vec<Vec<usize>> = Vec::new();
    for (pi, pipe) in parsed.pipelines.iter().enumerate() {
        let mut row = Vec::new();
        for (si, stage) in pipe.iter().enumerate() {
            let idx = nodes.len();
            nodes.push(node_from_stage(pi, si, stage));
            row.push(idx);
        }
        at.push(row);
    }

    let mut label_defs: HashMap<String, Vec<LabelRef>> = HashMap::new();
    for (pi, pipe) in parsed.pipelines.iter().enumerate() {
        for (si, stage) in pipe.iter().enumerate() {
            if let Some(lbl) = &stage.label {
                let runtime = match nodes[at[pi][si]].kind {
                    NodeKind::LabelReference => None,
                    _ => Some(at[pi][si]),
                };
                label_defs.entry(lbl.clone()).or_default().push(LabelRef {
                    pipeline: pi,
                    index: si,
                    node: runtime,
                    occurrence: stage.label_occurrence,
                });
            }
        }
    }

    let mut edges = Vec::new();

    for (pi, pipe) in parsed.pipelines.iter().enumerate() {
        let mut prev: Option<usize> = None;
        for (si, stage) in pipe.iter().enumerate() {
            let ni = at[pi][si];
            match nodes[ni].kind {
                NodeKind::LabelReference => {
                    prev = None;
                }
                _ => {
                    if let Some(pw) = prev {
                        let in_stream = stream_for_occurrence(stage.label_occurrence);
                        edges.push(TopoEdge {
                            writer: pw,
                            writer_stream: StreamId::PRIMARY.0,
                            reader: ni,
                            reader_stream: in_stream.0,
                        });
                    }
                    prev = Some(ni);
                }
            }
        }
    }

    let mut label_only: HashMap<(usize, usize), (usize, StreamId)> = HashMap::new();
    for refs in label_defs.values() {
        let Some(def) = refs.iter().find(|r| r.occurrence == 0) else {
            continue;
        };
        let Some(def_node) = def.node else {
            continue;
        };
        for r in refs.iter().filter(|r| r.occurrence > 0 && r.node.is_none()) {
            label_only.insert(
                (r.pipeline, r.index),
                (def_node, stream_for_occurrence(r.occurrence)),
            );
        }
    }

    for refs in label_defs.values() {
        if refs.len() < 2 {
            continue;
        }
        let Some(def) = refs.iter().find(|r| r.occurrence == 0) else {
            continue;
        };
        let Some(def_node) = def.node else {
            continue;
        };
        for r in refs.iter().filter(|r| r.occurrence > 0) {
            let stream = stream_for_occurrence(r.occurrence);
            if let Some(ref_node) = r.node {
                edges.push(TopoEdge {
                    writer: def_node,
                    writer_stream: stream.0,
                    reader: ref_node,
                    reader_stream: StreamId::PRIMARY.0,
                });
            } else {
                let ids = &at[r.pipeline];
                if r.index + 1 < ids.len() {
                    let next = ids[r.index + 1];
                    if !matches!(nodes[next].kind, NodeKind::LabelReference) {
                        edges.push(TopoEdge {
                            writer: def_node,
                            writer_stream: stream.0,
                            reader: next,
                            reader_stream: StreamId::PRIMARY.0,
                        });
                    } else if let Some(&(next_id, next_stream)) =
                        label_only.get(&(r.pipeline, r.index + 1))
                    {
                        edges.push(TopoEdge {
                            writer: def_node,
                            writer_stream: stream.0,
                            reader: next_id,
                            reader_stream: next_stream.0,
                        });
                    }
                }
                if r.index > 0 {
                    let prev = ids[r.index - 1];
                    if !matches!(nodes[prev].kind, NodeKind::LabelReference) {
                        edges.push(TopoEdge {
                            writer: prev,
                            writer_stream: StreamId::PRIMARY.0,
                            reader: def_node,
                            reader_stream: stream.0,
                        });
                    }
                }
            }
        }
    }

    edges.sort_by_key(|e| (e.writer, e.writer_stream, e.reader, e.reader_stream));
    edges.dedup();
    PipelineTopology { nodes, edges }
}

struct LabelRef {
    pipeline: usize,
    index: usize,
    node: Option<usize>,
    occurrence: u32,
}

fn stream_for_occurrence(occ: u32) -> StreamId {
    StreamId(occ)
}

fn node_from_stage(pipeline: usize, stage: usize, spec: &ParsedStage) -> TopoNode {
    let kind = if spec.connector.is_some() {
        NodeKind::Connector
    } else if spec.verb.is_empty() && spec.label.is_some() {
        if spec.label_occurrence == 0 {
            NodeKind::LabelDefinition
        } else {
            NodeKind::LabelReference
        }
    } else {
        NodeKind::Stage
    };
    TopoNode {
        pipeline: (pipeline + 1) as u32,
        stage: (stage + 1) as u32,
        kind,
        label: spec.label.clone(),
        label_occurrence: spec.label_occurrence,
        verb: spec.verb.clone(),
        running: spec.running.clone(),
        span: spec.span,
        verb_span: spec.verb_span,
    }
}
