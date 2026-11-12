//! Graph → pipeline text, including labels and ENDCHAR for secondary streams.
//!
//! Display names on connection points stay out of this text. `*:` connectors
//! are not written (those wait for composite stages).

use std::collections::HashSet;

use pipe_parser::SourceSpan;
use pipe_stages::{NodeKind, PipelineTopology};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerializeError {
    Empty,
    Connectors,
    Cycle,
}

impl std::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the graph has no stages"),
            Self::Connectors => write!(
                f,
                "this release does not rewrite pipes that use pipe input or pipe output connectors"
            ),
            Self::Cycle => write!(
                f,
                "this graph has a loop on the primary connections, so it cannot be written as a pipe"
            ),
        }
    }
}

impl std::error::Error for SerializeError {}

/// One box on the graph (not a later `L:` reference).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphStage {
    pub verb: String,
    /// Clause without a leading label (`locate /ap/`).
    pub running: String,
    /// Label already on this stage in Source, if any.
    pub label: Option<String>,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub writer: usize,
    pub writer_stream: u32,
    pub reader: usize,
    pub reader_stream: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphSpec {
    pub stages: Vec<GraphStage>,
    pub edges: Vec<GraphEdge>,
}

/// Stages that appear as boxes (not label-only references).
pub fn graphable(topo: &PipelineTopology) -> Vec<usize> {
    topo.nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind != NodeKind::LabelReference)
        .map(|(i, _)| i)
        .collect()
}

pub fn graph_from_topo(topo: &PipelineTopology) -> Result<GraphSpec, SerializeError> {
    let idxs = graphable(topo);
    if idxs.is_empty() {
        return Err(SerializeError::Empty);
    }
    let mut stages = Vec::with_capacity(idxs.len());
    let mut remap = vec![None; topo.nodes.len()];
    for (g, &i) in idxs.iter().enumerate() {
        let n = &topo.nodes[i];
        if n.kind == NodeKind::Connector {
            return Err(SerializeError::Connectors);
        }
        remap[i] = Some(g);
        let label = if n.label_occurrence == 0 {
            n.label.clone()
        } else {
            None
        };
        stages.push(GraphStage {
            verb: n.verb.clone(),
            running: n.running.clone(),
            label,
            kind: n.kind,
        });
    }
    let mut edges = Vec::new();
    for e in &topo.edges {
        let Some(w) = remap.get(e.writer).copied().flatten() else {
            continue;
        };
        let Some(r) = remap.get(e.reader).copied().flatten() else {
            continue;
        };
        edges.push(GraphEdge {
            writer: w,
            writer_stream: e.writer_stream,
            reader: r,
            reader_stream: e.reader_stream,
        });
    }
    edges.sort_by_key(|e| (e.writer, e.writer_stream, e.reader, e.reader_stream));
    edges.dedup();
    Ok(GraphSpec { stages, edges })
}

fn is_primary(e: &GraphEdge) -> bool {
    e.writer_stream == 0 && e.reader_stream == 0
}

fn stage_clause(s: &GraphStage, label: Option<&str>) -> String {
    let run = s.running.trim();
    match (label, run.is_empty()) {
        (Some(l), true) => format!("{l}:"),
        (Some(l), false) => format!("{l}: {run}"),
        (None, true) => s.verb.to_ascii_lowercase(),
        (None, false) => run.to_string(),
    }
}

fn valid_label(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '#')
}

fn assign_labels(g: &GraphSpec) -> Vec<Option<String>> {
    let n = g.stages.len();
    let mut need = vec![false; n];
    for e in &g.edges {
        if e.writer_stream != 0 {
            need[e.writer] = true;
        }
        if e.reader_stream != 0 {
            need[e.reader] = true;
        }
    }
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = vec![None; n];
    for (i, s) in g.stages.iter().enumerate() {
        if !need[i] {
            continue;
        }
        if let Some(l) = s.label.as_deref() {
            let key = l.to_ascii_lowercase();
            if valid_label(l) && taken.insert(key) {
                out[i] = Some(l.to_string());
            }
        }
    }
    let mut letters: Vec<String> = Vec::new();
    for s in &g.stages {
        if let Some(c) = s.verb.chars().next() {
            if c.is_ascii_alphabetic() {
                let l = c.to_ascii_lowercase().to_string();
                if !letters.contains(&l) {
                    letters.push(l);
                }
            }
        }
    }
    for c in 'a'..='z' {
        let l = c.to_string();
        if !letters.contains(&l) {
            letters.push(l);
        }
    }
    let mut extra = 0u32;
    for (i, slot) in out.iter_mut().enumerate() {
        if !need[i] || slot.is_some() {
            continue;
        }
        let name = loop {
            let cand = if extra == 0 {
                letters
                    .iter()
                    .find(|l| !taken.contains(l.as_str()))
                    .cloned()
            } else {
                letters.first().map(|l| format!("{l}{extra}"))
            };
            extra = extra.saturating_add(1);
            let Some(c) = cand else {
                continue;
            };
            if taken.insert(c.to_ascii_lowercase()) {
                break c;
            }
            if extra > 10_000 {
                break "z9".into();
            }
        };
        *slot = Some(name);
    }
    out
}

