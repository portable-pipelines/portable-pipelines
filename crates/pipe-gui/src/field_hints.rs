//! Missing `$field` notes and Source/Records hints after records have flowed.

use pipe_core::{DebugEventKind, StageId};
use pipe_stages::{json_field_tokens, operand_field_hint, NodeKind, PipelineTopology};

use crate::model::topology_stage_id;
use crate::worker::Snapshot;

/// A `$field` in a stage's operands that was not among fields seen on input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceHint {
    pub node_index: usize,
    pub span: (usize, usize),
    pub spelled: String,
    pub message: String,
}

pub fn notes_for_stage(snap: &Snapshot, id: StageId) -> Vec<String> {
    let mut out = Vec::new();
    for e in &snap.events {
        if e.stage != id {
            continue;
        }
        if let DebugEventKind::Note { text } = &e.kind {
            if !out.iter().any(|t| t == text) {
                out.push(text.clone());
            }
        }
    }
    out
}

fn seen_keys(snap: &Snapshot, id: StageId) -> Option<Vec<String>> {
    let log = snap.record_logs.iter().find(|(s, _)| *s == id)?;
    if log.1.received.is_empty() {
        return None;
    }
    let mut keys = Vec::new();
    for rec in &log.1.received {
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&rec.bytes) else {
            continue;
        };
        let Some(obj) = v.as_object() else {
            continue;
        };
        for k in obj.keys() {
            if !keys.iter().any(|x| x == k) {
                keys.push(k.clone());
            }
        }
    }
    Some(keys)
}

/// Operand `$field` tokens that are not among JSON keys seen on this stage's input.
pub fn source_hints(source: &str, topo: &PipelineTopology, snap: &Snapshot) -> Vec<SourceHint> {
    let mut out = Vec::new();
    for (i, node) in topo.nodes.iter().enumerate() {
        if !matches!(node.kind, NodeKind::Stage) {
            continue;
        }
        let Some(id) = topology_stage_id(topo, i) else {
            continue;
        };
        let Some(present) = seen_keys(snap, id) else {
            continue;
        };
        let start = node.verb_span.end.max(node.span.start);
        let end = node.span.end.min(source.len());
        if start >= end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            continue;
        }
        let ops = &source[start..end];
        for (rel, rel_end, spelled) in json_field_tokens(ops) {
            let Some(message) = operand_field_hint(&spelled, &present) else {
                continue;
            };
            out.push(SourceHint {
                node_index: i,
                span: (start + rel, start + rel_end),
                spelled,
                message,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_parser::parse_pipeline;
    use pipe_stages::inspect_topology;

    #[test]
    fn owner_i_token_sits_in_lookup_operands() {
        let src = "literal x | v: lookup $owner_i | hole";
        let parsed = parse_pipeline(src).unwrap();
        let topo = inspect_topology(&parsed);
        let lookup = topo
            .nodes
            .iter()
            .find(|n| n.verb.eq_ignore_ascii_case("LOOKUP"))
            .expect("LOOKUP");
        let ops = &src[lookup.verb_span.end..lookup.span.end];
        let toks = json_field_tokens(ops);
        assert_eq!(toks.len(), 1, "{toks:?}");
        assert_eq!(toks[0].2, "$owner_i");
        let abs = lookup.verb_span.end + toks[0].0;
        assert_eq!(&src[abs..abs + toks[0].2.len()], "$owner_i");
    }
}
