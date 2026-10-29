//! SUBCOM — named subcommand environment (host shell subset).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
use pipe_stages::{run_spec, StageRegistry};

fn run_spec_collect(spec: &str) -> Vec<String> {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let sink_out = Rc::clone(&sink_out);
        move |_| pipe_stages::collect_to(Rc::clone(&sink_out))
    });
    let full = format!("{spec} | >>COLLECT");
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn emit(records: &[&str]) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<String> = records.iter().map(|s| (*s).to_string()).collect();
    Box::new(move |ctx| {
        for rec in records {
            if ctx.write(Record::from(rec)).is_err() {
                break;
            }
        }
    })
}

fn run_subcom(env: &str, extra: &[&str], input: &[&str]) -> Vec<String> {
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut args = vec![env.to_string()];
    args.extend(extra.iter().map(|s| (*s).to_string()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage("SUBCOM", reg.create("SUBCOM", args).expect("SUBCOM"));
    let snk = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    if !input.is_empty() {
        let src = b.add_stage("src", emit(input));
        b.connect_primary(src, stage);
    }
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn command_environment_runs_echo() {
    assert_eq!(
        run_subcom("COMMAND", &["echo", "hello"], &[]),
        vec!["hello"]
    );
    assert_eq!(run_subcom("CMD", &["echo", "hi"], &[]), vec!["hi"]);
    assert_eq!(run_subcom("SYSTEM", &[], &["echo one"]), vec!["one"]);
}

#[test]
fn blank_input_records_are_discarded() {
    assert_eq!(
        run_subcom("SH", &[], &["", "   ", "echo kept"]),
        vec!["kept"]
    );
}

#[test]
fn unknown_environment_is_an_error() {
    let err = run_spec("subcom cms query | hole").unwrap_err().to_string();
    assert!(
        err.contains("not implemented") || err.contains("SUBCOM"),
        "{err}"
    );
    let err = run_spec("subcom | hole").unwrap_err().to_string();
    assert!(err.contains("environment required"), "{err}");
}

#[test]
fn empty_input_after_operand_command() {
    assert_eq!(run_subcom("COMMAND", &["echo", "only"], &[]), vec!["only"]);
}

#[test]
fn literal_echo_omits_space_before_pipe() {
    // Same shape as the harvested example. A space before `|` is a trailing
    // blank in LITERAL; Windows ECHO would print it.
    assert_eq!(
        run_spec_collect("literal echo hello| subcom command"),
        vec!["hello"]
    );
}
