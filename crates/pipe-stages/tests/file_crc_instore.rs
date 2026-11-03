//! FILEBACK, FILERAND, FILEFAST, FILESLOW, FILEUPDATE, CRC, INSTORE, OUTSTORE.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{callpipe_from_spec, run_spec, with_cleared_instore, StageRegistry};

static TMP_N: AtomicU64 = AtomicU64::new(0);

fn tmp(name: &str) -> PathBuf {
    let n = TMP_N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("cms-pp-{}-{}-{}", std::process::id(), name, n))
}

fn write_lines(path: &std::path::Path, lines: &[&str]) {
    let mut body = String::new();
    for line in lines {
        body.push_str(line);
        body.push('\n');
    }
    std::fs::write(path, body).unwrap();
}

fn file_lines(path: &std::path::Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.is_empty() => Vec::new(),
        Ok(text) => {
            let mut recs: Vec<String> = text
                .split('\n')
                .map(|s| s.trim_end_matches('\r').to_string())
                .collect();
            if recs.last().is_some_and(|s| s.is_empty()) {
                recs.pop();
            }
            recs
        }
        Err(_) => Vec::new(),
    }
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

fn run_bytes(verb: &str, args: &[&str], records: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let recs: Vec<String> = records.iter().map(|s| (*s).to_string()).collect();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in recs {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("{verb}")),
    );
    let snk = b.add_stage(
        "snk",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.as_bytes().to_vec());
            }
        }),
    );
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn crc16i(data: &[u8]) -> Vec<u8> {
    let mut crc = 0x8000u16;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x8005;
            } else {
                crc <<= 1;
            }
        }
    }
    crc.to_be_bytes().to_vec()
}

