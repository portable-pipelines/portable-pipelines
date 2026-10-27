use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use pipe_core::{
    has_stage_context, with_pipeline_message_sink, CallPipeConnectors, PipeError, PipeResult,
    PipelineBuilder, StageCtx, StageId, StageOrigin, StreamId, TraceConfig,
};
use pipe_parser::{
    parse_pipeline, parse_with_options, ConnectorKind, ParsedPipeline, ParsedStage, PipeOptions,
};

use crate::registry::StageRegistry;

/// Parse and run a PIPE specification with the default built-in registry.
pub fn run_spec(spec: &str) -> PipeResult<()> {
    run_spec_with_return_code(spec).map(|_| ())
}

/// Like [`run_spec`], but return the CMS aggregate return code (p.219).
pub fn run_spec_with_return_code(spec: &str) -> PipeResult<i32> {
    run_spec_with_trace_return_code(spec, false)
}

/// Like [`run_spec_with_return_code`], with TRACE forced on (`--trace`).
pub fn run_spec_with_trace_return_code(spec: &str, force_trace: bool) -> PipeResult<i32> {
    let parsed = parse_pipeline(spec).map_err(|e| PipeError::parse(e.to_string()))?;
    run_parsed_with_return_code(&parsed, &StageRegistry::default(), force_trace)
}

/// Like [`run_spec`], but `force_trace` enables TRACE even without `(TRACE)` in the spec.
pub fn run_spec_with_trace(spec: &str, force_trace: bool) -> PipeResult<()> {
    run_spec_with_trace_return_code(spec, force_trace).map(|_| ())
}

/// Build a dispatcher graph from a parsed specification and run it.
pub fn run_parsed(parsed: &ParsedPipeline, registry: &StageRegistry) -> PipeResult<()> {
    run_parsed_with_trace(parsed, registry, false)
}

/// Build and run, returning the CMS aggregate return code of the network.
pub fn run_parsed_with_return_code(
    parsed: &ParsedPipeline,
    registry: &StageRegistry,
    force_trace: bool,
) -> PipeResult<i32> {
    pipeline_builder_from_parsed(parsed, registry, force_trace)?.run_with_return_code()
}

/// Build and run, optionally forcing TRACE on.
pub fn run_parsed_with_trace(
    parsed: &ParsedPipeline,
    registry: &StageRegistry,
    force_trace: bool,
) -> PipeResult<()> {
    pipeline_builder_from_parsed(parsed, registry, force_trace)?.run()
}

fn pipeline_builder_from_parsed(
    parsed: &ParsedPipeline,
    registry: &StageRegistry,
    force_trace: bool,
) -> PipeResult<PipelineBuilder> {
    let mut builder = build_pipeline_graph(parsed, registry)?;
    if force_trace || parsed.options.trace {
        builder.set_trace(TraceConfig::stderr());
    }
    Ok(builder)
}

