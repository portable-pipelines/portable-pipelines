//! Graph → pipeline text. Release 1 covers simple one-line pipes.
//!
//! Multistream, labels, and leading `(OPTIONS)` groups are rejected with a
//! structured error so later releases can fill those paths in without changing
//! the simple-pipe contract.

use pipe_parser::SourceSpan;
use pipe_stages::{NodeKind, PipelineTopology};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerializeError {
    Empty,
    MultiplePipelines,
    Labels,
    Connectors,
    Multistream,
}

impl std::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the graph has no stages"),
            Self::MultiplePipelines => {
                write!(
                    f,
                    "serializing more than one pipeline is not implemented yet"
                )
            }
            Self::Labels => write!(f, "serializing labels is not implemented yet"),
            Self::Connectors => write!(f, "serializing connectors is not implemented yet"),
            Self::Multistream => {
                write!(
                    f,
                    "serializing secondary and numbered streams is not implemented yet"
                )
            }
        }
    }
}

/// Stages that appear as boxes (not label-only references).
fn graphable(topo: &PipelineTopology) -> Vec<usize> {
    topo.nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind != NodeKind::LabelReference)
        .map(|(i, _)| i)
        .collect()
}

/// One-line pipe text (`verb args | verb args`) from a simple single-pipeline graph.
pub fn simple_pipe_text(topo: &PipelineTopology) -> Result<String, SerializeError> {
    let idxs = graphable(topo);
    if idxs.is_empty() {
        return Err(SerializeError::Empty);
    }
    let mut pipelines = Vec::new();
    for &i in &idxs {
        let n = &topo.nodes[i];
        if !pipelines.contains(&n.pipeline) {
            pipelines.push(n.pipeline);
        }
        if n.label.is_some() || n.kind == NodeKind::LabelDefinition {
            return Err(SerializeError::Labels);
        }
        if n.kind == NodeKind::Connector {
            return Err(SerializeError::Connectors);
        }
    }
    if pipelines.len() > 1 {
        return Err(SerializeError::MultiplePipelines);
    }
    for e in &topo.edges {
        if e.writer_stream != 0 || e.reader_stream != 0 {
            return Err(SerializeError::Multistream);
        }
    }
    let mut parts = Vec::new();
    for &i in &idxs {
        let running = topo.nodes[i].running.trim();
        if running.is_empty() {
            return Err(SerializeError::Labels);
        }
        parts.push(running.to_string());
    }
    Ok(parts.join(" | "))
}

/// Same verbs (order) and primary connections. Ignores exact source bytes.
pub fn equivalent_topology(a: &PipelineTopology, b: &PipelineTopology) -> bool {
    let ia = graphable(a);
    let ib = graphable(b);
    if ia.len() != ib.len() {
        return false;
    }
    for (&i, &j) in ia.iter().zip(ib.iter()) {
        if !a.nodes[i].verb.eq_ignore_ascii_case(&b.nodes[j].verb) {
            return false;
        }
        if a.nodes[i].kind != b.nodes[j].kind {
            return false;
        }
    }
    let remap = |nodes: &[usize], idx: usize| nodes.iter().position(|&n| n == idx);
    let edges_of = |topo: &PipelineTopology, nodes: &[usize]| -> Vec<(usize, u32, usize, u32)> {
        let mut e: Vec<_> = topo
            .edges
            .iter()
            .filter_map(|edge| {
                Some((
                    remap(nodes, edge.writer)?,
                    edge.writer_stream,
                    remap(nodes, edge.reader)?,
                    edge.reader_stream,
                ))
            })
            .collect();
        e.sort();
        e
    };
    edges_of(a, &ia) == edges_of(b, &ib)
}

/// Replace the operand text of one stage in place, keeping the rest of the spec.
pub fn patch_stage_operands(
    source: &str,
    verb_span: SourceSpan,
    stage_span: SourceSpan,
    new_args: &str,
) -> Result<String, String> {
    if verb_span.end > source.len() || stage_span.end > source.len() {
        return Err("stage span is outside the source text".into());
    }
    if verb_span.end > stage_span.end || verb_span.start < stage_span.start {
        return Err("verb span must sit inside the stage span".into());
    }
    if !source.is_char_boundary(verb_span.end) || !source.is_char_boundary(stage_span.end) {
        return Err("stage span is not on a character boundary".into());
    }
    let mut out = String::with_capacity(source.len() + new_args.len() + 2);
    out.push_str(&source[..verb_span.end]);
    let args = new_args.trim();
    if !args.is_empty() {
        out.push(' ');
        out.push_str(args);
    }
    let rest = &source[stage_span.end..];
    if rest.starts_with('|') && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_parser::parse_pipeline;
    use pipe_stages::inspect_topology;

    fn round_trip(spec: &str) {
        let parsed = parse_pipeline(spec).unwrap();
        let t1 = inspect_topology(&parsed);
        let text = simple_pipe_text(&t1).unwrap_or_else(|e| panic!("{spec:?}: {e}"));
        let parsed2 = parse_pipeline(&text).unwrap();
        let t2 = inspect_topology(&parsed2);
        assert!(
            equivalent_topology(&t1, &t2),
            "round trip graph mismatch\nfrom: {spec}\n to: {text}"
        );
    }

    #[test]
    fn simple_one_line_pipes_round_trip() {
        round_trip("literal a|hole");
        round_trip("literal apple | locate /ap/ | console");
        round_trip("literal a | spec 1. | console");
        round_trip("literal hello world | hole");
    }

    #[test]
    fn rejects_labels_and_second_pipeline() {
        let t = inspect_topology(
            &parse_pipeline("(ENDCHAR ?) literal a | L: locate /x/ | console ? L: | console")
                .unwrap(),
        );
        let err = simple_pipe_text(&t).unwrap_err();
        assert!(
            matches!(
                err,
                SerializeError::MultiplePipelines | SerializeError::Labels
            ),
            "{err:?}"
        );
    }

    #[test]
    fn patch_operands_keeps_surrounding_text() {
        let src = "literal apple | hole";
        let parsed = parse_pipeline(src).unwrap();
        let t = inspect_topology(&parsed);
        let n = &t.nodes[0];
        let out = patch_stage_operands(src, n.verb_span, n.span, "pear").unwrap();
        assert_eq!(out, "literal pear | hole");
        let parsed2 = parse_pipeline(&out).unwrap();
        assert_eq!(parsed2.stages.len(), 2);
        let lit = parsed2.stages[0]
            .args
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            lit.contains("pear"),
            "operands={:?} running={}",
            parsed2.stages[0].args,
            parsed2.stages[0].running
        );
        let pear_end = "literal pear".len();
        let cleared = patch_stage_operands(
            &out,
            n.verb_span,
            SourceSpan::new(n.span.start, pear_end),
            "",
        )
        .unwrap();
        assert_eq!(cleared, "literal | hole");
    }
}
