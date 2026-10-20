//! Stage-verb abbreviations from Author’s Edition 1.1.11 syntax diagrams.
//!
//! Only verbs whose diagram shows a lowercase suffix are abbreviable.
//! Operand abbreviations (ANYCASE, STRING, …) are tested with their stages.

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

#[test]
fn unique_min_4() {
    // UNIQue p.572: uniq works; uni is shorter than the uppercase prefix.
    assert!(StageRegistry::default().contains("UNIQ"));
    assert!(StageRegistry::default().contains("UNIQU"));
    assert!(!StageRegistry::default().contains("UNI"));
    assert_eq!(
        run_spec_collect("literal a a a b b b a a a c c c | split | uniq"),
        vec!["a", "b", "a", "c"]
    );
    let err = run_spec("literal a | uni | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("UNI"),
        "{err}"
    );
}

#[test]
fn truncate_min_5() {
    // TRUNCate p.289: trunc works; trun is rejected.
    let reg = StageRegistry::default();
    assert!(reg.contains("TRUNC"));
    assert!(reg.contains("TRUNCA"));
    assert!(reg.contains("TRUNCAT"));
    assert!(reg.contains("TRUNCATE"));
    assert!(!reg.contains("TRUN"));
    assert_eq!(run_spec_collect("literal abcdef | trunc 3"), vec!["abc"]);
    let err = run_spec("literal abcdef | trun 3 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("TRUN"),
        "{err}"
    );
}

#[test]
fn console_and_terminal_min_4() {
    // CONSole / TERMinal p.303.
    let reg = StageRegistry::default();
    assert!(reg.contains("CONS"));
    assert!(reg.contains("CONSO"));
    assert!(reg.contains("CONSOL"));
    assert!(reg.contains("CONSOLE"));
    assert!(!reg.contains("CON"));
    assert!(reg.contains("TERM"));
    assert!(reg.contains("TERMINAL"));
    assert!(!reg.contains("TER"));
    assert!(reg.create("CONS", vec![]).is_some());
    assert!(reg.create("TERM", vec![]).is_some());
    assert!(reg.create("CON", vec![]).is_none());
}

#[test]
fn specs_min_4() {
    // SPECs p.496. SPEC is also the Appendix A verb (exact).
    let reg = StageRegistry::default();
    assert!(reg.contains("SPEC"));
    assert!(reg.contains("SPECS"));
    assert!(!reg.contains("SPE"));
}

#[test]
fn duplicate_min_3() {
    // DUPlicate p.334: dup works; du is rejected.
    assert!(StageRegistry::default().contains("DUP"));
    assert!(!StageRegistry::default().contains("DU"));
    assert_eq!(run_spec_collect("literal a|dup"), vec!["a", "a"]);
    let err = run_spec("literal a | du | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("DU"),
        "{err}"
    );
}

