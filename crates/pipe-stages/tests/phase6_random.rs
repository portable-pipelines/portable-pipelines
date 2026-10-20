//! RANDOM — Author's Edition 1.1.11 pp.469–470.
//!
//! Four-byte binary records. Tests are seeded; they do not assert IBM’s
//! undocumented generator sequences.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

fn sink_bytes(out: Rc<RefCell<Vec<Vec<u8>>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            out.borrow_mut().push(r.as_bytes().to_vec());
        }
    })
}

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

fn take_random(args: &[&str], n: usize) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage(
        "RANDOM",
        reg.create("RANDOM", args.iter().map(|a| (*a).to_string()).collect())
            .expect("RANDOM"),
    );
    let take = b.add_stage(
        "TAKE",
        reg.create("TAKE", vec![n.to_string()]).expect("TAKE"),
    );
    let snk = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(stage, take);
    b.connect_primary(take, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

fn u32_be(bytes: &[u8]) -> u32 {
    assert_eq!(bytes.len(), 4);
    u32::from_be_bytes(bytes.try_into().unwrap())
}

#[test]
fn four_byte_records_with_seed() {
    let recs = take_random(&["*", "1"], 3);
    assert_eq!(recs.len(), 3);
    for r in &recs {
        assert_eq!(r.len(), 4);
    }
}

#[test]
fn same_seed_same_sequence() {
    let a = take_random(&["*", "42"], 5);
    let b = take_random(&["*", "42"], 5);
    assert_eq!(a, b);
    assert_eq!(
        run_spec_collect("random * 42 | take 5 | spec 1-* c2x"),
        run_spec_collect("random * 42 | take 5 | spec 1-* c2x")
    );
}

#[test]
fn different_seeds_differ() {
    let a = take_random(&["*", "1"], 4);
    let b = take_random(&["*", "2"], 4);
    assert_ne!(a, b);
}

#[test]
fn modulus_restricts_values() {
    // p.470: modulus is the remainder; output numbers are positive (0..m-1).
    let recs = take_random(&["7", "1"], 20);
    assert_eq!(recs.len(), 20);
    for r in &recs {
        let v = u32_be(r);
        assert!(v < 7, "{v}");
        assert_eq!(&r[..3], &[0, 0, 0]);
    }
}

#[test]
fn star_placeholders() {
    let with_mod = take_random(&["7", "*"], 3);
    assert_eq!(with_mod.len(), 3);
    for r in &with_mod {
        assert!(u32_be(r) < 7);
    }
    let no_mod = take_random(&["*", "9"], 2);
    assert_eq!(no_mod.len(), 2);
    assert_eq!(no_mod, take_random(&["*", "9"], 2));
}

#[test]
fn negative_seed() {
    let a = take_random(&["*", "-1"], 3);
    let b = take_random(&["*", "-1"], 3);
    assert_eq!(a, b);
    assert_ne!(a, take_random(&["*", "1"], 3));
}

#[test]
fn first_number_is_modulus_not_seed() {
    // `random 7` is modulus 7 (clock seed); `random * 7` is seed 7.
    let seeded = take_random(&["*", "7"], 5);
    assert_eq!(seeded, take_random(&["*", "7"], 5));
    let modulo = take_random(&["7", "1"], 5);
    for r in &modulo {
        assert!(u32_be(r) < 7);
    }
}

#[test]
fn spec_form_seeded() {
    let out = run_spec_collect("random 10 1 | take 4 | spec 1-* c2x");
    assert_eq!(out.len(), 4);
    for hex in &out {
        assert_eq!(hex.len(), 8);
        let v = u32::from_str_radix(hex, 16).unwrap();
        assert!(v < 10, "{hex}");
    }
    assert_eq!(out, run_spec_collect("random 10 1 | take 4 | spec 1-* c2x"));
}

#[test]
fn must_be_first() {
    let err = err_of("literal a | random 7 1 | hole");
    assert!(err.contains("must be a first stage"), "{err}");
}

#[test]
fn modulus_must_be_positive() {
    let err = err_of("random 0 1 | hole");
    assert!(err.contains("positive") || err.contains("modulus"), "{err}");
    let err = err_of("random -1 | hole");
    assert!(
        err.contains("unrecognized") || err.contains("modulus") || err.contains("positive"),
        "{err}"
    );
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "random 7 1 extra | hole",
        "random SEED 1 | hole",
        "random 7 FIRST | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("RANDOM"));
    assert!(!reg.contains("RANDO"));
    assert!(!reg.contains("RAND"));
    let err = err_of("rando 7 1 | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("RANDO"),
        "{err}"
    );
}

#[test]
fn stops_when_output_severs() {
    let recs = take_random(&["*", "1"], 1);
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].len(), 4);
}

#[test]
fn no_args_with_take_writes_full_width() {
    // Clock seed: only check record shape, not values.
    let out = run_spec_collect("random | take 2 | spec 1-* c2x");
    assert_eq!(out.len(), 2);
    for hex in &out {
        assert_eq!(hex.len(), 8);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
