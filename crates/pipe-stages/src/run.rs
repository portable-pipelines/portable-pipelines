use std::collections::HashMap;

use pipe_core::{
    CallPipeConnectors, PipeError, PipeResult, PipelineBuilder, StageCtx, StageId, StreamId,
    TraceConfig,
};
use pipe_parser::{parse_pipeline, parse_with_options, ConnectorKind, ParsedPipeline, PipeOptions};

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
    let mut builder = PipelineBuilder::new();
    if force_trace || parsed.options.trace {
        builder.set_trace(TraceConfig::stderr());
    }
    let mut stage_ids: Vec<Vec<Option<StageId>>> = Vec::new();

    // label -> list of (pipeline_idx, stage_idx_in_pipeline, StageId, occurrence)
    let mut label_defs: HashMap<String, Vec<LabelRef>> = HashMap::new();

    for (pi, pipe) in parsed.pipelines.iter().enumerate() {
        let mut ids = Vec::new();
        for (si, stage) in pipe.iter().enumerate() {
            if stage.connector.is_some() {
                // Connectors are handled by CALLPIPE wrappers; in top-level
                // specs a lone `*:` is an error for now.
                return Err(PipeError::parse(
                    "`*:` connectors are only valid inside CALLPIPE/ADDPIPE nested specs",
                ));
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
            let entry = resolve_stage(registry, &stage.verb, args)?;
            let id = builder.add_stage(stage.verb.clone(), entry);
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
            .ok_or_else(|| PipeError::parse(format!("label {label} has no definition")))?;
        let def_id = def
            .stage_id
            .ok_or_else(|| PipeError::parse(format!("label {label} definition has no stage")))?;

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
    Err(PipeError::parse(format!("unknown stage: {verb}")))
}

fn rexx_stage_stub(verb: &str, args: Vec<String>) -> pipe_core::StageFn {
    let verb = verb.to_string();
    Box::new(move |ctx: &mut StageCtx| {
        // Deferred to pipe-rexx via a registered factory when available.
        if let Err(e) = try_run_rexx(ctx, &verb, &args) {
            ctx.fail(format!("REXX stage {verb}: {e}"));
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

thread_local! {
    static REXX_HOOK: std::cell::RefCell<Option<RexxHook>> = const { std::cell::RefCell::new(None) };
}

pub type RexxHook = std::rc::Rc<dyn Fn(&mut StageCtx, &str, &[String]) -> Result<(), String>>;

/// Allow `pipe-rexx` / CLI to install the REXX stage runner without a hard cycle.
pub fn set_rexx_hook(hook: RexxHook) {
    REXX_HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

/// Build a nested pipeline from a CALLPIPE specification string.
pub fn callpipe_from_spec(
    ctx: &mut StageCtx,
    spec: &str,
    registry: &StageRegistry,
) -> PipeResult<()> {
    let parsed = parse_pipeline(spec).map_err(|e| PipeError::parse(e.to_string()))?;
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

    // Strip connector stages for the nested builder.
    let mut nested_parsed = parsed.clone();
    for pipe in &mut nested_parsed.pipelines {
        pipe.retain(|s| s.connector.is_none());
    }

    ctx.callpipe(
        |builder| {
            if let Err(e) = populate_builder(builder, &nested_parsed, registry) {
                panic!("CALLPIPE build: {e}");
            }
        },
        connectors,
    )
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
    let parsed = parse_with_options(spec, options).map_err(|e| PipeError::parse(e.to_string()))?;
    for pipe in &parsed.pipelines {
        for s in pipe {
            if s.connector.is_some() {
                return Err(PipeError::parse(
                    "do not write an explicit connector on the subroutine pipeline",
                ));
            }
        }
    }

    let connectors = CallPipeConnectors {
        feed_from_caller_input: false,
        drain_to_caller_output: true,
        caller_input: ctx.selected_input(),
        caller_output: ctx.selected_output(),
    };

    ctx.callpipe(
        |builder| {
            if let Err(e) = populate_builder(builder, &parsed, registry) {
                panic!("CALLPIPE build: {e}");
            }
        },
        connectors,
    )
}

fn populate_builder(
    builder: &mut PipelineBuilder,
    parsed: &ParsedPipeline,
    registry: &StageRegistry,
) -> PipeResult<()> {
    // Simplified: single linear pipeline only for CALLPIPE nested body.
    let pipe = parsed
        .pipelines
        .first()
        .ok_or_else(|| PipeError::parse("empty CALLPIPE"))?;
    let mut prev: Option<StageId> = None;
    for stage in pipe {
        if stage.verb.is_empty() {
            continue;
        }
        let args: Vec<String> = stage.args.iter().map(|a| a.as_str().to_string()).collect();
        let entry = resolve_stage(registry, &stage.verb, args)?;
        let id = builder.add_stage(stage.verb.clone(), entry);
        if let Some(p) = prev {
            builder.connect_primary(p, id);
        }
        prev = Some(id);
    }
    Ok(())
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
