//! Convert the graph the GUI draws into Mermaid flowchart text.
//!
//! The conversion is pure: it reads [`GraphView`] and returns text. Clipboard
//! and menus live in the window code.

use crate::graph::{stream_caption, GraphView, PlacedEdge, PlacedNode};

/// Console line after a successful copy.
pub const COPIED_LOG: &str = "Copied the pipe as Mermaid.";
/// Console line when Source is empty or there is no graph to copy.
pub const NOTHING_LOG: &str = "No pipe is shown to copy.";

/// Clipboard text: a GitHub Mermaid fence around the flowchart.
pub fn mermaid_from_graph(view: &GraphView) -> String {
    let body = flowchart_from_graph(view);
    format!("```mermaid\n{body}```\n")
}

/// Mermaid flowchart body (`flowchart LR` plus nodes and connections).
///
/// Left to right matches the usual reading of a pipe. Node labels are the
/// stage title and operands. Connection labels match the words drawn on
/// secondary outputs.
pub fn flowchart_from_graph(view: &GraphView) -> String {
    let mut out = String::from("flowchart LR\n");
    for n in &view.nodes {
        let id = node_id(n.index);
        let label = escape_mermaid_label(&stage_label(n));
        out.push_str(&format!("  {id}[\"{label}\"]\n"));
    }
    for e in &view.edges {
        if !view.nodes.iter().any(|n| n.index == e.writer)
            || !view.nodes.iter().any(|n| n.index == e.reader)
        {
            continue;
        }
        let from = node_id(e.writer);
        let to = node_id(e.reader);
        match edge_label(view, e) {
            Some(cap) => {
                let cap = escape_mermaid_label(&cap);
                out.push_str(&format!("  {from} -->|\"{cap}\"| {to}\n"));
            }
            None => out.push_str(&format!("  {from} --> {to}\n")),
        }
    }
    out
}

fn node_id(index: usize) -> String {
    format!("s{index}")
}

/// Stage name plus operands, the same information the graph box shows.
pub fn stage_label(n: &PlacedNode) -> String {
    let title = n.title.trim();
    let operands = operands_for(n);
    if title.is_empty() {
        return if operands.is_empty() {
            "stage".into()
        } else {
            operands
        };
    }
    if operands.is_empty() {
        title.to_string()
    } else {
        format!("{title} {operands}")
    }
}