/// Build the dispatcher graph without choosing a TRACE sink.
pub(crate) fn build_pipeline_graph(
    parsed: &ParsedPipeline,
    registry: &StageRegistry,
) -> PipeResult<PipelineBuilder> {
    let mut builder = PipelineBuilder::new();
    let mut stage_ids: Vec<Vec<Option<StageId>>> = Vec::new();

    // label -> list of (pipeline_idx, stage_idx_in_pipeline, StageId, occurrence)
    let mut label_defs: HashMap<String, Vec<LabelRef>> = HashMap::new();

    for (pi, pipe) in parsed.pipelines.iter().enumerate() {
        let mut ids = Vec::new();
        for (si, stage) in pipe.iter().enumerate() {
            let origin = stage_origin(parsed, pi, si, stage);
            if stage.connector.is_some() {
                // Connectors are handled by CALLPIPE wrappers; in top-level
                // specs a lone `*:` is an error for now.
                return Err(PipeError::parse(
                    "`*:` connectors are only valid inside CALLPIPE/ADDPIPE nested specs",
                )
                .with_origin(origin));
            }
            if stage.verb.is_empty() && stage.label.is_some() {
                // Label-only reference: no new stage body; stream attachment only.
                ids.push(None);
                if let Some(lbl) = &stage.label {
                    label_defs.entry(lbl.clone()).or_default().push(LabelRef {
                        pipeline: pi,
                        index: si,
                        stage_id: None,
                        occurrence: stage.label_occurrence,
                    });
                }
                continue;
            }
            let args: Vec<String> = stage.args.iter().map(|a| a.as_str().to_string()).collect();
            let entry = resolve_stage(registry, &stage.verb, args)
                .map_err(|e| e.with_origin(origin.clone()))?;
            let id = builder.add_stage_with_origin(stage.verb.clone(), entry, Some(origin));
            ids.push(Some(id));
            if let Some(lbl) = &stage.label {
                label_defs.entry(lbl.clone()).or_default().push(LabelRef {
                    pipeline: pi,
                    index: si,
                    stage_id: Some(id),
                    occurrence: stage.label_occurrence,
                });
            }
        }
        stage_ids.push(ids);
    }

    // Linear primary connections within each pipeline.
    for (pi, pipe) in parsed.pipelines.iter().enumerate() {
        let mut prev: Option<(StageId, StreamId)> = None;
        for (si, stage) in pipe.iter().enumerate() {
            if let Some(id) = stage_ids[pi][si] {
                if let Some((pw, _ps)) = prev {
                    // Determine which input stream this stage occurrence uses.
                    let in_stream = stream_for_occurrence(stage.label_occurrence);
                    builder.connect(pw, StreamId::PRIMARY, id, in_stream);
                }
                prev = Some((id, StreamId::PRIMARY));
            } else if let Some(lbl) = &stage.label {
                // Label reference without verb: connect previous primary out to
                // the defining stage's secondary (or Nth) output / input.
                wire_label_reference(
                    &mut builder,
                    &label_defs,
                    lbl,
                    stage.label_occurrence,
                    &mut prev,
                )?;
            }
        }
    }

    // Label-only slots (`L:` with no verb) attach that stream of the defined
    // stage. Consecutive label-only stages (`L: | G: | console`, GATE p.366)
    // connect those streams to each other, not to the next verb.
    let mut label_only: HashMap<(usize, usize), (StageId, StreamId)> = HashMap::new();
    for refs in label_defs.values() {
        let Some(def) = refs.iter().find(|r| r.occurrence == 0) else {
            continue;
        };
        let Some(def_id) = def.stage_id else {
            continue;
        };
        for r in refs
            .iter()
            .filter(|r| r.occurrence > 0 && r.stage_id.is_none())
        {
            label_only.insert(
                (r.pipeline, r.index),
                (def_id, stream_for_occurrence(r.occurrence)),
            );
        }
    }

    // Cross-pipeline label wiring: second occurrence connects to secondary streams.
    for (label, refs) in &label_defs {
        if refs.len() < 2 {
            continue;
        }
        let def = refs
            .iter()
            .find(|r| r.occurrence == 0)
            .ok_or_else(|| label_error(parsed, refs, format!("label {label} has no definition")))?;
        let def_id = def.stage_id.ok_or_else(|| {
            label_error(
                parsed,
                refs,
                format!("label {label} definition has no stage"),
            )
        })?;

        for r in refs.iter().filter(|r| r.occurrence > 0) {
            let stream = stream_for_occurrence(r.occurrence);
            if let Some(ref_id) = r.stage_id {
                // Labeled stage with a verb on a later occurrence — connect
                // defining stage's secondary out to this stage's primary in,
                // or this stage's secondary out back, depending on position.
                // CMS: second label occurrence typically attaches secondary
                // stream of the *same* stage program instance (the definition).
                // Parallel pipeline `L: | console` means: take secondary output
                // of L's stage and feed console.
                // When the reference has its own verb, that verb IS a new stage
                // reading from the secondary of the defined stage.
                builder.connect(def_id, stream, ref_id, StreamId::PRIMARY);
            } else {
                let ids = &stage_ids[r.pipeline];
                // Immediate neighbour only (1.1.11 p.420 both-directions;
                // p.366 `l: | g:` cascade). Skipping a label-only neighbour
                // would wire locate's secondary straight to console and leave
                // GATE's secondary unconnected.
                if r.index + 1 < ids.len() {
                    if let Some(next_id) = ids[r.index + 1] {
                        builder.connect(def_id, stream, next_id, StreamId::PRIMARY);
                    } else if let Some(&(next_id, next_stream)) =
                        label_only.get(&(r.pipeline, r.index + 1))
                    {
                        builder.connect(def_id, stream, next_id, next_stream);
                    }
                }
                if r.index > 0 {
                    if let Some(prev_id) = ids[r.index - 1] {
                        builder.connect(prev_id, StreamId::PRIMARY, def_id, stream);
                    }
                }
            }
        }
    }

    Ok(builder)
}

