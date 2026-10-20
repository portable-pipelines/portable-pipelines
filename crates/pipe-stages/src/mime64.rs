//! 64ENCODE / 64DECODE — MIME Base-64.
//!
//! Author's Edition 1.1.11 pp.613–614.
//!
//! Syntax (diagrams all-caps; no operands):
//! `64ENCODE`
//! `64DECODE`
//!
//! RFC 1521. Portable ASCII alphabet `A–Z a–z 0–9 + /` and pad `=` (the book
//! says encoded data are represented in EBCDIC; this port uses the RFC table
//! in ASCII, matching XLATE’s portable-ASCII treatment of EBCDIC samples).

use pipe_core::{Record, StageCtx, StreamId};

/// 57 input bytes encode to a 76-character output record (p.614).
const LINE_BYTES: usize = 57;
const LINE_CHARS: usize = 76;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_encode(ctx, &args))
}

pub fn decode(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_decode(ctx, &args))
}

fn reject_operands(ctx: &mut StageCtx, verb: &str, args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    ctx.fail(format!("{verb}: unexpected operand {}", args[0]));
    true
}

fn reject_extra_inputs(ctx: &mut StageCtx, verb: &str) -> bool {
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::PRIMARY.0 {
            ctx.fail(format!(
                "{verb}: no input stream other than the primary may be connected"
            ));
            return true;
        }
    }
    false
}

fn short_if_no_output(ctx: &mut StageCtx) -> bool {
    if ctx.output_connected(StreamId::PRIMARY) {
        return false;
    }
    ctx.select_input(StreamId::PRIMARY);
    ctx.short_input();
    true
}

fn run_encode(ctx: &mut StageCtx, args: &[String]) {
    if reject_operands(ctx, "64ENCODE", args) {
        return;
    }
    if reject_extra_inputs(ctx, "64ENCODE") {
        return;
    }
    if short_if_no_output(ctx) {
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    let mut pending = Vec::new();
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        while pending.len() >= LINE_BYTES {
            let chunk: Vec<u8> = pending.drain(..LINE_BYTES).collect();
            let line = encode_bytes(&chunk);
            debug_assert_eq!(line.len(), LINE_CHARS);
            if ctx.write(Record::from(line)).is_err() {
                return;
            }
            if !ctx.output_connected(StreamId::PRIMARY) {
                return;
            }
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if rec.is_empty() {
                // p.614: null input records are discarded (byte stream).
                let _ = ctx.read();
                continue;
            }
            pending.extend_from_slice(rec.as_bytes());
            let _ = ctx.read();
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            if !pending.is_empty() {
                let _ = ctx.write(Record::from(encode_bytes(&pending)));
            }
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

fn run_decode(ctx: &mut StageCtx, args: &[String]) {
    if reject_operands(ctx, "64DECODE", args) {
        return;
    }
    if reject_extra_inputs(ctx, "64DECODE") {
        return;
    }
    if short_if_no_output(ctx) {
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = decode_bytes(rec.as_bytes());
            if ctx.write(Record::from(out)).is_err() {
                return;
            }
            let _ = ctx.read();
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

fn encode_bytes(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(encoded_len(input.len()));
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = u32::from(input[i]) << 16 | u32::from(input[i + 1]) << 8 | u32::from(input[i + 2]);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize]);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize]);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize]);
        out.push(ALPHABET[(n & 0x3f) as usize]);
        i += 3;
    }
    match input.len() - i {
        1 => {
            let n = u32::from(input[i]) << 16;
            out.push(ALPHABET[((n >> 18) & 0x3f) as usize]);
            out.push(ALPHABET[((n >> 12) & 0x3f) as usize]);
            out.push(b'=');
            out.push(b'=');
        }
        2 => {
            let n = u32::from(input[i]) << 16 | u32::from(input[i + 1]) << 8;
            out.push(ALPHABET[((n >> 18) & 0x3f) as usize]);
            out.push(ALPHABET[((n >> 12) & 0x3f) as usize]);
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize]);
            out.push(b'=');
        }
        _ => {}
    }
    out
}

