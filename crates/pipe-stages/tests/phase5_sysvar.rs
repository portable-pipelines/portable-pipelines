//! SYSVAR — host and environment variables.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, StageCtx};
use pipe_stages::{run_spec, StageRegistry};

fn run_sysvar(args: &[&str]) -> Vec<String> {
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "SYSVAR",
        reg.create("SYSVAR", args.iter().map(|a| (*a).to_string()).collect())
            .expect("SYSVAR"),
    );
    let snk = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn os_arch_pid_are_stable() {
    assert_eq!(run_sysvar(&["OS"]), vec![std::env::consts::OS.to_string()]);
    assert_eq!(
        run_sysvar(&["ARCH"]),
        vec![std::env::consts::ARCH.to_string()]
    );
    assert_eq!(run_sysvar(&["PID"]), vec![std::process::id().to_string()]);
}

#[test]
fn named_env_round_trip() {
    let key = "PIPE_SYSVAR_TEST_KEY";
    let val = "pipe-sysvar-value";
    std::env::set_var(key, val);
    assert_eq!(run_sysvar(&[key]), vec![val.to_string()]);
    std::env::remove_var(key);
    assert_eq!(run_sysvar(&[key]), vec!["".to_string()]);
}

#[test]
fn dump_includes_set_variable() {
    let key = "PIPE_SYSVAR_DUMP_KEY";
    std::env::set_var(key, "dump-me");
    let out = run_sysvar(&[]);
    assert!(
        out.iter().any(|l| l == "PIPE_SYSVAR_DUMP_KEY dump-me"),
        "missing dump line in {out:?}"
    );
    std::env::remove_var(key);
}

#[test]
fn must_be_first() {
    let err = run_spec("literal x | sysvar OS | hole")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("must be first") || err.contains("may not be connected"),
        "{err}"
    );
}

#[test]
fn several_names_one_each() {
    let out = run_sysvar(&["OS", "ARCH"]);
    assert_eq!(
        out,
        vec![
            std::env::consts::OS.to_string(),
            std::env::consts::ARCH.to_string()
        ]
    );
}