fn update_rec(n: u64, data: &str) -> String {
    format!("{n:>10}{data}")
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn fileback_reads_last_first() {
    let path = tmp("fileback.txt");
    write_lines(&path, &["One desk", "Two chairs", "Three tables"]);
    let out = run_spec_collect(&format!("fileback {}", path.display()));
    assert_eq!(out, vec!["Three tables", "Two chairs", "One desk"]);
    let via_syn = run_spec_collect(&format!("diskback {}", path.display()));
    assert_eq!(via_syn, out);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileback_empty_file() {
    let path = tmp("fileback-empty.txt");
    std::fs::write(&path, "").unwrap();
    let out = run_spec_collect(&format!("fileback {}", path.display()));
    assert!(out.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileback_missing_file_is_an_error() {
    let path = tmp("fileback-missing.txt");
    let _ = std::fs::remove_file(&path);
    let err = err_of(&format!("fileback {} | hole", path.display()));
    assert!(
        err.contains("file read") || err.contains("No such") || err.contains("cannot find"),
        "{err}"
    );
}

#[test]
fn fileback_must_be_first() {
    let path = tmp("fileback-not-first.txt");
    write_lines(&path, &["x"]);
    let err = err_of(&format!("literal z | fileback {} | hole", path.display()));
    assert!(err.contains("first stage"), "{err}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileback_take_last_instore_reverse() {
    with_cleared_instore(|| {
        let path = tmp("fileback-take.txt");
        write_lines(&path, &["one", "two", "three", "four"]);
        let out = run_spec_collect(&format!(
            "fileback {} | take last 2 | instore reverse | outstore",
            path.display()
        ));
        assert_eq!(out, vec!["one", "two"]);
        let last_two = run_spec_collect(&format!(
            "fileback {} | take 2 | instore reverse | outstore",
            path.display()
        ));
        assert_eq!(last_two, vec!["three", "four"]);
        let _ = std::fs::remove_file(&path);
    });
}

#[test]
fn filerand_operand_and_input_ranges() {
    let path = tmp("filerand.txt");
    write_lines(
        &path,
        &[
            "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "r13",
            "r14", "r15", "r16",
        ],
    );
    let out = run_spec_collect(&format!("filerand {} 3 10 15.2", path.display()));
    assert_eq!(out, vec!["r3", "r10", "r15", "r16"]);
    let mixed = run_spec_collect(&format!("literal 4-5| filerand {} 3", path.display()));
    assert_eq!(mixed, vec!["r3", "r4", "r5"]);
    let recno = run_spec_collect(&format!("filerand {} recno 2", path.display()));
    assert_eq!(recno, vec!["         2r2"]);
    let any = run_spec_collect(&format!("filerand {} 3 10 1 2 12 5 4", path.display()));
    assert_eq!(any, vec!["r3", "r10", "r1", "r2", "r12", "r5", "r4"]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn filerand_first_without_range_writes_nothing() {
    let path = tmp("filerand-none.txt");
    write_lines(&path, &["a", "b"]);
    let out = run_spec_collect(&format!("filerand {}", path.display()));
    assert!(out.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn filerand_blocked_rejected() {
    let path = tmp("filerand-block.txt");
    write_lines(&path, &["a"]);
    let err = err_of(&format!("filerand {} blocked 1 | hole", path.display()));
    assert!(err.to_ascii_uppercase().contains("BLOCK"), "{err}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn filefast_matches_disk() {
    let path = tmp("filefast.txt");
    let _ = std::fs::remove_file(&path);
    assert!(run_spec_collect(&format!("filefast {}", path.display())).is_empty());
    write_lines(&path, &["old"]);
    run_spec(&format!("literal new| filefast {} | hole", path.display())).unwrap();
    assert_eq!(file_lines(&path), vec!["old", "new"]);
    let via = run_spec_collect(&format!("diskfast {}", path.display()));
    assert_eq!(via, vec!["old", "new"]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileslow_from_reads_and_writes() {
    let path = tmp("fileslow.txt");
    write_lines(&path, &["a", "b", "c", "d", "e"]);
    let out = run_spec_collect(&format!("fileslow {} from 3", path.display()));
    assert_eq!(out, vec!["c", "d", "e"]);
    run_spec(&format!(
        "literal X| fileslow {} from 2 | hole",
        path.display()
    ))
    .unwrap();
    assert_eq!(file_lines(&path), vec!["a", "X", "c", "d", "e"]);
    run_spec(&format!(
        "literal Y| fileslow {} from 6 | hole",
        path.display()
    ))
    .unwrap();
    assert_eq!(file_lines(&path), vec!["a", "X", "c", "d", "e", "Y"]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileslow_without_from_is_filefast() {
    let path = tmp("fileslow-default.txt");
    let _ = std::fs::remove_file(&path);
    run_spec(&format!("literal z| fileslow {} | hole", path.display())).unwrap();
    assert_eq!(file_lines(&path), vec!["z"]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileupdate_replaces_numbered_records() {
    let path = tmp("fileupdate.txt");
    write_lines(
        &path,
        &[
            "One desk",
            "Two chairs",
            "Three tables",
            "Four pencils",
            "Five vases",
            "Six books",
            "Five pens",
        ],
    );
    let five = update_rec(5, "Four vases");
    let seven = update_rec(7, "Four pens");
    run_spec(&format!(
        "literal {five}| literal {seven}| fileupdate {} | hole",
        path.display()
    ))
    .unwrap();
    assert_eq!(
        file_lines(&path),
        vec![
            "One desk",
            "Two chairs",
            "Three tables",
            "Four pencils",
            "Four vases",
            "Six books",
            "Four pens",
        ]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileupdate_with_spec_recno() {
    let path = tmp("fileupdate-spec.txt");
    write_lines(&path, &["alpha", "Five vases", "gamma"]);
    run_spec(&format!(
        "< {} | spec recno 1.10 right 1-* next | locate /Five/ | spec 1-* 1 /Four/ 11 | fileupdate {} | hole",
        path.display(),
        path.display()
    ))
    .unwrap();
    assert_eq!(file_lines(&path), vec!["alpha", "Four vases", "gamma"]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileupdate_must_not_be_first() {
    let path = tmp("fileupdate-first.txt");
    write_lines(&path, &["x"]);
    let err = err_of(&format!("fileupdate {} | hole", path.display()));
    assert!(err.contains("first stage"), "{err}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn fileupdate_short_record_is_an_error() {
    let path = tmp("fileupdate-short.txt");
    write_lines(&path, &["x"]);
    let err = err_of(&format!(
        "literal 3 hello| fileupdate {} | hole",
        path.display()
    ));
    assert!(err.contains("11"), "{err}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn crc_writes_two_byte_checksum() {
    let got = run_bytes("CRC", &[], &["ab", "c"]);
    assert_eq!(got, vec![crc16i(b"abc")]);
    let empty = run_bytes("CRC", &["CRC16I"], &[]);
    assert_eq!(empty, vec![crc16i(b"")]);
}

#[test]
fn crc_append_copies_then_checksum() {
    let got = run_bytes("CRC", &["APPEND"], &["ab", "c"]);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], b"ab");
    assert_eq!(got[1], b"c");
    assert_eq!(got[2], crc16i(b"abc"));
}

#[test]
fn crc_secondary_gets_checksum() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let recs = ["hello".to_string()];
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in recs {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
            }
        }),
    );
    let crc = b.add_stage("CRC", reg.create("CRC", vec![]).unwrap());
    let po = b.add_stage(
        "po",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                p.borrow_mut().push(r.as_bytes().to_vec());
            }
        }),
    );
    let so = b.add_stage(
        "so",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                s.borrow_mut().push(r.as_bytes().to_vec());
            }
        }),
    );
    b.connect_primary(src, crc);
    b.connect_primary(crc, po);
    b.connect(crc, StreamId::SECONDARY, so, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec![b"hello".to_vec()]);
    assert_eq!(*secondary.borrow(), vec![crc16i(b"hello")]);
}

#[test]
fn instore_outstore_preserves_order() {
    with_cleared_instore(|| {
        let out = run_spec_collect("literal a|literal b|literal c| instore | outstore");
        assert_eq!(out, vec!["c", "b", "a"]);
    });
}

#[test]
fn instore_reverse_words() {
    with_cleared_instore(|| {
        let out = run_spec_collect(
            "literal backwards are words these | split | instore reverse | outstore | join * / /",
        );
        assert_eq!(out, vec!["these words are backwards"]);
    });
}

#[test]
fn outstore_rejects_a_non_token() {
    with_cleared_instore(|| {
        let err = err_of("literal hello| outstore | hole");
        assert!(err.contains("not an INSTORE token"), "{err}");
    });
}

#[test]
fn instore_token_is_consumed() {
    with_cleared_instore(|| {
        let out = run_spec_collect("literal a|literal b| instore | outstore");
        assert_eq!(out, vec!["b", "a"]);
        let err = err_of("literal leftover| outstore | hole");
        assert!(err.contains("not an INSTORE token"), "{err}");
    });
}

#[test]
fn instore_works_across_callpipe() {
    with_cleared_instore(|| {
        let out = Rc::new(RefCell::new(Vec::new()));
        let o = Rc::clone(&out);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx: &mut StageCtx| {
                for rec in ["one two three"] {
                    if ctx.write(Record::from(rec)).is_err() {
                        break;
                    }
                }
            }),
        );
        let caller = b.add_stage(
            "caller",
            Box::new(move |ctx: &mut StageCtx| {
                callpipe_from_spec(
                    ctx,
                    "*: | split | instore reverse | outstore | *:",
                    &StageRegistry::default(),
                )
                .unwrap();
            }),
        );
        let snk = b.add_stage(
            "snk",
            Box::new(move |ctx: &mut StageCtx| {
                while let Ok(Some(r)) = ctx.read() {
                    o.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(src, caller);
        b.connect_primary(caller, snk);
        b.run().unwrap();
        assert_eq!(*out.borrow(), vec!["three", "two", "one"]);
    });
}

#[test]
fn new_verbs_are_registered() {
    let reg = StageRegistry::default();
    for name in [
        "FILEBACK",
        "DISKBACK",
        "FILERAND",
        "DISKRAND",
        "FILEFAST",
        "DISKFAST",
        "FILESLOW",
        "DISKSLOW",
        "FILEUPDATE",
        "DISKUPDATE",
        "CRC",
        "INSTORE",
        "OUTSTORE",
    ] {
        assert!(reg.contains(name), "{name}");
    }
    assert!(reg.contains("FILEF"));
    assert!(!reg.contains("FILE"));
    assert!(reg.contains("FILEUPDA"));
    assert!(!reg.contains("FILEUPD"));
    assert!(!reg.contains("CR"));
    assert!(!reg.contains("INSTOR"));
    let names = reg.registered_names();
    let synonyms = [
        "TERMINAL",
        "CMD",
        "NOTLOCATE",
        "3WAY",
        "NINSIDE",
        "TRUNCATE",
        "SYNCHRONIZE",
        "NOTEOFBACK",
        "TRANSLATE",
        "TOKENIZE",
        "NOTFIND",
        "FROMLABEL",
        "STRFROMLABEL",
        "FROMTARGET",
        "SPECS",
        "DISKBACK",
        "DISKRAND",
        "DISKFAST",
        "DISKSLOW",
        "DISKUPDATE",
    ];
    let extensions = ["JSON", "CSV"];
    let primaries: Vec<_> = names
        .iter()
        .copied()
        .filter(|n| !synonyms.contains(n) && !extensions.contains(n))
        .collect();
    assert_eq!(
        primaries.len(),
        128,
        "registry primaries changed: {primaries:?}"
    );
    assert!(reg.contains("JSON"));
    assert!(reg.contains("CSV"));
}