struct LabelRef {
    pipeline: usize,
    index: usize,
    stage_id: Option<StageId>,
    occurrence: u32,
}

fn stream_for_occurrence(occ: u32) -> StreamId {
    StreamId(occ)
}

fn stage_origin(
    parsed: &ParsedPipeline,
    pipeline: usize,
    stage: usize,
    spec: &ParsedStage,
) -> StageOrigin {
    StageOrigin {
        pipeline: (pipeline + 1) as u32,
        stage: (stage + 1) as u32,
        running: spec.running.clone(),
        name: parsed.options.name.clone(),
    }
}

fn label_error(parsed: &ParsedPipeline, refs: &[LabelRef], msg: String) -> PipeError {
    let err = PipeError::parse(msg);
    let Some(r) = refs.first() else {
        return err;
    };
    let Some(pipe) = parsed.pipelines.get(r.pipeline) else {
        return err;
    };
    let Some(spec) = pipe.get(r.index) else {
        return err;
    };
    err.with_origin(stage_origin(parsed, r.pipeline, r.index, spec))
}

fn wire_label_reference(
    _builder: &mut PipelineBuilder,
    _label_defs: &HashMap<String, Vec<LabelRef>>,
    _label: &str,
    _occurrence: u32,
    _prev: &mut Option<(StageId, StreamId)>,
) -> PipeResult<()> {
    // Primary linear pass leaves label-only nodes to the cross-pipeline pass.
    *_prev = None;
    Ok(())
}

fn resolve_stage(
    registry: &StageRegistry,
    verb: &str,
    args: Vec<String>,
) -> PipeResult<pipe_core::StageFn> {
    if let Some(entry) = registry.create(verb, args.clone()) {
        return Ok(entry);
    }
    // REXX file stage: verb ends with .rexx / .rex or is REXX
    let lower = verb.to_ascii_lowercase();
    if lower.ends_with(".rexx") || lower.ends_with(".rex") || verb.eq_ignore_ascii_case("REXX") {
        return Ok(rexx_stage_stub(verb, args));
    }
    // Python file stage (non-IBM): verb ends with .py. No PYTHON keyword.
    if lower.ends_with(".py") {
        return Ok(python_stage_stub(verb, args));
    }
    Err(PipeError::parse(format!("unknown stage: {verb}")))
}

fn rexx_stage_stub(verb: &str, args: Vec<String>) -> pipe_core::StageFn {
    let verb = verb.to_string();
    Box::new(move |ctx: &mut StageCtx| {
        // Deferred to pipe-rexx via a registered factory when available.
        if let Err(e) = try_run_rexx(ctx, &verb, &args) {
            // Nested CALLPIPE already identified the failing subpipeline stage.
            if has_stage_context(&e) {
                ctx.fail(e);
            } else {
                ctx.fail(format!("REXX stage {verb}: {e}"));
            }
        }
    })
}

fn try_run_rexx(ctx: &mut StageCtx, verb: &str, args: &[String]) -> Result<(), String> {
    // Weak coupling: call into pipe_rexx helper if the CLI registered a hook.
    if let Some(hook) = REXX_HOOK.with(|h| h.borrow().clone()) {
        return hook(ctx, verb, args);
    }
    Err("no REXX runtime registered; link pipe-rexx and call pipe_rexx::install_hook()".into())
}

fn python_stage_stub(verb: &str, args: Vec<String>) -> pipe_core::StageFn {
    let verb = verb.to_string();
    Box::new(move |ctx: &mut StageCtx| {
        if let Err(e) = try_run_python(ctx, &verb, &args) {
            if has_stage_context(&e) {
                ctx.fail(e);
            } else {
                ctx.fail(format!("Python stage {verb}: {e}"));
            }
        }
    })
}