fn operands_for(n: &PlacedNode) -> String {
    let running = n.full_text.trim();
    let verb = n.verb.trim();
    if !verb.is_empty() {
        if let Some(rest) = strip_prefix_ci(running, verb) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    n.detail.trim().to_string()
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut t = text.chars();
    for p in prefix.chars() {
        match t.next() {
            Some(c) if c.eq_ignore_ascii_case(&p) => {}
            _ => return None,
        }
    }
    Some(t.as_str())
}

fn edge_label(view: &GraphView, e: &PlacedEdge) -> Option<String> {
    let stream_word = view
        .ports
        .iter()
        .find(|p| p.node_index == e.writer && p.output && p.stream == e.writer_stream)
        .map(|p| p.word.as_str())
        .filter(|s| !s.is_empty());
    stream_caption(e.writer_stream, None, stream_word)
}

/// Quote-safe label body for a Mermaid flowchart node or connection.
///
/// `#` is encoded first as a character of the input, then other special
/// characters, so entity markers we insert are not re-encoded.
pub fn escape_mermaid_label(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '#' => out.push_str("#35;"),
            '"' => out.push_str("#quot;"),
            '<' => out.push_str("#lt;"),
            '>' => out.push_str("#gt;"),
            '&' => out.push_str("#amp;"),
            '[' => out.push_str("#91;"),
            ']' => out.push_str("#93;"),
            '{' => out.push_str("#123;"),
            '}' => out.push_str("#125;"),
            '(' => out.push_str("#40;"),
            ')' => out.push_str("#41;"),
            '|' => out.push_str("#124;"),
            ';' => out.push_str("#59;"),
            '`' => out.push_str("#96;"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GuiModel;
    use std::path::PathBuf;

    fn graph_of(spec: &str) -> crate::graph::GraphView {
        let mut m = GuiModel::new(PathBuf::from("."));
        m.set_text(spec.to_string());
        assert!(
            m.parse_error.is_none(),
            "spec should parse: {:?}",
            m.parse_error
        );
        m.graph.expect("graph")
    }

    fn node<'a>(view: &'a GraphView, verb: &str) -> &'a PlacedNode {
        let key = verb.to_ascii_uppercase();
        view.nodes
            .iter()
            .find(|n| n.verb.to_ascii_uppercase() == key)
            .unwrap_or_else(|| panic!("missing {verb} in {:?}", titles(view)))
    }

    fn titles(view: &GraphView) -> Vec<String> {
        view.nodes.iter().map(stage_label).collect()
    }

    fn outgoing<'a>(view: &'a GraphView, writer: usize) -> Vec<&'a PlacedEdge> {
        view.edges.iter().filter(|e| e.writer == writer).collect()
    }

    #[test]
    fn fence_wraps_flowchart_lr() {
        let view = graph_of("literal hello| hole");
        let text = mermaid_from_graph(&view);
        assert!(text.starts_with("```mermaid\n"), "{text}");
        assert!(text.ends_with("```\n"), "{text}");
        assert!(text.contains("flowchart LR\n"), "{text}");
        assert!(!text.to_lowercase().contains("debugger"), "{text}");
        assert!(!text.to_lowercase().contains("lane"), "{text}");
    }

    #[test]
    fn linear_pipe_has_one_node_per_stage_and_primary_connections() {
        let view = graph_of("literal apple|literal banana|locate /ap/|console");
        let body = flowchart_from_graph(&view);
        assert_eq!(view.nodes.len(), 4, "{:?}", titles(&view));
        assert_eq!(view.edges.len(), 3);
        for n in &view.nodes {
            assert!(
                body.contains(&format!("s{}[\"", n.index)),
                "missing node {} in {body}",
                n.index
            );
        }
        let lit = &view.nodes[0];
        assert_eq!(stage_label(lit), "LITERAL apple");
        let loc = node(&view, "LOCATE");
        assert_eq!(stage_label(loc), "LOCATE /ap/");
        for e in &view.edges {
            assert_eq!(e.writer_stream, 0);
            let line = format!("  s{} --> s{}\n", e.writer, e.reader);
            assert!(body.contains(&line), "missing {line} in {body}");
            assert!(
                !body.contains(&format!("s{} -->|\"", e.writer)),
                "primary connection should be unlabeled: {body}"
            );
        }
    }

    #[test]
    fn locate_secondary_output_is_labeled_unmatched() {
        let view = graph_of(
            "(ENDCHAR ?) literal apple|literal banana|literal apricot| L: locate /ap/| console ? L: | hole",
        );
        let loc = node(&view, "LOCATE");
        let outs = outgoing(&view, loc.index);
        assert_eq!(outs.len(), 2, "{outs:?}");
        let body = flowchart_from_graph(&view);
        let labeled: Vec<_> = outs
            .iter()
            .filter(|e| e.writer_stream != 0)
            .map(|e| edge_label(&view, e).expect("secondary label"))
            .collect();
        assert_eq!(labeled, vec!["unmatched".to_string()]);
        assert!(
            body.contains("-->|\"unmatched\"|"),
            "secondary connection: {body}"
        );
        assert!(
            stage_label(loc).contains("LOCATE /ap/"),
            "{}",
            stage_label(loc)
        );
        assert!(
            view.nodes
                .iter()
                .any(|n| n.verb.eq_ignore_ascii_case("HOLE")),
            "{:?}",
            titles(&view)
        );
    }

    #[test]
    fn fanout_wires_each_secondary_output() {
        let view = graph_of("(ENDCHAR ?) literal hello | a: fanout | hole ? a: | console");
        let fan = node(&view, "FANOUT");
        assert_eq!(stage_label(fan), "A: FANOUT");
        let outs = outgoing(&view, fan.index);
        assert_eq!(outs.len(), 2, "primary plus one secondary: {outs:?}");
        let mut streams: Vec<u32> = outs.iter().map(|e| e.writer_stream).collect();
        streams.sort();
        assert_eq!(streams, vec![0, 1]);
        let body = flowchart_from_graph(&view);
        let secondary = outs.iter().find(|e| e.writer_stream == 1).unwrap();
        let cap = edge_label(&view, secondary).expect("fanout secondary caption");
        assert_eq!(cap, "stream 1");
        assert!(
            body.contains(&format!(
                "s{} -->|\"stream 1\"| s{}",
                fan.index, secondary.reader
            )),
            "{body}"
        );
        let hole = node(&view, "HOLE");
        let console = node(&view, "CONSOLE");
        assert!(
            outs.iter()
                .any(|e| e.writer_stream == 0 && e.reader == hole.index)
                || outs
                    .iter()
                    .any(|e| e.writer_stream == 0 && e.reader == console.index)
        );
    }

    #[test]
    fn marina_fanout_has_four_outputs() {
        let spec = include_str!("../../../examples/marina/to-csv.pipe");
        let view = graph_of(spec.trim());
        let fan = node(&view, "FANOUT");
        let outs = outgoing(&view, fan.index);
        assert_eq!(outs.len(), 4, "marina fanout outputs: {outs:?}");
        let body = flowchart_from_graph(&view);
        for e in &outs {
            if e.writer_stream == 0 {
                continue;
            }
            let cap = edge_label(&view, e).expect("secondary");
            assert_eq!(cap, format!("stream {}", e.writer_stream));
            assert!(
                body.contains(&format!("s{} -->|\"{cap}\"| s{}", fan.index, e.reader)),
                "{body}"
            );
        }
    }

    #[test]
    fn escape_encodes_quotes_hashes_brackets_and_separators() {
        let raw = r#"say "hi" #tag <a> {b} [c] (d) | /path/ e;f `g`"#;
        let esc = escape_mermaid_label(raw);
        assert!(!esc.contains('"'), "{esc}");
        assert!(
            !esc.char_indices()
                .any(|(i, c)| c == '#' && !is_entity_at(&esc, i)),
            "bare hash in {esc}"
        );
        assert!(esc.contains("#quot;"), "{esc}");
        assert!(esc.contains("#35;"), "{esc}");
        assert!(esc.contains("#lt;"), "{esc}");
        assert!(esc.contains("#gt;"), "{esc}");
        assert!(esc.contains("#123;"), "{esc}");
        assert!(esc.contains("#125;"), "{esc}");
        assert!(esc.contains("#91;"), "{esc}");
        assert!(esc.contains("#93;"), "{esc}");
        assert!(esc.contains("#40;"), "{esc}");
        assert!(esc.contains("#41;"), "{esc}");
        assert!(esc.contains("#124;"), "{esc}");
        assert!(esc.contains("#59;"), "{esc}");
        assert!(esc.contains("#96;"), "{esc}");
        assert!(esc.contains("/path/"), "{esc}");
    }

    fn is_entity_at(s: &str, hash_idx: usize) -> bool {
        s[hash_idx..]
            .find(';')
            .map(|semi| {
                let body = &s[hash_idx + 1..hash_idx + semi];
                !body.is_empty() && body.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .unwrap_or(false)
    }

    #[test]
    fn pipe_with_awkward_operands_stays_quoted_and_escaped() {
        // `||` is an escaped pipe in a delimited string, so LOCATE stays one stage.
        let view = graph_of(r#"literal say "hi"|locate /a||b#c/|hole"#);
        let text = mermaid_from_graph(&view);
        assert_eq!(view.nodes.len(), 3, "{:?}", titles(&view));
        let lit = view
            .nodes
            .iter()
            .find(|n| n.verb.eq_ignore_ascii_case("LITERAL"))
            .unwrap();
        assert_eq!(stage_label(lit), r#"LITERAL say "hi""#);
        let loc = node(&view, "LOCATE");
        assert_eq!(stage_label(loc), r#"LOCATE /a||b#c/"#);
        assert!(text.contains("#quot;hi#quot;"), "{text}");
        assert!(text.contains("#124;"), "{text}");
        assert!(text.contains("#35;"), "{text}");
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('s') && trimmed.contains("[\"") {
                assert!(trimmed.ends_with("\"]"), "{line}");
            }
        }
    }

    #[test]
    fn file_stage_angle_brackets_are_escaped() {
        let view = graph_of("< examples/marina/vessels.jsonl | hole");
        let body = flowchart_from_graph(&view);
        assert!(body.contains("#lt;"), "{body}");
        assert!(!body.contains("[\"< "), "{body}");
        let file = view
            .nodes
            .iter()
            .find(|n| n.verb == "<")
            .expect("file stage");
        assert!(
            stage_label(file).contains("examples/marina/vessels.jsonl"),
            "{}",
            stage_label(file)
        );
    }

    #[test]
    #[ignore = "writes sample .mmd files when PIPE_MERMAID_DUMP is set"]
    fn dump_examples_when_env_set() {
        let Ok(dir) = std::env::var("PIPE_MERMAID_DUMP") else {
            return;
        };
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let samples = [
            (
                "linear",
                "literal apple|literal banana|literal apricot|locate /ap/|console",
            ),
            (
                "locate-secondary",
                "(ENDCHAR ?) literal apple|literal banana|literal apricot| L: locate /ap/| console ? L: | hole",
            ),
            (
                "marina-fanout",
                include_str!("../../../examples/marina/to-csv.pipe").trim(),
            ),
        ];
        for (name, spec) in samples {
            let view = graph_of(spec);
            std::fs::write(dir.join(format!("{name}.mmd")), flowchart_from_graph(&view)).unwrap();
            std::fs::write(dir.join(format!("{name}.md")), mermaid_from_graph(&view)).unwrap();
        }
    }

    #[test]
    fn empty_graph_is_still_a_flowchart() {
        let view = GraphView {
            nodes: Vec::new(),
            edges: Vec::new(),
            ports: Vec::new(),
            width: 0.0,
            height: 0.0,
            direction: crate::graph::LayoutDirection::Horizontal,
        };
        let text = mermaid_from_graph(&view);
        assert_eq!(text, "```mermaid\nflowchart LR\n```\n");
    }

    #[test]
    fn log_lines_are_plain_and_name_the_gui_not_a_debugger() {
        for s in [COPIED_LOG, NOTHING_LOG] {
            let lower = s.to_lowercase();
            assert!(s.ends_with('.'), "{s}");
            assert!(!lower.contains("debugger"), "{s}");
            assert!(!lower.contains("lane"), "{s}");
            assert!(!lower.contains("cms"), "{s}");
            assert!(!lower.contains("ibm"), "{s}");
            assert!(!lower.contains("z/vm"), "{s}");
        }
    }
}
