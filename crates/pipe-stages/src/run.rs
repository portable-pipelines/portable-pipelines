use std::collections::HashMap;

use pipe_core::{
    CallPipeConnectors, PipeError, PipeResult, PipelineBuilder, StageCtx, StageId, StreamId,
    TraceConfig,
};
use pipe_parser::{parse_pipeline, ConnectorKind, ParsedPipeline};

use crate::registry::StageRegistry;

/// Parse and run a PIPE specification with the default built-in registry.
pub fn run_spec(spec: &str) -> PipeResult<()> {
    run_spec_with_trace(spec, false)
}

/// Like [`run_spec`], but `force_trace` enables TRACE even without `(TRACE)` in the spec.
pub fn run_spec_with_trace(spec: &str, force_trace: bool) -> PipeResult<()> {
    let parsed = parse_pipeline(spec).map_err(|e| PipeError::parse(e.to_string()))?;
    run_parsed_with_trace(&parsed, &StageRegistry::default(), force_trace)
}

/// Build a dispatcher graph from a parsed specification and run it.
pub fn run_parsed(parsed: &ParsedPipeline, registry: &StageRegistry) -> PipeResult<()> {
    run_parsed_with_trace(parsed, registry, false)
}

/// Build and run, optionally forcing TRACE on.
pub fn run_parsed_with_trace(
    parsed: &ParsedPipeline,
    registry: &StageRegistry,
    force_trace: bool,
) -> PipeResult<()> {
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
                // specs a lone `*` is an error for now.
                return Err(PipeError::parse(
                    "`*` connectors are only valid inside CALLPIPE/ADDPIPE nested specs",
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
                // Find the next real stage after this label-only slot in its pipeline.
                let ids = &stage_ids[r.pipeline];
                if let Some(next_id) = ids.iter().skip(r.index + 1).flatten().next() {
                    builder.connect(def_id, stream, *next_id, StreamId::PRIMARY);
                } else if let Some(prev_id) = ids.iter().take(r.index).rev().flatten().next() {
                    // Trailing label: connect previous stage into secondary input of def.
                    builder.connect(*prev_id, StreamId::PRIMARY, def_id, stream);
                }
            }
        }
    }

    builder.run()
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

fn resolve_stage(registry: &StageRegistry, verb: &str, args: Vec<String>) -> PipeResult<pipe_core::StageFn> {
    if let Some(entry) = registry.create(verb, args.clone()) {
        return Ok(entry);
    }
    // REXX file stage: verb ends with .REX / .rexx or is REXX
    let lower = verb.to_ascii_lowercase();
    if lower.ends_with(".rex") || lower.ends_with(".rexx") || verb.eq_ignore_ascii_case("REXX") {
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
        // Use a custom sink via FILE would need disk; instead test locate path
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
            reg.create(
                "CHANGE",
                vec!["/hello/".into(), "/hi/".into()],
            )
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
