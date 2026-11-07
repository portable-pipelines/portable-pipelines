//! Static notes about possible host side effects. Not exhaustive.

use pipe_parser::{ParsedPipeline, ParsedStage};
use pipe_stages::{NodeKind, PipelineTopology, StageRegistry};

/// A verb that often writes a host file or talks to the terminal.
pub fn side_effect_note(verb: &str) -> Option<&'static str> {
    match verb.to_ascii_uppercase().as_str() {
        ">" | ">>" => Some("writes a host file"),
        "<" => Some("reads a host file"),
        "DISK" | "FILEFAST" | "DISKFAST" | "FILEUPDATE" | "FILEAPPEND" => {
            Some("reads or writes a host file depending on position")
        }
        "CONSOLE" | "TERMINAL" => Some("reads or writes the host console"),
        "COMMAND" | "PIPECMD" | "SUBCOM" => Some("may run a host command"),
        "RUNPIPE" => Some("nested PIPE text is data, not a graph"),
        _ => None,
    }
}

pub fn collect_warnings(
    parsed: &ParsedPipeline,
    topo: &PipelineTopology,
    registry: &StageRegistry,
) -> Vec<String> {
    let mut out = Vec::new();
    for d in &parsed.diagnostics {
        out.push(format!("parser: {}", d.message));
    }
    for node in &topo.nodes {
        if matches!(node.kind, NodeKind::Stage)
            && !node.verb.is_empty()
            && !registry.contains(&node.verb)
        {
            out.push(format!(
                "unknown stage {} (still shown; Run will fail until it is registered)",
                node.verb
            ));
        }
        if let Some(note) = side_effect_note(&node.verb) {
            out.push(format!("{}: {note}", display_verb(node.verb.as_str())));
        }
        if looks_like_script(&node.verb) {
            out.push(format!(
                "{}: external script. Side effects are not listed here.",
                node.verb
            ));
        }
    }
    if parsed.pipelines.iter().flatten().any(stage_may_write) {
        out.push(
            "Run is required for side effects. Opening or drawing the graph does not execute."
                .into(),
        );
        out.push(
            "This list is not exhaustive for CALLPIPE, ADDPIPE, RUNPIPE, REXX, Python, or COMMAND."
                .into(),
        );
    }
    out.sort();
    out.dedup();
    out
}

fn display_verb(verb: &str) -> &str {
    if verb.is_empty() {
        "(label)"
    } else {
        verb
    }
}

fn looks_like_script(verb: &str) -> bool {
    let l = verb.to_ascii_lowercase();
    l.ends_with(".rexx") || l.ends_with(".rex") || l.ends_with(".py")
}

fn stage_may_write(stage: &ParsedStage) -> bool {
    let v = stage.verb.to_ascii_uppercase();
    matches!(
        v.as_str(),
        ">" | ">>"
            | "DISK"
            | "FILEFAST"
            | "DISKFAST"
            | "FILEUPDATE"
            | "FILEAPPEND"
            | "CONSOLE"
            | "TERMINAL"
            | "COMMAND"
            | "PIPECMD"
    ) || looks_like_script(&stage.verb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_parser::parse_pipeline;
    use pipe_stages::inspect_topology;

    #[test]
    fn file_write_is_warned_not_executed() {
        let parsed = parse_pipeline("literal secret | > out.txt").unwrap();
        let topo = inspect_topology(&parsed);
        let notes = collect_warnings(&parsed, &topo, &StageRegistry::default());
        assert!(notes.iter().any(|n| n.contains("writes a host file")));
        assert!(notes.iter().any(|n| n.contains("Run is required")));
    }

    #[test]
    fn unknown_verb_is_visible() {
        let parsed = parse_pipeline("notastage | hole").unwrap();
        let topo = inspect_topology(&parsed);
        let notes = collect_warnings(&parsed, &topo, &StageRegistry::default());
        assert!(notes.iter().any(|n| n.contains("NOTASTAGE")));
    }
}