struct Chains {
    id: Vec<Option<usize>>,
    list: Vec<Vec<usize>>,
}

fn primary_chains(g: &GraphSpec) -> Result<Chains, SerializeError> {
    let n = g.stages.len();
    let mut prim_out = vec![None; n];
    let mut prim_in = vec![None; n];
    for e in &g.edges {
        if !is_primary(e) {
            continue;
        }
        if prim_out[e.writer].is_some() || prim_in[e.reader].is_some() {
            continue;
        }
        prim_out[e.writer] = Some(e.reader);
        prim_in[e.reader] = Some(e.writer);
    }
    let mut id = vec![None; n];
    let mut list = Vec::new();
    for i in 0..n {
        if prim_in[i].is_some() || id[i].is_some() {
            continue;
        }
        let mut chain = Vec::new();
        let mut cur = Some(i);
        let mut guard = 0;
        while let Some(c) = cur {
            if id[c].is_some() {
                return Err(SerializeError::Cycle);
            }
            id[c] = Some(list.len());
            chain.push(c);
            cur = prim_out[c];
            guard += 1;
            if guard > n + 1 {
                return Err(SerializeError::Cycle);
            }
        }
        list.push(chain);
    }
    for i in 0..n {
        if id[i].is_some() {
            continue;
        }
        let mut chain = Vec::new();
        let mut cur = Some(i);
        let mut guard = 0;
        while let Some(c) = cur {
            if id[c].is_some() {
                break;
            }
            id[c] = Some(list.len());
            chain.push(c);
            cur = prim_out[c];
            guard += 1;
            if guard > n + 1 {
                return Err(SerializeError::Cycle);
            }
        }
        if !chain.is_empty() {
            list.push(chain);
        }
    }
    Ok(Chains { id, list })
}

fn edge_into(g: &GraphSpec, reader: usize, stream: u32) -> Option<&GraphEdge> {
    g.edges
        .iter()
        .find(|e| e.reader == reader && e.reader_stream == stream)
}

fn edge_from(g: &GraphSpec, writer: usize, stream: u32) -> Option<&GraphEdge> {
    g.edges
        .iter()
        .find(|e| e.writer == writer && e.writer_stream == stream)
}

fn residual_out<'a>(g: &'a GraphSpec, writer: usize) -> Option<&'a GraphEdge> {
    g.edges
        .iter()
        .find(|e| e.writer == writer && !is_primary(e))
}

fn emit_chain(g: &GraphSpec, chain: &[usize], labels: &[Option<String>]) -> Vec<String> {
    chain
        .iter()
        .map(|&i| stage_clause(&g.stages[i], labels[i].as_deref()))
        .collect()
}

fn pick_endchar(body: &str) -> char {
    for c in ['?', '!', '#', '%', '~', ';', '^'] {
        if !body.contains(c) {
            return c;
        }
    }
    '?'
}

/// Pipeline text for a graph of stages and connections.
pub fn pipe_text(g: &GraphSpec) -> Result<String, SerializeError> {
    if g.stages.is_empty() {
        return Err(SerializeError::Empty);
    }
    if g.stages.iter().any(|s| s.kind == NodeKind::Connector) {
        return Err(SerializeError::Connectors);
    }
    let n = g.stages.len();
    let labels = assign_labels(g);
    let chains = primary_chains(g)?;
    let main = chains
        .list
        .iter()
        .enumerate()
        .max_by_key(|(i, c)| (c.len(), n - *i))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut used_chain = vec![false; chains.list.len()];
    let mut pipelines: Vec<Vec<String>> = Vec::new();
    pipelines.push(emit_chain(g, &chains.list[main], &labels));
    used_chain[main] = true;

    let mut covered: HashSet<(usize, u32)> = HashSet::new();
    let max_stream = g
        .edges
        .iter()
        .map(|e| e.writer_stream.max(e.reader_stream))
        .max()
        .unwrap_or(0);

    let take_chain =
        |used_chain: &mut [bool], idx: usize, labels: &[Option<String>]| -> Vec<String> {
            let Some(&cid) = chains.id[idx].as_ref() else {
                return vec![stage_clause(&g.stages[idx], labels[idx].as_deref())];
            };
            if used_chain[cid] {
                return Vec::new();
            }
            used_chain[cid] = true;
            emit_chain(g, &chains.list[cid], &labels)
        };

    for si in 0..n {
        if labels[si].is_none() {
            continue;
        }
        for stream in 1..=max_stream {
            if covered.contains(&(si, stream)) {
                continue;
            }
            let ein = edge_into(g, si, stream);
            let eout = edge_from(g, si, stream);
            if ein.is_none() && eout.is_none() {
                let higher = (stream + 1..=max_stream)
                    .any(|s| edge_into(g, si, s).is_some() || edge_from(g, si, s).is_some());
                if higher {
                    let l = labels[si].as_deref().unwrap();
                    pipelines.push(vec![format!("{l}:")]);
                    covered.insert((si, stream));
                }
                continue;
            }
            covered.insert((si, stream));
            let mut tokens = Vec::new();
            if let Some(e) = ein {
                tokens.extend(take_chain(&mut used_chain, e.writer, &labels));
            }
            tokens.push(format!("{}:", labels[si].as_deref().unwrap()));
            let mut next_out = eout;
            let mut hops = 0;
            while let Some(e) = next_out {
                tokens.extend(take_chain(&mut used_chain, e.reader, &labels));
                let last = chains
                    .id
                    .get(e.reader)
                    .and_then(|c| *c)
                    .and_then(|cid| chains.list.get(cid).and_then(|ch| ch.last()).copied())
                    .unwrap_or(e.reader);
                next_out = None;
                if let Some(res) = residual_out(g, last) {
                    if let Some(l) = labels[res.reader].as_deref() {
                        covered.insert((res.reader, res.reader_stream));
                        tokens.push(format!("{l}:"));
                        next_out = edge_from(g, res.reader, res.reader_stream);
                    }
                }
                hops += 1;
                if hops > n {
                    break;
                }
            }
            pipelines.push(tokens);
        }
    }

    for (i, ch) in chains.list.iter().enumerate() {
        if used_chain[i] {
            continue;
        }
        pipelines.push(emit_chain(g, ch, &labels));
        used_chain[i] = true;
    }

    let parts: Vec<String> = pipelines
        .into_iter()
        .map(|p| p.join(" | "))
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() <= 1 {
        return Ok(parts.first().cloned().unwrap_or_default());
    }
    let joined = parts.join(" ");
    let end = pick_endchar(&joined);
    Ok(format!(
        "(ENDCHAR {end}) {}",
        parts.join(&format!(" {end} "))
    ))
}

