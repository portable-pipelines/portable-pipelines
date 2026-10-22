//! ASA print-report showcase: return codes, host-file sink, and no-stall size.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn pipe_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pipe"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn python_available_or_skip() -> bool {
    if pipe_python::python_available() {
        return true;
    }
    if pipe_python::pipe_require_python() {
        panic!(
            "PIPE_REQUIRE_PYTHON=1 but Python 3.10+ was not discovered ({})",
            pipe_python::discovery_hint()
        );
    }
    false
}

fn require_regina() {
    if matches!(std::env::var("PIPE_REQUIRE_REGINA").as_deref(), Ok("1")) {
        assert_eq!(
            pipe_rexx::preferred_engine(),
            pipe_rexx::RexxEngine::Regina,
            "PIPE_REQUIRE_REGINA=1 but Regina was not discovered/loaded ({})",
            pipe_rexx::discovery_hint()
        );
    }
}

fn host_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn control_total_spec(report: &str, jsonl: Option<&str>) -> String {
    let json_sink = jsonl.map(|p| format!(" | > {p}")).unwrap_or_default();
    format!(
        "(ENDCHAR ?) < {report} | nfind 1| t: locate / TOTAL / | insert /T/ | k: faninany | examples/showcase/ctltotal.rexx | o: fanin 1 0 | console ? t: | verify 3.3 /0123456789/ | f: fanout | spec 2.6 strip 1 x09 next 10.7 strip next x09 next 19.18 strip next x09 next 39.5 strip next x09 next 46.10 strip next | examples/showcase/tsv2json.py branch region rep units:int amount_cents:cents{json_sink} | o: ? f: | insert /D/ | k:"
    )
}

fn run_pipe(spec: &str) -> std::process::Output {
    pipe_bin()
        .current_dir(repo_root())
        .arg(spec)
        .output()
        .expect("run pipe")
}

fn stdout_lines(out: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn control_total_ok_is_rc_zero() {
    if !python_available_or_skip() {
        return;
    }
    require_regina();
    let out = run_pipe(&control_total_spec(
        "examples/showcase/sales-report.txt",
        None,
    ));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines = stdout_lines(&out);
    assert_eq!(
        lines.last().map(String::as_str),
        Some("control total OK: 5 rows, 48 units, 5760.00")
    );
}

#[test]
fn control_total_mismatch_is_rc_eight() {
    if !python_available_or_skip() {
        return;
    }
    require_regina();
    let out = run_pipe(&control_total_spec(
        "examples/showcase/sales-report-bad.txt",
        None,
    ));
    assert_eq!(
        out.status.code(),
        Some(8),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines = stdout_lines(&out);
    assert_eq!(
        lines.last().map(String::as_str),
        Some("CONTROL TOTAL MISMATCH: report says 48 5760.00, detail adds up to 48 5770.00")
    );
}

#[test]
fn control_total_writes_jsonl() {
    if !python_available_or_skip() {
        return;
    }
    require_regina();
    let dir = std::env::temp_dir().join("cms-pipelines-showcase-jsonl");
    let _ = std::fs::create_dir_all(&dir);
    let jsonl = dir.join("sales.jsonl");
    let _ = std::fs::remove_file(&jsonl);
    let spec = control_total_spec(
        "examples/showcase/sales-report.txt",
        Some(&host_path(&jsonl)),
    );
    let out = run_pipe(&spec);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&jsonl).expect("sales.jsonl");
    let file_lines: Vec<&str> = text.lines().collect();
    assert_eq!(file_lines.len(), 5, "jsonl={text:?}");
    assert!(file_lines[0].contains("\"amount_cents\": 144000"), "{text}");
    let console = stdout_lines(&out);
    assert_eq!(console.len(), 6);
    assert_eq!(
        console.last().map(String::as_str),
        Some("control total OK: 5 rows, 48 units, 5760.00")
    );
}

#[test]
fn control_total_3000_rows_does_not_stall() {
    if !python_available_or_skip() {
        return;
    }
    require_regina();
    let dir = std::env::temp_dir().join("cms-pipelines-showcase-3000");
    let _ = std::fs::create_dir_all(&dir);
    let report = dir.join("sales-3000.txt");
    write_asa_report(&report, 3000, 1, 100);
    let spec = control_total_spec(&host_path(&report), None);
    let out = run_pipe(&spec);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("stalled"),
        "3000-row report stalled: {stderr}"
    );
    assert_eq!(out.status.code(), Some(0), "stderr={stderr}");
    let lines = stdout_lines(&out);
    assert_eq!(
        lines.last().map(String::as_str),
        Some("control total OK: 3000 rows, 3000 units, 3000.00")
    );
    assert_eq!(lines.len(), 3001);
}

fn write_asa_report(path: &Path, rows: usize, units: i64, cents: i64) {
    let mut buf = Vec::new();
    writeln!(
        buf,
        "1ACME CORP                 SALES BY BRANCH                    PAGE    1"
    )
    .unwrap();
    writeln!(buf, " RUN DATE 2026-10-21").unwrap();
    writeln!(
        buf,
        " BRANCH  REGION   REP                 UNITS      AMOUNT"
    )
    .unwrap();
    writeln!(
        buf,
        " ------  -------  ------------------  -----  ----------"
    )
    .unwrap();
    for i in 0..rows {
        let branch = format!("B{:03}", i % 1000);
        let rec = asa_detail(&branch, "NORTH", "J. SMITH", units, cents);
        writeln!(buf, "{rec}").unwrap();
    }
    let total_units = units * rows as i64;
    let total_cents = cents * rows as i64;
    writeln!(
        buf,
        " ------  -------  ------------------  -----  ----------"
    )
    .unwrap();
    writeln!(buf, "{}", asa_total(total_units, total_cents)).unwrap();
    std::fs::write(path, buf).expect("write report");
}

fn asa_detail(branch: &str, region: &str, rep: &str, units: i64, cents: i64) -> String {
    let mut rec = vec![b' '; 55];
    rec[0] = b' ';
    put(&mut rec, 1, 6, branch.as_bytes());
    put(&mut rec, 9, 7, region.as_bytes());
    put(&mut rec, 18, 18, rep.as_bytes());
    put(&mut rec, 38, 5, format!("{units:>5}").as_bytes());
    put(&mut rec, 45, 10, format!("{:>10}", money(cents)).as_bytes());
    String::from_utf8(rec).expect("asa detail")
}

fn asa_total(units: i64, cents: i64) -> String {
    let mut rec = vec![b' '; 55];
    rec[0] = b' ';
    put(&mut rec, 9, 7, b"TOTAL");
    put(&mut rec, 38, 5, format!("{units:>5}").as_bytes());
    put(&mut rec, 45, 10, format!("{:>10}", money(cents)).as_bytes());
    String::from_utf8(rec).expect("asa total")
}

fn put(rec: &mut [u8], start: usize, width: usize, bytes: &[u8]) {
    let n = bytes.len().min(width);
    rec[start..start + n].copy_from_slice(&bytes[..n]);
}

fn money(cents: i64) -> String {
    let neg = cents < 0;
    let cents = cents.unsigned_abs();
    let dollars = cents / 100;
    let rem = cents % 100;
    let mut digits: Vec<char> = dollars.to_string().chars().collect();
    let mut i = digits.len() as isize - 3;
    while i > 0 {
        digits.insert(i as usize, ',');
        i -= 3;
    }
    format!(
        "{}{}.{:02}",
        if neg { "-" } else { "" },
        digits.into_iter().collect::<String>(),
        rem
    )
}