/// Run a `.py` stage if the CLI registered a hook.
pub fn invoke_python_hook(ctx: &mut StageCtx, verb: &str, args: &[String]) -> Result<(), String> {
    if let Some(hook) = PYTHON_HOOK.with(|h| h.borrow().clone()) {
        return hook(ctx, verb, args);
    }
    Err(
        "no Python runtime registered; link pipe-python and call pipe_python::install_hook()"
            .into(),
    )
}

fn try_run_python(ctx: &mut StageCtx, verb: &str, args: &[String]) -> Result<(), String> {
    invoke_python_hook(ctx, verb, args)
}

thread_local! {
    static REXX_HOOK: std::cell::RefCell<Option<RexxHook>> = const { std::cell::RefCell::new(None) };
    static PYTHON_HOOK: std::cell::RefCell<Option<PythonHook>> =
        const { std::cell::RefCell::new(None) };
    static VARPOOL_HOOK: std::cell::RefCell<Option<VarPoolHook>> =
        const { std::cell::RefCell::new(None) };
}

pub type RexxHook = std::rc::Rc<dyn Fn(&mut StageCtx, &str, &[String]) -> Result<(), String>>;
pub type PythonHook = std::rc::Rc<dyn Fn(&mut StageCtx, &str, &[String]) -> Result<(), String>>;
pub type VarPoolHook = std::rc::Rc<dyn Fn(VarPoolOp) -> Result<VarPoolResult, String>>;

#[derive(Clone)]
pub enum VarPoolOp {
    Get(String),
    Set(String, String),
    List,
}

pub enum VarPoolResult {
    Value(String),
    Success,
    List(Vec<(String, String)>),
}