pub fn pipe_text_from_topo(topo: &PipelineTopology) -> Result<String, SerializeError> {
    pipe_text(&graph_from_topo(topo)?)
}

/// One-line pipe text (`verb args | verb args`) when the graph has no labels.
pub fn simple_pipe_text(topo: &PipelineTopology) -> Result<String, SerializeError> {
    let g = graph_from_topo(topo)?;
    if g.edges
        .iter()
        .any(|e| e.writer_stream != 0 || e.reader_stream != 0)
        || g.stages.iter().any(|s| s.label.is_some())
    {
        return pipe_text(&g);
    }
    pipe_text(&g)
}

/// Same verbs (order) and connections. Ignores exact source bytes.
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
        let text = pipe_text_from_topo(&t1).unwrap_or_else(|e| panic!("{spec:?}: {e}"));
        let parsed2 = parse_pipeline(&text).unwrap();
        let t2 = inspect_topology(&parsed2);
        assert!(
            equivalent_topology(&t1, &t2),
            "round trip graph mismatch\nfrom: {spec}\n to: {text}"
        );
        assert!(!text.to_ascii_lowercase().contains("debugger"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("lane"), "{text}");
    }

    #[test]
    fn simple_one_line_pipes_round_trip() {
        round_trip("literal a|hole");
        round_trip("literal apple | locate /ap/ | console");
        round_trip("literal a | spec 1. | console");
        round_trip("literal hello world | hole");
    }

    #[test]
    fn locate_unmatched_round_trip() {
        round_trip("(ENDCHAR ?) literal apple | L: locate /ap/ | console ? L: | hole");
        let t = inspect_topology(
            &parse_pipeline("(ENDCHAR ?) literal apple | L: locate /ap/ | console ? L: | hole")
                .unwrap(),
        );
        let text = pipe_text_from_topo(&t).unwrap();
        assert!(text.contains("ENDCHAR"), "{text}");
        assert!(text.contains("L:"), "{text}");
        assert!(text.contains("locate"), "{text}");
    }

    #[test]
    fn lookup_table_round_trip() {
        round_trip("(ENDCHAR ?) literal apricot | L: lookup | hole ? literal apple | L:");
    }

    #[test]
    fn lookup_table_and_not_found_and_extra() {
        round_trip(
            "(ENDCHAR ?) literal apple| L: lookup | hole ? literal apple cherry| split | L: | hole ? L: | console",
        );
    }

    #[test]
    fn fanout_two_outputs() {
        round_trip("(ENDCHAR ?) literal a | f: fanout | hole ? f: | console");
    }

    #[test]
    fn marina_style_lookup_and_fanin() {
        round_trip(
            "(ENDCHAR ?) literal v | v: lookup $k | g: fanin | hole ? literal t | v: ? literal extra | sort | g:",
        );
    }

    #[test]
    fn collate_middle_label_continues_to_fanin() {
        round_trip(
            "(ENDCHAR ?) literal a | c: collate $id | g: fanin | hole ? literal b | c: | elastic | g:",
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

    #[test]
    fn unused_error_variants_do_not_say_debugger() {
        for e in [
            SerializeError::Empty,
            SerializeError::Connectors,
            SerializeError::Cycle,
        ] {
            let s = e.to_string().to_ascii_lowercase();
            assert!(!s.contains("debugger"), "{s}");
            assert!(!s.contains("lane"), "{s}");
            assert!(!s.contains("ibm"), "{s}");
        }
    }
}