fn encoded_len(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// RFC 1521: characters outside the alphabet are ignored. Length need not be a
/// multiple of four (p.613: “this is not enforced”). Leftover 2/3 alphabet
/// characters are decoded as if padded; a leftover 1 is ignored.
fn decode_bytes(input: &[u8]) -> Vec<u8> {
    let mut chars = Vec::with_capacity(input.len());
    for &c in input {
        if is_base64_char(c) || c == b'=' {
            chars.push(c);
        }
    }
    let mut out = Vec::with_capacity(chars.len() / 4 * 3);
    let mut i = 0;
    while i + 4 <= chars.len() {
        decode_quad(&chars[i..i + 4], &mut out);
        i += 4;
    }
    match chars.len() - i {
        0 | 1 => {}
        2 | 3 => {
            let mut pad = chars[i..].to_vec();
            while pad.len() < 4 {
                pad.push(b'=');
            }
            decode_quad(&pad, &mut out);
        }
        _ => {}
    }
    out
}

fn is_base64_char(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/')
}

fn decode_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_quad(q: &[u8], out: &mut Vec<u8>) {
    let v0 = decode_val(q[0]);
    let v1 = decode_val(q[1]);
    let (Some(a), Some(b)) = (v0, v1) else {
        return;
    };
    let c = decode_val(q[2]).unwrap_or(0);
    let d = decode_val(q[3]).unwrap_or(0);
    let n = u32::from(a) << 18 | u32::from(b) << 12 | u32::from(c) << 6 | u32::from(d);
    out.push((n >> 16) as u8);
    if q[2] != b'=' {
        out.push((n >> 8) as u8);
        if q[3] != b'=' {
            out.push(n as u8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_1521_vectors() {
        assert_eq!(encode_bytes(b""), b"");
        assert_eq!(encode_bytes(b"a"), b"YQ==");
        assert_eq!(encode_bytes(b"ab"), b"YWI=");
        assert_eq!(encode_bytes(b"abc"), b"YWJj");
        assert_eq!(encode_bytes(b"Man"), b"TWFu");
        assert_eq!(encode_bytes(b"hello"), b"aGVsbG8=");
        assert_eq!(encode_bytes(b"pleasure."), b"cGxlYXN1cmUu");
        assert_eq!(encode_bytes(b"leasure."), b"bGVhc3VyZS4=");
        assert_eq!(encode_bytes(b"asure."), b"YXN1cmUu");
        assert_eq!(encode_bytes(b"sure."), b"c3VyZS4=");
    }

    #[test]
    fn fifty_seven_bytes_are_seventy_six_chars() {
        let input = vec![b'A'; LINE_BYTES];
        let out = encode_bytes(&input);
        assert_eq!(out.len(), LINE_CHARS);
        assert_eq!(
            out,
            b"QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB"
        );
        assert_eq!(decode_bytes(&out), input);
    }

    #[test]
    fn decode_ignores_non_alphabet_and_accepts_unpadded() {
        assert_eq!(decode_bytes(b"YQ=="), b"a");
        assert_eq!(decode_bytes(b"YQ"), b"a");
        assert_eq!(decode_bytes(b"YWI="), b"ab");
        assert_eq!(decode_bytes(b"YWI"), b"ab");
        assert_eq!(decode_bytes(b"Y W I ="), b"ab");
        assert_eq!(decode_bytes(b"!!!!"), b"");
        assert_eq!(decode_bytes(b"YQ==YQ=="), b"aa");
    }

    #[test]
    fn round_trip_binary() {
        let samples: [&[u8]; 6] = [
            b"",
            b"\x00",
            b"\xff\xfe\xfd",
            b"hello, world",
            &[0u8, 1, 2, 3, 4, 5],
            b"A",
        ];
        for s in samples {
            assert_eq!(decode_bytes(&encode_bytes(s)), s, "{s:?}");
        }
    }
}
