//! COMBINE — combine a run of records into one.
//!
//! Author's Edition 1.1.11 pp.298–300.
//!
//! Syntax (fixed railroad order):
//! `COMBINE [number | * | KEYLENgth number | STOP [ALLEOF | ANYEOF | number]]`
//! `Or | aNd | eXclusiveor | FIRST | LAST`
//!
//! The function keyword is required. Verb `COMBINE` is all-caps (no prefix).
//! `KEYLENgth` min 6. Function abbreviations follow the Syntax Description
//! and Note 1 (p.300): `Or` min 1; `N` or exact `AND`; `X` or `EXCLUSIVEOR`
//! min 3; `FIRST` / `LAST` exact. `STOP` / `ALLEOF` / `ANYEOF` are whole words.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::matches_abbrev;

pub fn combine(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let parsed = match parse_combine(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("COMBINE: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::SECONDARY.0 {
                ctx.fail("COMBINE: no input stream other than the primary and secondary may be connected");
                return;
            }
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx, ctx.input_connected(StreamId::SECONDARY));
            return;
        }

        let secondary = ctx.input_connected(StreamId::SECONDARY);
        match parsed.mode {
            ParsedMode::Stop(_) if !secondary => {
                ctx.fail("COMBINE: STOP is valid only when two input streams are defined");
            }
            ParsedMode::Append(_) | ParsedMode::Unlimited | ParsedMode::KeyLength(_)
                if secondary =>
            {
                ctx.fail(
                    "COMBINE: a number or KEYLENGTH is not valid when two input streams are defined",
                );
            }
            _ => {
                let spec = parsed.resolve(secondary);
                run_combine(ctx, spec, secondary);
            }
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Function {
    Or,
    And,
    Xor,
    First,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    AllEof,
    AnyEof,
    Count(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParsedMode {
    /// No run-count / STOP operand (default 1 on one stream; STOP ALLEOF on two).
    Default,
    /// Append this many records to the first of a set.
    Append(u64),
    Unlimited,
    KeyLength(usize),
    Stop(Stop),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CombineSpec {
    mode: ParsedMode,
    func: Function,
}

impl CombineSpec {
    fn resolve(self, secondary: bool) -> Self {
        let mode = match self.mode {
            ParsedMode::Default if secondary => ParsedMode::Stop(Stop::AllEof),
            ParsedMode::Default => ParsedMode::Append(1),
            other => other,
        };
        Self {
            mode,
            func: self.func,
        }
    }
}

fn parse_combine(args: &[String]) -> Result<CombineSpec, String> {
    let mut i = 0;

    let mut mode = ParsedMode::Default;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if args[i] == "*" {
            mode = ParsedMode::Unlimited;
            i += 1;
        } else if matches_abbrev(&upper, "KEYLENGTH", 6) {
            i += 1;
            if i >= args.len() {
                return Err("KEYLENGTH requires a number".into());
            }
            let n = parse_usize(&args[i], "KEYLENGTH")?;
            mode = ParsedMode::KeyLength(n);
            i += 1;
        } else if upper == "STOP" {
            i += 1;
            mode = ParsedMode::Stop(parse_stop(args, &mut i)?);
        } else if let Ok(n) = args[i].parse::<u64>() {
            mode = ParsedMode::Append(n);
            i += 1;
        }
    }

    if i >= args.len() {
        return Err("function is required".into());
    }
    let func = parse_function(&args[i])?;
    i += 1;

    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    Ok(CombineSpec { mode, func })
}

fn parse_stop(args: &[String], i: &mut usize) -> Result<Stop, String> {
    if *i >= args.len() {
        return Ok(Stop::AllEof);
    }
    let upper = args[*i].to_ascii_uppercase();
    if upper == "ALLEOF" {
        *i += 1;
        Ok(Stop::AllEof)
    } else if upper == "ANYEOF" {
        *i += 1;
        Ok(Stop::AnyEof)
    } else if let Ok(n) = args[*i].parse::<usize>() {
        if n == 0 {
            Err("STOP count must be at least 1".into())
        } else {
            *i += 1;
            Ok(Stop::Count(n))
        }
    } else {
        // Next token is the function; STOP alone is STOP ALLEOF (main line).
        Ok(Stop::AllEof)
    }
}

fn parse_function(arg: &str) -> Result<Function, String> {
    let u = arg.to_ascii_uppercase();
    // `Or` min 1 (p.298 Syntax Description "O"; Note 1: write in full as OR).
    if matches_abbrev(&u, "OR", 1) {
        return Ok(Function::Or);
    }
    // `aNd`: Syntax Description lists "N". Note 1: AND is a synonym for N
    // (no "abbreviated down to", so AND is exact; A / AN are not valid).
    if u == "N" || u == "AND" {
        return Ok(Function::And);
    }
    // `eXclusiveor`: Syntax Description lists "X". Note 1: EXCLUSIVEOR is a
    // synonym for X and can be abbreviated down to three characters.
    if u == "X" || matches_abbrev(&u, "EXCLUSIVEOR", 3) {
        return Ok(Function::Xor);
    }
    // FIRST / LAST are all-caps in the railroad (exact).
    if u == "FIRST" {
        return Ok(Function::First);
    }
    if u == "LAST" {
        return Ok(Function::Last);
    }
    Err(unexpected_operand(arg))
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if matches_abbrev(&upper, "KEYLENGTH", 6)
        || upper == "STOP"
        || upper == "ALLEOF"
        || upper == "ANYEOF"
        || matches_abbrev(&upper, "OR", 1)
        || upper == "N"
        || upper == "AND"
        || upper == "X"
        || matches_abbrev(&upper, "EXCLUSIVEOR", 3)
        || upper == "FIRST"
        || upper == "LAST"
    {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn parse_usize(s: &str, what: &str) -> Result<usize, String> {
    s.parse::<usize>()
        .map_err(|_| format!("{what} requires a number, got {s}"))
}

fn run_combine(ctx: &mut StageCtx, spec: CombineSpec, secondary: bool) {
    ctx.select_output(StreamId::PRIMARY);
    if secondary {
        let stop = match spec.mode {
            ParsedMode::Stop(s) => s,
            _ => Stop::AllEof,
        };
        run_pair(ctx, spec.func, stop);
    } else {
        run_single(ctx, spec);
    }
}

fn run_single(ctx: &mut StageCtx, spec: CombineSpec) {
    let mut buf = Vec::new();
    let mut contributors: u64 = 0;
    let mut key: Option<Vec<u8>> = None;
    let key_len = match spec.mode {
        ParsedMode::KeyLength(n) => n,
        _ => 0,
    };

    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        if !ctx.input_record_ready(StreamId::PRIMARY) {
            if ctx.input_at_eof(StreamId::PRIMARY) {
                if contributors > 0 && !emit(ctx, std::mem::take(&mut buf)) {
                    return;
                }
                return;
            }
            if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
                return;
            }
            continue;
        }

        let Some(rec) = ctx.peek() else {
            continue;
        };
        let bytes = rec.as_bytes();

        if let ParsedMode::KeyLength(_) = spec.mode {
            let rec_key = record_key(bytes, key_len);
            if contributors > 0 && key.as_deref() != Some(rec_key.as_slice()) {
                if !emit(ctx, std::mem::take(&mut buf)) {
                    return;
                }
                contributors = 0;
                key = None;
            }
        }

        let completes = match spec.mode {
            ParsedMode::Default => contributors.saturating_add(1) >= 2,
            ParsedMode::Append(n) => contributors.saturating_add(1) >= n.saturating_add(1),
            ParsedMode::Unlimited | ParsedMode::KeyLength(_) => {
                ctx.input_producer_done(StreamId::PRIMARY)
            }
            ParsedMode::Stop(_) => false,
        };

        if contributors == 0 {
            buf.extend_from_slice(bytes);
            if let ParsedMode::KeyLength(_) = spec.mode {
                key = Some(record_key(bytes, key_len));
            }
        } else {
            combine_into(&mut buf, bytes, spec.func, key_len);
        }
        contributors += 1;

        if completes {
            if !emit(ctx, std::mem::take(&mut buf)) {
                ctx.select_input(StreamId::PRIMARY);
                let _ = ctx.read();
                ctx.short_input();
                return;
            }
            contributors = 0;
            key = None;
        }

        ctx.select_input(StreamId::PRIMARY);
        if ctx.read().is_err() {
            return;
        }
    }
}

fn run_pair(ctx: &mut StageCtx, func: Function, stop: Stop) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx, true);
            return;
        }

        let p_ready = ctx.input_record_ready(StreamId::PRIMARY);
        let s_ready = ctx.input_record_ready(StreamId::SECONDARY);
        let p_eof = ctx.input_at_eof(StreamId::PRIMARY);
        let s_eof = ctx.input_at_eof(StreamId::SECONDARY);
        let dead = usize::from(p_eof) + usize::from(s_eof);

        if p_ready && s_ready {
            if !emit_pair(ctx, func) {
                short_inputs(ctx, true);
                return;
            }
            continue;
        }

        if p_eof && s_eof {
            return;
        }

        if stop_reached(stop, dead) {
            // IBM leaves the other stream unconsumed (p.298). Our `short`
            // severs so a producer is not parked on the one-record slot.
            short_inputs(ctx, true);
            return;
        }

        if p_ready && s_eof {
            if !emit_one(ctx, StreamId::PRIMARY) {
                short_inputs(ctx, true);
                return;
            }
            continue;
        }
        if s_ready && p_eof {
            if !emit_one(ctx, StreamId::SECONDARY) {
                short_inputs(ctx, true);
                return;
            }
            continue;
        }

        // Wait only on streams that are still live. An input already at EOF
        // makes `wait_any_input` return immediately, which would busy-loop
        // and starve the remaining producer.
        let mut waiting = Vec::new();
        if !p_eof {
            waiting.push(StreamId::PRIMARY);
        }
        if !s_eof {
            waiting.push(StreamId::SECONDARY);
        }
        if waiting.is_empty() {
            return;
        }
        if ctx.wait_any_input(&waiting).is_err() {
            return;
        }
    }
}

