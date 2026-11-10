//! Static notes about possible host side effects. Not exhaustive.

use std::path::Path;

use pipe_parser::{ParsedPipeline, ParsedStage};
use pipe_stages::{is_language_stage_verb, NodeKind, PipelineTopology, StageRegistry};

/// Plain-language facts for the About expander. Not shown as warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipelineAbout {
    pub how_step_works: String,
    pub scripts: Vec<String>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub other: Vec<String>,
}

impl PipelineAbout {
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
            && self.reads.is_empty()
            && self.writes.is_empty()
            && self.other.is_empty()
    }

    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.how_step_works.is_empty() {
            out.push(self.how_step_works.clone());
        }
        out.extend(self.scripts.iter().cloned());
        if !self.reads.is_empty() {
            out.push(format!("Reads files: {}", self.reads.join(", ")));
        }
        if !self.writes.is_empty() {
            out.push(format!("Writes files: {}", self.writes.join(", ")));
        }
        out.extend(self.other.iter().cloned());
        out
    }
}

pub fn collect_about(parsed: &ParsedPipeline) -> PipelineAbout {
    let mut python = Vec::new();
    let mut rexx = Vec::new();
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut other = Vec::new();
    for st in parsed.pipelines.iter().flatten() {
        if is_language_stage_verb(&st.verb) {
            let name = Path::new(&st.verb)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(st.verb.as_str())
                .to_string();
            match crate::graph::script_language(&st.verb) {
                Some("REXX") => rexx.push(name),
                _ => python.push(name),
            }
        }
        let v = st.verb.to_ascii_uppercase();
        let file = st
            .args
            .first()
            .map(|a| a.as_str().trim().to_string())
            .filter(|s| !s.is_empty());
        match v.as_str() {
            "<" => {
                if let Some(f) = file {
                    reads.push(f);
                }
            }
            ">" | ">>" => {
                if let Some(f) = file {
                    writes.push(f);
                }
            }
            "CONSOLE" | "TERMINAL" => {
                other.push(
                    "CONSOLE output appears in this window. The window does not read a hidden terminal."
                        .into(),
                );
            }
            "COMMAND" | "PIPECMD" | "SUBCOM" => {
                other.push(
                    "A COMMAND stage may run a program on this computer when you Step or Run."
                        .into(),
                );
            }
            _ => {}
        }
    }
    python.sort();
    python.dedup();
    rexx.sort();
    rexx.dedup();
    reads.sort();
    reads.dedup();
    writes.sort();
    writes.dedup();
    other.sort();
    other.dedup();
    let mut scripts = Vec::new();
    if let Some(s) = script_sentence(&python, "Python") {
        scripts.push(s);
    }
    if let Some(s) = script_sentence(&rexx, "REXX") {
        scripts.push(s);
    }
    PipelineAbout {
        how_step_works:
            "Each Step moves one record through one stage. A stage can have more than one input or output."
                .into(),
        scripts,
        reads,
        writes,
        other,
    }
}

fn script_sentence(names: &[String], lang: &str) -> Option<String> {
    match names {
        [] => None,
        [one] => Some(format!(
            "{one} is a {lang} script; it only runs when you Step or Run"
        )),
        [a, b] => Some(format!(
            "{a} and {b} are {lang} scripts; they only run when you Step or Run"
        )),
        many => Some(format!(
            "{} are {lang} scripts; they only run when you Step or Run",
            many.join(", ")
        )),
    }
}

pub fn is_blocking_note(note: &str) -> bool {
    note.contains("unknown stage") || note.starts_with("parser:")
}

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
            && !is_language_stage_verb(&node.verb)
        {
            out.push(format!(
                "unknown stage {} (still shown; Run will fail until it is registered)",
                node.verb
            ));
        }
        if let Some(note) = side_effect_note(&node.verb) {
            out.push(format!("{}: {note}", display_verb(node.verb.as_str())));
        }
        if is_language_stage_verb(&node.verb) {
            let lang = crate::graph::script_language(&node.verb).unwrap_or("script");
            out.push(format!(
                "{}: {lang} stage. The graph does not open or run the file.",
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
    ) || is_language_stage_verb(&stage.verb)
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
        assert!(notes.iter().any(|n| n.contains("Run will fail")));
    }

    #[test]
    fn python_script_is_not_unknown_builtin() {
        let parsed = parse_pipeline("literal a | examples/tuck.py owner | hole").unwrap();
        let topo = inspect_topology(&parsed);
        let notes = collect_warnings(&parsed, &topo, &StageRegistry::default());
        assert!(
            notes.iter().any(|n| {
                n.to_ascii_lowercase().contains("tuck.py")
                    && n.contains("not open or run")
                    && !n.contains("not an unknown")
            }),
            "{notes:?}"
        );
        assert!(
            !notes
                .iter()
                .any(|n| n.contains("unknown stage") && n.to_ascii_lowercase().contains("tuck.py")),
            "{notes:?}"
        );
    }

    #[test]
    fn about_names_scripts_and_files_plainly() {
        let parsed =
            parse_pipeline("< in.csv | examples/tuck.py owner | examples/pack.py | > out.txt")
                .unwrap();
        let about = collect_about(&parsed);
        let lines = about.lines();
        assert!(
            lines.iter().any(|l| l.contains("tuck.py")
                && l.contains("pack.py")
                && l.contains("Python")
                && l.contains("Step or Run")),
            "{lines:?}"
        );
        assert_eq!(about.reads, vec!["in.csv".to_string()]);
        assert_eq!(about.writes, vec!["out.txt".to_string()]);
        assert!(lines.iter().any(|l| l.starts_with("Reads files:")));
        assert!(lines.iter().any(|l| l.starts_with("Writes files:")));
        assert!(!lines.iter().any(|l| l.contains("not exhaustive")));
        assert!(!lines.iter().any(|l| l.contains("I/O cycle")));
    }

    #[test]
    fn blocking_notes_are_unknown_or_parser_only() {
        assert!(is_blocking_note(
            "unknown stage NOTASTAGE (still shown; Run will fail until it is registered)"
        ));
        assert!(is_blocking_note("parser: extra junk"));
        assert!(!is_blocking_note("<: reads a host file"));
    }
}
