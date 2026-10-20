//! QPENCODE / QPDECODE — MIME quoted-printable.
//!
//! Author's Edition 1.1.11 pp.466–467.
//!
//! Syntax (diagrams all-caps; no operands):
//! `QPENCODE`
//! `QPDECODE`
//!
//! RFC 2045. Operates in the ASCII domain (p.466 / p.467). Portable ASCII:
//! hex values in the inventory are ASCII bytes. The book’s Note 2 (convert
//! EBCDIC mail back to ASCII before QPDECODE, then back to EBCDIC) is left
//! out — this host is already ASCII.

use pipe_core::{Record, StageCtx, StreamId};

/// IBM: as many 76-byte output records as required (p.467).
const LINE_LEN: usize = 76;
/// Soft-break `=` needs one column, so a continuation holds at most 75 content.
const SOFT_CONTENT: usize = LINE_LEN - 1;

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
    if reject_operands(ctx, "QPENCODE", args) {
        return;
    }
    if reject_extra_inputs(ctx, "QPENCODE") {
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
            let lines = wrap_qp(&qp_encode_record(rec.as_bytes()));
            for line in lines {
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(Record::from(line)).is_err() {
                    return;
                }
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

fn run_decode(ctx: &mut StageCtx, args: &[String]) {
    if reject_operands(ctx, "QPDECODE", args) {
        return;
    }
    if reject_extra_inputs(ctx, "QPDECODE") {
        return;
    }

    // p.466: secondary streams may be defined. Premature termination when any
    // of its output streams is not connected — primary is always required;
    // secondary is required only when it starts connected (defined).
    let secondary_defined = ctx.output_connected(StreamId::SECONDARY);
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    let mut pending: Option<Vec<u8>> = None;
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if secondary_defined && !ctx.output_connected(StreamId::SECONDARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            match qp_decode_record(rec.as_bytes()) {
                Decode::Ok { mut bytes, splice } => {
                    if let Some(prev) = pending.take() {
                        let mut joined = prev;
                        joined.append(&mut bytes);
                        bytes = joined;
                    }
                    if splice {
                        pending = Some(bytes);
                    } else {
                        ctx.select_output(StreamId::PRIMARY);
                        if ctx.write(Record::from(bytes)).is_err() {
                            return;
                        }
                    }
                    let _ = ctx.read();
                }
                Decode::Invalid => {
                    // p.466: no secondary → message and exit. Secondary defined →
                    // write any splice buffer to primary, the whole bad record to
                    // secondary, and continue.
                    if !secondary_defined {
                        ctx.fail("QPDECODE: invalid escape sequence");
                        return;
                    }
                    if let Some(partial) = pending.take() {
                        ctx.select_output(StreamId::PRIMARY);
                        if ctx.write(Record::from(partial)).is_err() {
                            return;
                        }
                    }
                    ctx.select_output(StreamId::SECONDARY);
                    if ctx.write(rec.clone()).is_err() {
                        return;
                    }
                    let _ = ctx.read();
                }
            }
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            if let Some(partial) = pending.take() {
                ctx.select_output(StreamId::PRIMARY);
                let _ = ctx.write(Record::from(partial));
            }
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

/// RFC 2045 printable 33–60 / 62–126, minus IBM extra ASCII that is not
/// codepage-invariant in the EBCDIC domain (p.467): `!"#$ @[\]^ `{|}~`.
fn is_qp_literal(b: u8) -> bool {
    matches!(
        b,
        b'%'..=b'<' | b'>'..=b'?' | b'A'..=b'Z' | b'_' | b'a'..=b'z'
    )
}

fn qp_encode_record(record: &[u8]) -> Vec<u8> {
    let mut trailing_from = record.len();
    while trailing_from > 0 {
        let b = record[trailing_from - 1];
        if b == b' ' || b == b'\t' {
            trailing_from -= 1;
        } else {
            break;
        }
    }
    let mut out = Vec::with_capacity(record.len());
    for (i, &b) in record.iter().enumerate() {
        let trailing = i >= trailing_from;
        if (b == b' ' || b == b'\t') && !trailing {
            out.push(b);
        } else if is_qp_literal(b) {
            out.push(b);
        } else {
            out.extend_from_slice(&qp_hex(b));
        }
    }
    out
}

fn qp_hex(b: u8) -> [u8; 3] {
    const HEX: &[u8] = b"0123456789ABCDEF";
    [b'=', HEX[(b >> 4) as usize], HEX[(b & 0x0f) as usize]]
}

/// Wrap encoded bytes into records of at most 76. Continuation records end
/// with a soft-break `=` so QPDECODE splices them (p.466). `=XX` is never
/// split, so a continuation can be shorter than 76 when a triplet would
/// otherwise straddle the limit.
fn wrap_qp(encoded: &[u8]) -> Vec<Vec<u8>> {
    if encoded.is_empty() {
        return vec![Vec::new()];
    }
    let mut lines = Vec::new();
    let mut i = 0;
    while i < encoded.len() {
        let left = encoded.len() - i;
        if left <= LINE_LEN {
            lines.push(encoded[i..].to_vec());
            break;
        }
        let mut take = SOFT_CONTENT;
        take = unsplit_triplet(&encoded[i..i + take], take);
        let mut line = encoded[i..i + take].to_vec();
        line.push(b'=');
        lines.push(line);
        i += take;
    }
    lines
}

fn unsplit_triplet(buf: &[u8], take: usize) -> usize {
    if take >= 1 && buf[take - 1] == b'=' {
        return take - 1;
    }
    if take >= 2 && buf[take - 2] == b'=' {
        return take - 2;
    }
    take
}

enum Decode {
    Ok { bytes: Vec<u8>, splice: bool },
    Invalid,
}

fn qp_decode_record(record: &[u8]) -> Decode {
    let mut out = Vec::with_capacity(record.len());
    let mut i = 0;
    while i < record.len() {
        if record[i] != b'=' {
            // Note 3 p.467: non-escape bytes are not validated.
            out.push(record[i]);
            i += 1;
            continue;
        }
        if i + 1 == record.len() {
            return Decode::Ok {
                bytes: out,
                splice: true,
            };
        }
        if i + 2 >= record.len() {
            return Decode::Invalid;
        }
        match (from_hex(record[i + 1]), from_hex(record[i + 2])) {
            (Some(hi), Some(lo)) => {
                out.push((hi << 4) | lo);
                i += 3;
            }
            _ => return Decode::Invalid,
        }
    }
    Decode::Ok {
        bytes: out,
        splice: false,
    }
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(s: &[u8]) -> Vec<u8> {
        qp_encode_record(s)
    }

    fn dec(s: &[u8]) -> (Vec<u8>, bool) {
        match qp_decode_record(s) {
            Decode::Ok { bytes, splice } => (bytes, splice),
            Decode::Invalid => panic!("invalid {s:?}"),
        }
    }

    #[test]
    fn literals_and_equals() {
        assert_eq!(enc(b"abc"), b"abc");
        assert_eq!(enc(b"a=b"), b"a=3Db");
        assert_eq!(enc(b""), b"");
    }

    #[test]
    fn trailing_whitespace_is_encoded() {
        assert_eq!(enc(b"a b"), b"a b");
        assert_eq!(enc(b"a "), b"a=20");
        assert_eq!(enc(b"a\t"), b"a=09");
        assert_eq!(enc(b"a \t "), b"a=20=09=20");
        assert_eq!(enc(b"  "), b"=20=20");
        assert_eq!(enc(b"a b "), b"a b=20");
    }

    #[test]
    fn ibm_extra_ascii_is_encoded() {
        // p.467: X'21222324' X'40' X'5b5c5d5e' X'60' X'7b7c7d7e'
        assert_eq!(enc(b"!\"#$"), b"=21=22=23=24");
        assert_eq!(enc(b"@"), b"=40");
        assert_eq!(enc(b"[\\]^"), b"=5B=5C=5D=5E");
        assert_eq!(enc(b"`"), b"=60");
        assert_eq!(enc(b"{|}~"), b"=7B=7C=7D=7E");
        // Underscore is not in that list and is RFC-safe.
        assert_eq!(enc(b"_"), b"_");
        assert_eq!(
            enc(b"%&'()*+,-./0123456789:;<>?"),
            b"%&'()*+,-./0123456789:;<>?"
        );
    }

    #[test]
    fn controls_and_eighth_bit() {
        assert_eq!(enc(&[0x00]), b"=00");
        assert_eq!(enc(&[0x7f]), b"=7F");
        assert_eq!(enc(&[0xff]), b"=FF");
    }

    #[test]
    fn wrap_seventy_six_and_soft_break() {
        let seventy_six = vec![b'a'; 76];
        assert_eq!(wrap_qp(&seventy_six), vec![seventy_six.clone()]);
        let seventy_seven = vec![b'a'; 77];
        let lines = wrap_qp(&seventy_seven);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 76);
        assert_eq!(lines[0][75], b'=');
        assert_eq!(&lines[0][..75], vec![b'a'; 75]);
        assert_eq!(lines[1], b"aa");
    }

    #[test]
    fn wrap_does_not_split_encoded_triplet() {
        // 74 a's then `=` → 74 + "=3D" = 77 encoded bytes.
        let mut rec = vec![b'a'; 74];
        rec.push(b'=');
        let encoded = enc(&rec);
        assert_eq!(encoded.len(), 77);
        let lines = wrap_qp(&encoded);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(b"="));
        assert!(!lines[0].windows(2).any(|w| w == b"=="));
        assert_eq!(lines[1], b"=3D");
        let (body, splice) = dec(&lines[0]);
        assert!(splice);
        let (tail, splice_tail) = dec(&lines[1]);
        assert!(!splice_tail);
        let mut joined = body;
        joined.extend_from_slice(&tail);
        assert_eq!(joined, rec);
    }

    #[test]
    fn decode_hex_is_case_insensitive() {
        assert_eq!(dec(b"a=3db").0, b"a=b");
        assert_eq!(dec(b"a=3Db").0, b"a=b");
        assert_eq!(dec(b"=").1, true);
        assert_eq!(dec(b"abc").0, b"abc");
    }

    #[test]
    fn decode_passes_non_escape_bytes() {
        // Note 3: spaces / controls that are not `=XX` pass through.
        assert_eq!(dec(b"a b").0, b"a b");
        assert_eq!(dec(&[0x01, b'A']).0, vec![0x01, b'A']);
    }

    #[test]
    fn decode_rejects_bad_escapes() {
        assert!(matches!(qp_decode_record(b"a=G1"), Decode::Invalid));
        assert!(matches!(qp_decode_record(b"a=3"), Decode::Invalid));
        assert!(matches!(qp_decode_record(b"a=zz"), Decode::Invalid));
        assert!(matches!(qp_decode_record(b"a=b"), Decode::Invalid));
    }

    #[test]
    fn encode_decode_round_trip() {
        let samples: [&[u8]; 8] = [
            b"",
            b"abc",
            b"a=b",
            b"hello world ",
            b"!@#",
            b"a\tb",
            &[0xff, 0x00, b'Z'],
            b"1234567890",
        ];
        for s in samples {
            let lines = wrap_qp(&enc(s));
            let mut out = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let (bytes, splice) = dec(line);
                out.extend_from_slice(&bytes);
                let last = i + 1 == lines.len();
                assert_eq!(splice, !last, "soft break only on continuation {s:?}");
            }
            assert_eq!(out, s, "{s:?}");
        }
    }
}