fn stop_reached(stop: Stop, dead: usize) -> bool {
    match stop {
        Stop::AllEof => false,
        Stop::AnyEof => dead >= 1,
        Stop::Count(n) => dead >= n,
    }
}

fn emit_pair(ctx: &mut StageCtx, func: Function) -> bool {
    ctx.select_input(StreamId::PRIMARY);
    let Some(primary) = ctx.peek() else {
        return true;
    };
    ctx.select_input(StreamId::SECONDARY);
    let Some(secondary) = ctx.peek() else {
        return true;
    };
    let mut buf = primary.as_bytes().to_vec();
    combine_into(&mut buf, secondary.as_bytes(), func, 0);
    if !emit(ctx, buf) {
        return false;
    }
    ctx.select_input(StreamId::PRIMARY);
    if ctx.read().is_err() {
        return false;
    }
    ctx.select_input(StreamId::SECONDARY);
    ctx.read().is_ok()
}

fn emit_one(ctx: &mut StageCtx, stream: StreamId) -> bool {
    ctx.select_input(stream);
    let Some(rec) = ctx.peek() else {
        return true;
    };
    if !emit(ctx, rec.as_bytes().to_vec()) {
        return false;
    }
    ctx.read().is_ok()
}

/// Combine `rec` into `buf` per p.299: the part that exceeds the buffer is
/// appended unmodified; the overlapping part is processed by `func`.
///
/// When `key_len > 0`, the first `key_len` columns of `buf` stay unchanged
/// and only positions beyond the key in `rec` are combined.
fn combine_into(buf: &mut Vec<u8>, rec: &[u8], func: Function, key_len: usize) {
    let rec_data = if rec.len() <= key_len {
        &[]
    } else {
        &rec[key_len..]
    };
    let buf_data_start = key_len.min(buf.len());
    let overlap = rec_data.len().min(buf.len().saturating_sub(buf_data_start));
    match func {
        Function::Or => {
            for i in 0..overlap {
                buf[buf_data_start + i] |= rec_data[i];
            }
        }
        Function::And => {
            for i in 0..overlap {
                buf[buf_data_start + i] &= rec_data[i];
            }
        }
        Function::Xor => {
            for i in 0..overlap {
                buf[buf_data_start + i] ^= rec_data[i];
            }
        }
        Function::First => {}
        Function::Last => {
            buf[buf_data_start..buf_data_start + overlap].copy_from_slice(&rec_data[..overlap]);
        }
    }
    if rec_data.len() > overlap {
        buf.extend_from_slice(&rec_data[overlap..]);
    }
}