/// Allow `pipe-rexx` / CLI to install the REXX stage runner without a hard cycle.
pub fn set_rexx_hook(hook: RexxHook) {
    REXX_HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

/// Allow `pipe-python` / CLI to install the Python stage runner without a hard cycle.
pub fn set_python_hook(hook: PythonHook) {
    PYTHON_HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

/// Allow `pipe-rexx` / CLI to install the REXX variable pool access without a hard cycle.
pub fn set_varpool_hook(hook: VarPoolHook) {
    VARPOOL_HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

pub(crate) fn call_varpool_hook(op: VarPoolOp) -> Result<VarPoolResult, String> {
    VARPOOL_HOOK.with(|h| {
        let hook = h.borrow();
        match hook.as_ref() {
            Some(f) => f(op),
            None => Err("REXX variable pool access requires Regina and must be called from a REXX program".to_string()),
        }
    })
}

/// Result of an independent nested PIPE (RUNPIPE, 1.1.11 pp.481–483).
pub(crate) struct IndependentRun {
    pub rc: i32,
    pub records: Vec<String>,
}

/// Issue one PIPE specification as a new pipeline set and collect intercepted
/// messages / TRACE lines. Nested errors become output records (they do not
/// fail the caller).
pub(crate) fn run_independent_spec(spec: &str, force_trace: bool) -> IndependentRun {
    fn msgid(id: &str, text: &str) -> String {
        format!("{id} {text}")
    }

    fn error_records(e: &PipeError) -> Vec<String> {
        let text = e.to_string();
        let mut lines = text.lines();
        let mut records = Vec::new();
        match lines.next() {
            Some(first) => records.push(msgid("PIPES0001E", first)),
            None => records.push(msgid("PIPES0001E", "")),
        }
        for line in lines {
            records.push(msgid("PIPES0003I", line));
        }
        records
    }

    let parsed = match parse_pipeline(spec) {
        Ok(p) => p,
        Err(e) => {
            return IndependentRun {
                rc: 1,
                records: error_records(&PipeError::parse(e.to_string())),
            };
        }
    };

    let mut builder = match build_pipeline_graph(&parsed, &StageRegistry::default()) {
        Ok(b) => b,
        Err(e) => {
            return IndependentRun {
                rc: 1,
                records: error_records(&e),
            };
        }
    };

    let trace_buf = Rc::new(RefCell::new(Vec::new()));
    let msg_buf = Rc::new(RefCell::new(Vec::new()));
    if force_trace || parsed.options.trace {
        builder.set_trace(TraceConfig::capture(Rc::clone(&trace_buf)));
    }

    let run_rc = with_pipeline_message_sink(Rc::clone(&msg_buf), || builder.run_with_return_code());

    let mut records = Vec::new();
    for line in trace_buf.borrow().iter() {
        records.push(msgid("PIPES0002I", line));
    }
    for line in msg_buf.borrow().iter() {
        records.push(msgid("PIPES0003I", line));
    }
    match run_rc {
        Ok(rc) => IndependentRun { rc, records },
        Err(e) => {
            records.extend(error_records(&e));
            IndependentRun { rc: 1, records }
        }
    }
}

/// Build a nested pipeline from a CALLPIPE specification string.
pub fn callpipe_from_spec(
    ctx: &mut StageCtx,
    spec: &str,
    registry: &StageRegistry,
) -> PipeResult<()> {
    let processing = format!("callpipe {spec}");
    let parsed = parse_pipeline(spec)
        .map_err(|e| PipeError::parse(e.to_string()).with_processing(&processing))?;
    let mut feed = false;
    let mut drain = false;
    for pipe in &parsed.pipelines {
        for s in pipe {
            match s.connector {
                Some(ConnectorKind::Input) => feed = true,
                Some(ConnectorKind::Output) => drain = true,
                None => {}
            }
        }
    }

    let connectors = CallPipeConnectors {
        feed_from_caller_input: feed,
        drain_to_caller_output: drain,
        caller_input: ctx.selected_input(),
        caller_output: ctx.selected_output(),
    };

    // Strip connector stages for the nested builder. CMS does not count
    // `*:` connectors as numbered stages.
    let mut nested_parsed = parsed.clone();
    for pipe in &mut nested_parsed.pipelines {
        pipe.retain(|s| s.connector.is_none());
    }

    let nested = build_pipeline_graph(&nested_parsed, registry)
        .map_err(|e| e.with_processing(&processing))?;

    ctx.callpipe(
        move |builder| {
            builder.extend_from(nested);
        },
        connectors,
    )
    .map_err(|e| e.with_processing(&processing))
}

/// CALLPIPE a subroutine whose beginning is unconnected and whose end drains
/// to the caller’s selected output (PREFACE / APPEND, 1.1.11 pp.263, 461).
///
/// Nested scan uses the book’s CALLPIPE defaults: separator `|`, escape `"`,
/// end-character `\`. An explicit `*:` connector is rejected.
pub(crate) fn callpipe_subroutine(
    ctx: &mut StageCtx,
    spec: &str,
    registry: &StageRegistry,
) -> PipeResult<()> {
    let options = PipeOptions {
        endchar: Some('\\'),
        separator: '|',
        escape: Some('"'),
        trace: false,
        name: None,
    };
    let processing = format!("callpipe {spec}");
    let parsed = parse_with_options(spec, options)
        .map_err(|e| PipeError::parse(e.to_string()).with_processing(&processing))?;
    for pipe in &parsed.pipelines {
        for s in pipe {
            if s.connector.is_some() {
                return Err(PipeError::parse(
                    "do not write an explicit connector on the subroutine pipeline",
                )
                .with_processing(&processing));
            }
        }
    }

    let connectors = CallPipeConnectors {
        feed_from_caller_input: false,
        drain_to_caller_output: true,
        caller_input: ctx.selected_input(),
        caller_output: ctx.selected_output(),
    };

    let nested =
        build_pipeline_graph(&parsed, registry).map_err(|e| e.with_processing(&processing))?;

    ctx.callpipe(
        move |builder| {
            builder.extend_from(nested);
        },
        connectors,
    )
    .map_err(|e| e.with_processing(&processing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn run_literal_count() {
        // Use a custom sink via DISK would need filesystem I/O; instead test locate path
        // by running literal | change through collecting with PIPELINE builder
        // via registry create.
        let reg = StageRegistry::default();
        assert!(reg.contains("LITERAL"));
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "literal",
            reg.create("LITERAL", vec!["hello".into()]).unwrap(),
        );
        let c = b.add_stage(
            "change",
            reg.create("CHANGE", vec!["/hello/".into(), "/hi/".into()])
                .unwrap(),
        );
        let s = b.add_stage(
            "sink",
            Box::new(move |ctx: &mut StageCtx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(a, c);
        b.connect_primary(c, s);
        b.run().unwrap();
        assert_eq!(*collected.borrow(), vec!["hi".to_string()]);
    }
}
