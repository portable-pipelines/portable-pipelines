//! Table-driven PIPE specifications (`tests/pipelines/*.pipefix`).
//!
//! CONSOLE / TERMINAL are rewritten to in-memory collectors. The work
//! directory is the repository root so `examples/` paths resolve.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pipe_core::StageCtx;
use pipe_parser::{parse_pipeline, ParsedPipeline};
use pipe_stages::{collect_to, StageRegistry};

const COLLECT: &str = ">>COLLECT";
const COLLECT_P: &str = ">>COLLECT_P";
const COLLECT_S: &str = ">>COLLECT_S";

#[derive(Debug)]
struct Case {
    id: String,
    spec: String,
    source: String,
    skip: Option<String>,
    need: Option<String>,
    expect: Option<Vec<String>>,
    primary: Option<Vec<String>>,
    secondary: Option<Vec<String>>,
    error: Option<Vec<String>>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_dir() -> PathBuf {
    repo_root().join("tests/pipelines")
}

fn load_all_cases() -> Vec<Case> {
    let mut files: Vec<PathBuf> = fs::read_dir(fixture_dir())
        .expect("tests/pipelines")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pipefix"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        out.extend(parse_pipefix(&path));
    }
    out
}

fn parse_pipefix(path: &Path) -> Vec<Case> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let source = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("pipefix")
        .to_string();
    let mut cases = Vec::new();
    let mut cur_id: Option<String> = None;
    let mut spec_lines: Vec<String> = Vec::new();
    let mut skip = None;
    let mut need = None;
    let mut expect = None;
    let mut primary = None;
    let mut secondary = None;
    let mut error = None;
    let mut saw_field = false;

    let flush = |cases: &mut Vec<Case>,
                 cur_id: &mut Option<String>,
                 spec_lines: &mut Vec<String>,
                 skip: &mut Option<String>,
                 need: &mut Option<String>,
                 expect: &mut Option<Vec<String>>,
                 primary: &mut Option<Vec<String>>,
                 secondary: &mut Option<Vec<String>>,
                 error: &mut Option<Vec<String>>,
                 saw_field: &mut bool,
                 source: &str| {
        let Some(id) = cur_id.take() else {
            spec_lines.clear();
            return;
        };
        let spec = spec_lines.join("\n").trim().to_string();
        spec_lines.clear();
        if spec.is_empty() {
            panic!("{source}: case {id} has no specification");
        }
        if !*saw_field && skip.is_none() {
            panic!("{source}: case {id} needs =>, => error:, or skip:");
        }
        cases.push(Case {
            id,
            spec,
            source: source.to_string(),
            skip: skip.take(),
            need: need.take(),
            expect: expect.take(),
            primary: primary.take(),
            secondary: secondary.take(),
            error: error.take(),
        });
        *saw_field = false;
    };

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.starts_with("%% ") {
            flush(
                &mut cases,
                &mut cur_id,
                &mut spec_lines,
                &mut skip,
                &mut need,
                &mut expect,
                &mut primary,
                &mut secondary,
                &mut error,
                &mut saw_field,
                &source,
            );
            cur_id = Some(trimmed[3..].trim().to_string());
            continue;
        }
        if cur_id.is_none() {
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("skip:") {
            skip = Some(rest.trim().to_string());
            saw_field = true;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("need:") {
            need = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("=>") {
            let rest = rest.trim();
            saw_field = true;
            if let Some(msg) = rest.strip_prefix("error:") {
                error
                    .get_or_insert_with(Vec::new)
                    .push(msg.trim().to_string());
            } else if let Some(body) = rest.strip_prefix("primary:") {
                primary = Some(parse_record_list(body.trim()));
            } else if let Some(body) = rest.strip_prefix("secondary:") {
                secondary = Some(parse_record_list(body.trim()));
            } else {
                expect = Some(parse_record_list(rest));
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        spec_lines.push(line.to_string());
    }
    flush(
        &mut cases,
        &mut cur_id,
        &mut spec_lines,
        &mut skip,
        &mut need,
        &mut expect,
        &mut primary,
        &mut secondary,
        &mut error,
        &mut saw_field,
        &source,
    );
    cases
}

fn parse_record_list(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() || s == "(empty)" {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut rest = s;
    loop {
        if rest.is_empty() {
            break;
        }
        let (item, next) = if rest.starts_with('"') {
            let bytes = rest.as_bytes();
            let mut i = 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            assert!(i < bytes.len(), "unclosed quoted record in {s:?}");
            let inner = &rest[1..i];
            let after = rest[i + 1..].trim_start();
            let after = after
                .strip_prefix('/')
                .map(|t| t.strip_prefix(' ').unwrap_or(t))
                .unwrap_or(after);
            (inner.to_string(), after)
        } else if let Some((a, b)) = rest.split_once(" / ") {
            (a.to_string(), b)
        } else {
            (rest.to_string(), "")
        };
        out.push(expand_visible_spaces(&item));
        rest = next;
    }
    out
}

fn expand_visible_spaces(s: &str) -> String {
    s.replace('␠', " ")
}

fn is_console(verb: &str) -> bool {
    verb.eq_ignore_ascii_case("CONSOLE") || verb.eq_ignore_ascii_case("TERMINAL")
}

fn rewrite_consoles(parsed: &mut ParsedPipeline) -> usize {
    let count = parsed
        .pipelines
        .iter()
        .flat_map(|p| p.iter())
        .filter(|s| is_console(&s.verb))
        .count();
    let mut seen = 0usize;
    for pipe in &mut parsed.pipelines {
        for stage in pipe {
            if !is_console(&stage.verb) {
                continue;
            }
            seen += 1;
            stage.verb = if count >= 2 {
                if seen == 1 {
                    COLLECT_P
                } else {
                    COLLECT_S
                }
            } else {
                COLLECT
            }
            .to_string();
            stage.args.clear();
            stage.raw_args.clear();
        }
    }
    if !parsed.pipelines.is_empty() {
        parsed.stages = parsed.pipelines[0].clone();
    }
    count
}

struct RunOut {
    primary: Vec<String>,
    secondary: Vec<String>,
}

fn run_case(spec: &str) -> Result<RunOut, String> {
    let mut parsed = parse_pipeline(spec).map_err(|e| e.to_string())?;
    rewrite_consoles(&mut parsed);
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    {
        let p = Rc::clone(&primary);
        reg.register(COLLECT, move |_| collect_to(Rc::clone(&p)));
    }
    {
        let p = Rc::clone(&primary);
        reg.register(COLLECT_P, move |_| collect_to(Rc::clone(&p)));
    }
    {
        let s = Rc::clone(&secondary);
        reg.register(COLLECT_S, move |_| collect_to(Rc::clone(&s)));
    }
    pipe_stages::run_parsed(&parsed, &reg).map_err(|e| e.to_string())?;
    let primary = primary.borrow().clone();
    let secondary = secondary.borrow().clone();
    Ok(RunOut { primary, secondary })
}

fn run_error(spec: &str) -> String {
    match parse_pipeline(spec) {
        Err(e) => e.to_string(),
        Ok(parsed) => pipe_stages::run_parsed(&parsed, &StageRegistry::default())
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default(),
    }
}

fn extract_readme_pipes(readme: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in readme.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("pipe \"") {
            if let Some(spec) = rest.strip_suffix('"') {
                out.push(spec.to_string());
            }
        }
    }
    out
}

#[test]
fn pipeline_fixtures_run() {
    pipe_rexx::install_hook();
    pipe_python::install_hook();
    if matches!(std::env::var("PIPE_REQUIRE_REGINA").as_deref(), Ok("1")) {
        assert_eq!(
            pipe_rexx::preferred_engine(),
            pipe_rexx::RexxEngine::Regina,
            "PIPE_REQUIRE_REGINA=1 but Regina was not discovered/loaded ({})",
            pipe_rexx::discovery_hint()
        );
    }
    let root = repo_root();
    std::env::set_current_dir(&root).expect("chdir repo root");

    let cases = load_all_cases();
    assert!(
        !cases.is_empty(),
        "no *.pipefix cases under tests/pipelines"
    );

    let mut ids = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    let mut ran = 0usize;
    let mut skipped = 0usize;

    for case in &cases {
        if !ids.insert(&case.id) {
            failures.push(format!("{}: duplicate id {}", case.source, case.id));
            continue;
        }
        if let Some(reason) = &case.skip {
            skipped += 1;
            assert!(
                !reason.is_empty(),
                "{}: {} skip: needs a reason",
                case.source,
                case.id
            );
            continue;
        }
        if let Some(need) = &case.need {
            if need == "python3" {
                if !pipe_python::python_available() {
                    if pipe_python::pipe_require_python() {
                        failures.push(format!(
                            "{} ({}): PIPE_REQUIRE_PYTHON=1 but Python 3.10+ was not discovered ({})",
                            case.id,
                            case.source,
                            pipe_python::discovery_hint()
                        ));
                    } else {
                        skipped += 1;
                    }
                    continue;
                }
            } else {
                failures.push(format!(
                    "{} ({}): unknown need: {need}",
                    case.id, case.source
                ));
                continue;
            }
        }
        ran += 1;
        if let Some(needles) = &case.error {
            let err = run_error(&case.spec);
            if err.is_empty() {
                failures.push(format!(
                    "{} ({}): expected error containing {needles:?}, got empty",
                    case.id, case.source
                ));
                continue;
            }
            for needle in needles {
                if !err.contains(needle) {
                    failures.push(format!(
                        "{} ({}): expected error containing {needle:?}, got {err:?}",
                        case.id, case.source
                    ));
                }
            }
            continue;
        }
        match run_case(&case.spec) {
            Err(e) => failures.push(format!("{} ({}): run failed: {e}", case.id, case.source)),
            Ok(got) => {
                if let Some(exp) = case.primary.as_ref().or(case.expect.as_ref()) {
                    if &got.primary != exp {
                        failures.push(format!(
                            "{} ({}): primary\n  spec: {}\n  expected: {exp:?}\n  got:      {:?}",
                            case.id, case.source, case.spec, got.primary
                        ));
                    }
                }
                if let Some(exp) = &case.secondary {
                    if &got.secondary != exp {
                        failures.push(format!(
                            "{} ({}): secondary\n  spec: {}\n  expected: {exp:?}\n  got:      {:?}",
                            case.id, case.source, case.spec, got.secondary
                        ));
                    }
                }
                if case.expect.is_none() && case.primary.is_none() && case.secondary.is_none() {
                    failures.push(format!(
                        "{} ({}): no expected records",
                        case.id, case.source
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} fixture failure(s); ran {ran}, skipped {skipped}\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn readme_harvest_covers_every_pipe_line() {
    let readme = fs::read_to_string(repo_root().join("examples/README.md")).unwrap();
    let listed = extract_readme_pipes(&readme);
    assert!(
        !listed.is_empty(),
        "no pipe \"…\" lines in examples/README.md"
    );

    let harvest = fixture_dir().join("readme-harvest.pipefix");
    let cases = parse_pipefix(&harvest);
    let mut have: std::collections::BTreeSet<&str> =
        cases.iter().map(|c| c.spec.as_str()).collect();

    let mut missing = Vec::new();
    for spec in &listed {
        if !have.remove(spec.as_str()) {
            missing.push(spec.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "examples/README.md pipe lines missing from readme-harvest.pipefix:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn collect_stage_is_not_a_builtin() {
    let _ = collect_to(Rc::new(RefCell::new(Vec::<String>::new())));
    let mut dummy = StageRegistry::default();
    dummy.register(">>COLLECT", |_| Box::new(|_ctx: &mut StageCtx| {}));
    assert!(
        !StageRegistry::default().contains(">>COLLECT"),
        ">>COLLECT must stay a test sink, not an IBM verb"
    );
}