fn record_key(bytes: &[u8], key_len: usize) -> Vec<u8> {
    bytes[..key_len.min(bytes.len())].to_vec()
}

fn emit(ctx: &mut StageCtx, bytes: Vec<u8>) -> bool {
    ctx.select_output(StreamId::PRIMARY);
    match ctx.write(Record::from(bytes)) {
        Ok(()) => ctx.output_connected(StreamId::PRIMARY),
        Err(_) => false,
    }
}

fn short_inputs(ctx: &mut StageCtx, secondary: bool) {
    ctx.select_input(StreamId::PRIMARY);
    ctx.short_input();
    if secondary {
        ctx.select_input(StreamId::SECONDARY);
        ctx.short_input();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CombineSpec, String> {
        parse_combine(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn default_is_append_one() {
        let spec = parse(&["OR"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::Default);
        assert_eq!(spec.resolve(false).mode, ParsedMode::Append(1));
        assert_eq!(spec.resolve(true).mode, ParsedMode::Stop(Stop::AllEof));
        assert_eq!(spec.func, Function::Or);
    }

    #[test]
    fn function_is_required() {
        let e = parse(&[]).unwrap_err();
        assert!(e.contains("function is required"), "{e}");
        let e = parse(&["2"]).unwrap_err();
        assert!(e.contains("function is required"), "{e}");
        let e = parse(&["*"]).unwrap_err();
        assert!(e.contains("function is required"), "{e}");
    }

    #[test]
    fn or_min_1() {
        assert_eq!(parse(&["O"]).unwrap().func, Function::Or);
        assert_eq!(parse(&["OR"]).unwrap().func, Function::Or);
        assert_eq!(parse(&["or"]).unwrap().func, Function::Or);
    }

    #[test]
    fn and_is_n_or_exact_and() {
        assert_eq!(parse(&["N"]).unwrap().func, Function::And);
        assert_eq!(parse(&["AND"]).unwrap().func, Function::And);
        assert_eq!(parse(&["and"]).unwrap().func, Function::And);
        let e = parse(&["A"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["AN"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["ND"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
    }

    #[test]
    fn exclusiveor_is_x_or_min_3() {
        assert_eq!(parse(&["X"]).unwrap().func, Function::Xor);
        assert_eq!(parse(&["EXC"]).unwrap().func, Function::Xor);
        assert_eq!(parse(&["EXCLUSIVEOR"]).unwrap().func, Function::Xor);
        assert_eq!(parse(&["exclusiveor"]).unwrap().func, Function::Xor);
        let e = parse(&["E"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["EX"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["XOR"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
    }

    #[test]
    fn first_and_last_are_exact() {
        assert_eq!(parse(&["FIRST"]).unwrap().func, Function::First);
        assert_eq!(parse(&["LAST"]).unwrap().func, Function::Last);
        let e = parse(&["FIRS"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["LAS"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["F"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["L"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
    }

    #[test]
    fn keylength_min_6() {
        let spec = parse(&["KEYLEN", "3", "OR"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::KeyLength(3));
        let spec = parse(&["KEYLENGTH", "0", "LAST"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::KeyLength(0));
        let e = parse(&["KEYLE", "3", "OR"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["KEYLENGTH", "OR"]).unwrap_err();
        assert!(e.contains("KEYLENGTH requires a number"), "{e}");
    }

    #[test]
    fn stop_defaults_to_alleof() {
        let spec = parse(&["STOP", "OR"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::Stop(Stop::AllEof));
        let spec = parse(&["STOP", "ALLEOF", "FIRST"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::Stop(Stop::AllEof));
        let spec = parse(&["STOP", "ANYEOF", "X"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::Stop(Stop::AnyEof));
        let spec = parse(&["STOP", "1", "AND"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::Stop(Stop::Count(1)));
        let spec = parse(&["STOP", "2", "OR"]).unwrap();
        assert_eq!(spec.mode, ParsedMode::Stop(Stop::Count(2)));
        let e = parse(&["STOP", "0", "OR"]).unwrap_err();
        assert!(e.contains("STOP count must be at least 1"), "{e}");
        let e = parse(&["STOP", "FOREVER", "OR"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
    }

    #[test]
    fn diagram_order_is_fixed() {
        let e = parse(&["OR", "2"]).unwrap_err();
        assert!(
            e.contains("diagram order") || e.contains("unexpected"),
            "{e}"
        );
        let e = parse(&["FIRST", "STOP", "ANYEOF"]).unwrap_err();
        assert!(
            e.contains("diagram order") || e.contains("unexpected"),
            "{e}"
        );
        let e = parse(&["2", "OR", "STOP"]).unwrap_err();
        assert!(
            e.contains("diagram order") || e.contains("unexpected"),
            "{e}"
        );
    }

    #[test]
    fn invented_operands_rejected() {
        for w in ["PAD", "COUNT", "ANYCASE", "XOR", "XORC", "BOTH"] {
            let e = parse(&[w]).unwrap_err();
            assert!(e.contains("unexpected operand"), "{w}: {e}");
        }
    }

    #[test]
    fn combine_into_or_and_xor_and_lengths() {
        let mut buf = b"AB".to_vec();
        combine_into(&mut buf, b"CD", Function::Or, 0);
        // 'A' 0x41 | 'C' 0x43 = 0x43 'C'; 'B' 0x42 | 'D' 0x44 = 0x46 'F'
        assert_eq!(buf, b"CF");

        let mut buf = b"AB".to_vec();
        combine_into(&mut buf, b"CD", Function::And, 0);
        // 0x41 & 0x43 = 0x41 'A'; 0x42 & 0x44 = 0x40 '@'
        assert_eq!(buf, b"A@");

        let mut buf = b"AA".to_vec();
        combine_into(&mut buf, b"BB", Function::Xor, 0);
        assert_eq!(buf, vec![0x03, 0x03]);

        let mut buf = b"ABCD".to_vec();
        combine_into(&mut buf, b"XY", Function::First, 0);
        assert_eq!(buf, b"ABCD");
        let mut buf = b"AB".to_vec();
        combine_into(&mut buf, b"WXYZ", Function::First, 0);
        assert_eq!(buf, b"ABYZ");

        let mut buf = b"ABCD".to_vec();
        combine_into(&mut buf, b"XY", Function::Last, 0);
        assert_eq!(buf, b"XYCD");
        let mut buf = b"AB".to_vec();
        combine_into(&mut buf, b"WXYZ", Function::Last, 0);
        assert_eq!(buf, b"WXYZ");
    }

    #[test]
    fn combine_into_keylength_leaves_key() {
        let mut buf = b"AA111".to_vec();
        combine_into(&mut buf, b"AA222", Function::Or, 2);
        // '1' 0x31 | '2' 0x32 = 0x33 '3'
        assert_eq!(buf, b"AA333");
        let mut buf = b"AA111".to_vec();
        combine_into(&mut buf, b"ZZ999", Function::Or, 2);
        // Later key columns are ignored; '1'| '9' = '9'.
        assert_eq!(buf, b"AA999");
    }

    #[test]
    fn first_zero_byte_still_contains_the_column() {
        let mut buf = vec![0x00, 0x00];
        combine_into(&mut buf, b"AA", Function::First, 0);
        assert_eq!(buf, vec![0x00, 0x00]);
        let mut buf = vec![0x00, 0x00];
        combine_into(&mut buf, b"AA", Function::Last, 0);
        assert_eq!(buf, b"AA");
    }
}