#[test]
fn all_caps_verbs_are_not_invented_abbreviations() {
    // CHOP, COPY, JOIN, SPLIT, PICK, ZONE, PREFACE, APPEND, LOOKUP, … diagrams are all caps.
    let reg = StageRegistry::default();
    assert!(reg.contains("CHOP"));
    assert!(!reg.contains("CHO"));
    assert!(reg.contains("COPY"));
    assert!(!reg.contains("COP"));
    assert!(reg.contains("JOIN"));
    assert!(!reg.contains("JOI"));
    assert!(reg.contains("COMBINE"));
    assert!(!reg.contains("COMBIN"));
    assert!(!reg.contains("COMB"));
    assert!(!reg.contains("COM"));
    assert!(reg.contains("COLLATE"));
    assert!(!reg.contains("COLLAT"));
    assert!(reg.contains("MERGE"));
    assert!(!reg.contains("MERG"));
    assert!(reg.contains("DEAL"));
    assert!(!reg.contains("DEA"));
    assert!(reg.contains("GATHER"));
    assert!(!reg.contains("GATHE"));
    assert!(reg.contains("DEBLOCK"));
    assert!(!reg.contains("DEBLOC"));
    assert!(reg.contains("PARCEL"));
    assert!(!reg.contains("PARCE"));
    assert!(reg.contains("HTTPSPLIT"));
    assert!(!reg.contains("HTTPSPLI"));
    assert!(reg.contains("SPLIT"));
    assert!(!reg.contains("SPLI"));
    assert!(reg.contains("PICK"));
    assert!(!reg.contains("PIC"));
    assert!(reg.contains("ZONE"));
    assert!(!reg.contains("ZON"));
    assert!(reg.contains("PREFACE"));
    assert!(!reg.contains("PREFAC"));
    assert!(reg.contains("APPEND"));
    assert!(!reg.contains("APPEN"));
    assert!(reg.contains("LOOKUP"));
    assert!(!reg.contains("LOOKU"));
    assert!(reg.contains("ELASTIC"));
    assert!(!reg.contains("ELASTI"));
    assert!(reg.contains("GATE"));
    assert!(!reg.contains("GAT"));
    assert!(reg.contains("NOEOFBACK"));
    assert!(!reg.contains("NOEOFBAC"));
    assert!(reg.contains("EOFBACK"));
    assert!(!reg.contains("EOFBAC"));
    assert!(!reg.contains("EOF"));
    assert!(reg.contains("AGGRC"));
    assert!(!reg.contains("AGGR"));
    assert!(!reg.contains("AGG"));
    assert!(reg.contains("DELAY"));
    assert!(!reg.contains("DELA"));
    assert!(!reg.contains("DEL"));
    assert!(reg.contains("BEAT"));
    assert!(!reg.contains("BEA"));
    assert!(!reg.contains("BE"));
    assert!(reg.contains("UPDATE"));
    assert!(!reg.contains("UPDAT"));
    assert!(!reg.contains("UPD"));
    assert!(reg.contains("PIPCMD"));
    assert!(!reg.contains("PIPCM"));
    assert!(!reg.contains("PIP"));
    assert!(reg.contains("PIPESTOP"));
    assert!(!reg.contains("PIPESTO"));
    assert!(!reg.contains("PIPES"));
    assert!(reg.contains("RUNPIPE"));
    assert!(!reg.contains("RUNPIP"));
    assert!(!reg.contains("RUNP"));
    assert!(reg.contains("XLATE"));
    assert!(!reg.contains("XLAT"));
    assert!(!reg.contains("XLA"));
    assert!(reg.contains("INSERT"));
    assert!(!reg.contains("INSER"));
    assert!(!reg.contains("INS"));
    assert!(reg.contains("PAD"));
    assert!(!reg.contains("PA"));
    assert!(!reg.contains("P"));
    assert!(reg.contains("REVERSE"));
    assert!(!reg.contains("REVERS"));
    assert!(!reg.contains("REV"));
    assert!(reg.contains("RETAB"));
    assert!(!reg.contains("RETA"));
    assert!(reg.contains("UNTAB"));
    assert!(!reg.contains("UNTA"));
    assert!(reg.contains("64ENCODE"));
    assert!(!reg.contains("64ENCOD"));
    assert!(reg.contains("64DECODE"));
    assert!(!reg.contains("64DECOD"));
    assert!(reg.contains("QPENCODE"));
    assert!(!reg.contains("QPENCOD"));
    assert!(reg.contains("QPDECODE"));
    assert!(!reg.contains("QPDECOD"));
    assert!(reg.contains("SNAKE"));
    assert!(!reg.contains("SNAK"));
    assert!(!reg.contains("SNA"));
    assert!(reg.contains("FIND"));
    assert!(!reg.contains("FIN"));
    assert!(!reg.contains("FI"));
    assert!(reg.contains("NFIND"));
    assert!(!reg.contains("NFIN"));
    assert!(!reg.contains("NFI"));
    assert!(reg.contains("NOTFIND"));
    assert!(!reg.contains("NOTFIN"));
    assert!(!reg.contains("NOTF"));
    assert!(reg.contains("FRLABEL"));
    assert!(!reg.contains("FRLAB"));
    assert!(reg.contains("FROMLABEL"));
    assert!(!reg.contains("FROMLAB"));
    assert!(reg.contains("TOLABEL"));
    assert!(!reg.contains("TOLABE"));
    assert!(reg.contains("WHILELABEL"));
    assert!(!reg.contains("WHILELAB"));
    assert!(reg.contains("STRFIND"));
    assert!(!reg.contains("STRFIN"));
    assert!(reg.contains("STRNFIND"));
    assert!(!reg.contains("STRNFIN"));
    assert!(reg.contains("STRFRLABEL"));
    assert!(reg.contains("STRFROMLABEL"));
    assert!(reg.contains("STRTOLABEL"));
    assert!(!reg.contains("STRTOLA"));
    // STRWHILElabel min 8 (p.596).
    assert!(reg.contains("STRWHILE"));
    assert!(reg.contains("STRWHILELABEL"));
    assert!(!reg.contains("STRWHIL"));
    assert!(reg.contains("STRLITERAL"));
    assert!(!reg.contains("STRLITERA"));
    assert!(!reg.contains("STRLIT"));
    assert!(reg.contains("SPILL"));
    assert!(!reg.contains("SPIL"));
    assert!(reg.contains("FRTARGET"));
    assert!(!reg.contains("FRTARGE"));
    assert!(reg.contains("FROMTARGET"));
    assert!(!reg.contains("FROMTARGE"));
    assert!(reg.contains("TOTARGET"));
    assert!(!reg.contains("TOTARGE"));
    assert!(reg.contains("BETWEEN"));
    assert!(!reg.contains("BETWEE"));
    assert!(reg.contains("INSIDE"));
    assert!(!reg.contains("INSID"));
    assert!(reg.contains("OUTSIDE"));
    assert!(!reg.contains("OUTSID"));
    // NOTINSIDe min 7 (p.442); NINSIDE is the documented synonym (p.443).
    assert!(reg.contains("NOTINSI"));
    assert!(reg.contains("NOTINSIDE"));
    assert!(!reg.contains("NOTINS"));
    assert!(reg.contains("NINSIDE"));
    assert!(!reg.contains("NINSID"));
    assert!(reg.contains("NOTEOFBACK"));
    assert!(!reg.contains("NOTEOFBAC"));
    assert!(reg.contains("IF"));
    assert!(!reg.contains("I"));
    assert!(reg.contains("ABBREV"));
    assert!(!reg.contains("ABBRE"));
    assert!(!reg.contains("ABB"));
    assert!(reg.contains("VERIFY"));
    assert!(!reg.contains("VERIF"));
    assert!(!reg.contains("VER"));
    assert!(reg.contains("ALL"));
    assert!(!reg.contains("AL"));
    assert!(reg.contains("WILDCARD"));
    assert!(!reg.contains("WILDCA"));
    assert!(!reg.contains("WILD"));
    assert!(reg.contains("ESCAPE"));
    assert!(!reg.contains("ESCAP"));
    assert!(!reg.contains("ESC"));
    assert!(reg.contains("CASEI"));
    assert!(!reg.contains("CASE"));
    assert!(!reg.contains("CAS"));
    assert!(reg.contains("NOT"));
    assert!(!reg.contains("NO"));
    assert!(!reg.contains("N"));
    assert!(reg.contains("DAM"));
    assert!(!reg.contains("DA"));
    assert!(reg.contains("JUXTAPOSE"));
    assert!(!reg.contains("JUXTAPOS"));
    assert!(reg.contains("COMMAND"));
    assert!(!reg.contains("COMM"));
    // SYNChronise min 4 (p.536); SYNCHRONIZE is the documented synonym.
    assert!(reg.contains("SYNC"));
    assert!(reg.contains("SYNCHRONISE"));
    assert!(reg.contains("SYNCHRONIZE"));
    assert!(!reg.contains("SYN"));
    let err = run_spec("literal a | cho 2 | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn parser_resolves_abbreviated_verb() {
    run_spec("literal a a | split | uniq | hole").unwrap();
    run_spec("literal abcdef | trunc 3 | hole").unwrap();
    run_spec("literal a | dup | hole").unwrap();
    run_spec("literal a | sync | hole").unwrap();
    run_spec("literal abcdef | substr 2-4 | hole").unwrap();
}

#[test]
fn translate_min_5() {
    // TRANSlate p.604 synonym of XLATE. XLATE itself is all-caps (no prefix).
    let reg = StageRegistry::default();
    assert!(reg.contains("TRANS"));
    assert!(reg.contains("TRANSL"));
    assert!(reg.contains("TRANSLATE"));
    assert!(!reg.contains("TRAN"));
    assert!(reg.contains("XLATE"));
    assert!(!reg.contains("XLAT"));
    assert_eq!(run_spec_collect("literal ab|trans upper"), vec!["AB"]);
    let err = run_spec("literal ab | tran | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("TRAN"),
        "{err}"
    );
}

#[test]
fn predselect_min_7() {
    // PREDSELect p.459: predsel works; predse is rejected.
    let reg = StageRegistry::default();
    assert!(reg.contains("PREDSEL"));
    assert!(reg.contains("PREDSELECT"));
    assert!(!reg.contains("PREDSE"));
    let err = run_spec("literal a | predse | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("PREDSE"),
        "{err}"
    );
}

#[test]
fn overlay_min_4() {
    // OVERlay p.447: over works; ove is shorter than the uppercase prefix.
    let reg = StageRegistry::default();
    assert!(reg.contains("OVER"));
    assert!(reg.contains("OVERL"));
    assert!(reg.contains("OVERLA"));
    assert!(reg.contains("OVERLAY"));
    assert!(!reg.contains("OVE"));
    assert_eq!(run_spec_collect("literal hello| over"), vec!["hello"]);
    let err = run_spec("literal a | ove | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("OVE"),
        "{err}"
    );
}

#[test]
fn mime_base64_and_quoted_printable_are_exact() {
    // 64ENCODE / 64DECODE p.613 and QPENCODE / QPDECODE pp.466–467 are all-caps.
    let reg = StageRegistry::default();
    assert!(reg.contains("64ENCODE"));
    assert!(reg.contains("64DECODE"));
    assert!(reg.contains("QPENCODE"));
    assert!(reg.contains("QPDECODE"));
    assert!(!reg.contains("64ENCOD"));
    assert!(!reg.contains("64DECOD"));
    assert!(!reg.contains("QPENCOD"));
    assert!(!reg.contains("QPDECOD"));
    assert_eq!(
        run_spec_collect("literal hello| 64encode"),
        vec!["aGVsbG8="]
    );
    assert_eq!(run_spec_collect("literal a=b| qpencode"), vec!["a=3Db"]);
    let err = run_spec("literal a | 64encod | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("64ENCOD"),
        "{err}"
    );
    let err = run_spec("literal a | qpdecod | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("QPDECOD"),
        "{err}"
    );
}

#[test]
fn retab_and_untab_are_exact() {
    // RETAB p.473 and UNTAB p.575 are all-caps (no prefix).
    let reg = StageRegistry::default();
    assert!(reg.contains("RETAB"));
    assert!(reg.contains("UNTAB"));
    assert!(!reg.contains("RETA"));
    assert!(!reg.contains("UNTA"));
    assert_eq!(
        run_spec_collect("literal a  b| retab | change x09 /*/"),
        vec!["a*b"]
    );
    let err = run_spec("literal a | reta | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("RETA"),
        "{err}"
    );
    let err = run_spec("literal a | unta | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("UNTA"),
        "{err}"
    );
}

#[test]
fn space_and_tokenise_are_exact() {
    // SNAKE p.491, SPACE p.495, and TOKENISE / TOKENIZE p.558 are all-caps.
    let reg = StageRegistry::default();
    assert!(reg.contains("SNAKE"));
    assert!(!reg.contains("SNAK"));
    assert!(!reg.contains("SNA"));
    assert!(reg.contains("SPACE"));
    assert!(!reg.contains("SPAC"));
    assert!(!reg.contains("SPA"));
    assert!(reg.contains("TOKENISE"));
    assert!(reg.contains("TOKENIZE"));
    assert!(!reg.contains("TOKEN"));
    assert!(!reg.contains("TOKENI"));
    assert!(!reg.contains("TOKENIS"));
    assert!(!reg.contains("TOKENIZ"));
    assert_eq!(
        run_spec_collect("literal a b c d | split | snake 2"),
        vec!["ac", "bd"]
    );
    assert_eq!(run_spec_collect("literal a   b| space"), vec!["a b"]);
    assert_eq!(
        run_spec_collect("literal a=b| tokenize /=/"),
        vec!["a", "=", "b"]
    );
    let err = run_spec("literal a | spac | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("SPAC"),
        "{err}"
    );
    let err = run_spec("literal a | token /(/ | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn xrange_random_digest_are_exact() {
    // XRANGE p.609, RANDOM p.469, DIGEST p.326 are all-caps (no prefix).
    let reg = StageRegistry::default();
    assert!(reg.contains("XRANGE"));
    assert!(!reg.contains("XRANG"));
    assert!(reg.contains("RANDOM"));
    assert!(!reg.contains("RANDO"));
    assert!(reg.contains("DIGEST"));
    assert!(!reg.contains("DIGES"));
    assert_eq!(run_spec_collect("xrange a-c"), vec!["abc"]);
    assert_eq!(
        run_spec_collect("random 8 1 | take 1 | spec 1-* c2x")
            .first()
            .map(|s| s.len()),
        Some(8)
    );
    assert_eq!(
        run_spec_collect("literal abc| digest md5 | spec 1-* c2x")
            .first()
            .map(|s| s.len()),
        Some(32)
    );
    for spec in ["xrang a-c | hole", "rando 8 1 | hole", "diges md5 | hole"] {
        let err = run_spec(spec).unwrap_err();
        assert!(err.to_string().contains("unknown stage"), "{spec}: {err}");
    }
}

#[test]
fn register_without_abbrev_stays_exact() {
    let mut reg = StageRegistry::new();
    let hit = Rc::new(RefCell::new(false));
    let flag = Rc::clone(&hit);
    reg.register("UNIQUE", move |_| {
        let flag = Rc::clone(&flag);
        Box::new(move |ctx: &mut StageCtx| {
            *flag.borrow_mut() = true;
            while ctx.read().ok().flatten().is_some() {}
        })
    });
    assert!(reg.create("UNIQUE", vec![]).is_some());
    assert!(reg.create("UNIQ", vec![]).is_none());

    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from("x"));
        }),
    );
    let stage = b.add_stage("UNIQUE", reg.create("UNIQUE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(*hit.borrow());
}

#[test]
fn substring_min_6() {
    // SUBSTRing p.536: substr works; subst is shorter than the uppercase prefix.
    let reg = StageRegistry::default();
    assert!(reg.contains("SUBSTR"));
    assert!(reg.contains("SUBSTRI"));
    assert!(reg.contains("SUBSTRING"));
    assert!(!reg.contains("SUBST"));
    assert_eq!(run_spec_collect("literal abcdef| substr 2-4"), vec!["bcd"]);
    let err = run_spec("literal abcdef | subst 2-4 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("SUBST"),
        "{err}"
    );
}
