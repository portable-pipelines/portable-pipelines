//! HTTPSPLIT — split an HTTP data stream into headers and body.
//!
//! Author's Edition 1.1.11 p.387.
//!
//! Syntax (diagram all-caps):
//! `HTTPSPLIT [EBCDIC]`
//!
//! ASCII default (Note 2 / Note 3): header CRLF is X'0D0A'. `EBCDIC` parses
//! headers with CRLF X'0D25' or X'0D15'.

use pipe_core::{Record, StageCtx, StreamId};

use crate::deblock::find_first_delim;

const ASCII_CRLF: &[u8] = b"\r\n";
const EBCDIC_CRLF_LF: &[u8] = b"\x0d\x25";
const EBCDIC_CRLF_NL: &[u8] = b"\x0d\x15";

pub fn httpsplit(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let ebcdic = match parse_httpsplit(&args) {
            Ok(e) => e,
            Err(e) => {
                ctx.fail(format!("HTTPSPLIT: {e}"));
                return;
            }
        };
        run_httpsplit(ctx, ebcdic);
    })
}

fn parse_httpsplit(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [s] if s.eq_ignore_ascii_case("EBCDIC") => Ok(true),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn crlf_needles(ebcdic: bool) -> Vec<&'static [u8]> {
    if ebcdic {
        vec![EBCDIC_CRLF_LF, EBCDIC_CRLF_NL]
    } else {
        vec![ASCII_CRLF]
    }
}

/// `content-length` followed by `=` or `:`, then an unsigned count.
///
/// Ambiguity: the book writes `content-length=`; HTTP/1.x uses `Content-Length:`.
/// Both separators are accepted; the name is matched case-insensitively.
pub(crate) fn content_length_of(line: &[u8], ebcdic: bool) -> Option<usize> {
    let ascii = if ebcdic {
        line.iter().copied().map(ebcdic_header_byte).collect()
    } else {
        line.to_ascii_lowercase()
    };
    let key = b"content-length";
    let pos = ascii.windows(key.len()).position(|w| w == key)?;
    let rest = &ascii[pos + key.len()..];
    let rest = skip_ws(rest);
    let rest = rest
        .strip_prefix(&[b'='][..])
        .or_else(|| rest.strip_prefix(&[b':'][..]))?;
    let rest = skip_ws(rest);
    if rest.is_empty() || !rest[0].is_ascii_digit() {
        return None;
    }
    let mut n: usize = 0;
    for &b in rest {
        if b.is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add((b - b'0') as usize);
        } else {
            break;
        }
    }
    Some(n)
}

fn skip_ws(s: &[u8]) -> &[u8] {
    let i = s
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(s.len());
    &s[i..]
}

/// Map EBCDIC CP037/1047 letters, digits, and HTTP punctuation to ASCII lowercase.
fn ebcdic_header_byte(b: u8) -> u8 {
    match b {
        0x81..=0x89 => b'a' + (b - 0x81),
        0x91..=0x99 => b'j' + (b - 0x91),
        0xA2..=0xA9 => b's' + (b - 0xA2),
        0xC1..=0xC9 => b'a' + (b - 0xC1),
        0xD1..=0xD9 => b'j' + (b - 0xD1),
        0xE2..=0xE9 => b's' + (b - 0xE2),
        0xF0..=0xF9 => b'0' + (b - 0xF0),
        0x7E => b'=',
        0x7A => b':',
        0x60 => b'-',
        0x40 | 0x41 => b' ',
        other => other.to_ascii_lowercase(),
    }
}

fn any_output(ctx: &StageCtx) -> bool {
    !ctx.connected_outputs().is_empty()
}

fn could_be_crlf_prefix(pending: &[u8], needles: &[&[u8]]) -> bool {
    needles
        .iter()
        .any(|n| n.starts_with(pending) && pending.len() < n.len())
}

fn run_httpsplit(ctx: &mut StageCtx, ebcdic: bool) {
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::PRIMARY.0 {
            ctx.fail("HTTPSPLIT: no input stream other than the primary may be connected");
            return;
        }
    }
    if ctx.connected_outputs().iter().any(|s| s.0 > 1) {
        ctx.fail("HTTPSPLIT: only the primary and secondary output streams may be defined");
        return;
    }

    if !any_output(ctx) {
        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
        }
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    let needles = crlf_needles(ebcdic);
    let mut pending = Vec::new();
    let mut header_mode = true;
    let mut drop_leading_crlf = true;
    let mut content_length: Option<usize> = None;
    let mut body_left: Option<usize> = None;

    loop {
        if !any_output(ctx) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
            }
            return;
        }

        if header_mode {
            if drop_leading_crlf {
                if let Some((0, nlen)) = find_first_delim(&pending, &needles) {
                    pending.drain(..nlen);
                    drop_leading_crlf = false;
                } else if !pending.is_empty() && !could_be_crlf_prefix(&pending, &needles) {
                    drop_leading_crlf = false;
                }
            }

            if let Some((pos, nlen)) = find_first_delim(&pending, &needles) {
                drop_leading_crlf = false;
                let rec: Vec<u8> = pending.drain(..pos).collect();
                pending.drain(..nlen);
                if rec.is_empty() {
                    // End of headers: write the null line, then data mode.
                    if ctx.output_connected(StreamId::PRIMARY) {
                        ctx.select_output(StreamId::PRIMARY);
                        if ctx.write(Record::empty()).is_err() {
                            return;
                        }
                    }
                    header_mode = false;
                    body_left = content_length;
                    if body_left.is_none() {
                        // No Content-Length: sever primary after the null line
                        // has been delivered (`short_output` would drop it).
                        if ctx.output_connected(StreamId::PRIMARY) {
                            ctx.select_output(StreamId::PRIMARY);
                            ctx.sever_output();
                        }
                    }
                    continue;
                }
                if let Some(n) = content_length_of(&rec, ebcdic) {
                    content_length = Some(n);
                }
                if ctx.output_connected(StreamId::PRIMARY) {
                    ctx.select_output(StreamId::PRIMARY);
                    if ctx.write(Record::from(rec)).is_err() {
                        return;
                    }
                }
                continue;
            }
        } else if let Some(left) = body_left {
            if left == 0 {
                header_mode = true;
                drop_leading_crlf = true;
                content_length = None;
                body_left = None;
                continue;
            }
            if !pending.is_empty() {
                let take = left.min(pending.len());
                let rec: Vec<u8> = pending.drain(..take).collect();
                body_left = Some(left - take);
                if ctx.output_connected(StreamId::SECONDARY) {
                    ctx.select_output(StreamId::SECONDARY);
                    if ctx.write(Record::from(rec)).is_err() {
                        return;
                    }
                }
                continue;
            }
        } else {
            // No content-length: remaining input as raw records.
            if !pending.is_empty() {
                let rec = std::mem::take(&mut pending);
                if ctx.output_connected(StreamId::SECONDARY) {
                    ctx.select_output(StreamId::SECONDARY);
                    if ctx.write(Record::from(rec)).is_err() {
                        return;
                    }
                }
                continue;
            }
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if !header_mode && body_left.is_none() && pending.is_empty() {
                // Raw input records for the body (Note 1), including nulls.
                if ctx.output_connected(StreamId::SECONDARY) {
                    ctx.select_output(StreamId::SECONDARY);
                    if ctx.write(rec).is_err() {
                        return;
                    }
                }
                let _ = ctx.read();
                continue;
            }
            pending.extend_from_slice(rec.as_bytes());
            let _ = ctx.read();
            continue;
        }

        if ctx.input_at_eof(StreamId::PRIMARY) {
            if header_mode {
                if drop_leading_crlf {
                    if let Some((0, nlen)) = find_first_delim(&pending, &needles) {
                        pending.drain(..nlen);
                    }
                }
                if !pending.is_empty() && ctx.output_connected(StreamId::PRIMARY) {
                    ctx.select_output(StreamId::PRIMARY);
                    let _ = ctx.write(Record::from(std::mem::take(&mut pending)));
                }
            } else if !pending.is_empty() && ctx.output_connected(StreamId::SECONDARY) {
                ctx.select_output(StreamId::SECONDARY);
                let _ = ctx.write(Record::from(std::mem::take(&mut pending)));
            }
            return;
        }

        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_length_equals_and_colon() {
        assert_eq!(content_length_of(b"Content-Length: 12", false), Some(12));
        assert_eq!(content_length_of(b"content-length=4", false), Some(4));
        assert_eq!(content_length_of(b"Host: x", false), None);
        assert_eq!(content_length_of(b"Content-Length: 0", false), Some(0));
    }

    #[test]
    fn content_length_ebcdic() {
        // C/c o n t e n t - L e n g t h = 5  in CP037
        let mut line = Vec::new();
        for &b in b"content-length=5" {
            line.push(ascii_to_ebcdic(b));
        }
        assert_eq!(content_length_of(&line, true), Some(5));
    }

    fn ascii_to_ebcdic(b: u8) -> u8 {
        match b {
            b'a'..=b'i' => 0x81 + (b - b'a'),
            b'j'..=b'r' => 0x91 + (b - b'j'),
            b's'..=b'z' => 0xA2 + (b - b's'),
            b'A'..=b'I' => 0xC1 + (b - b'A'),
            b'J'..=b'R' => 0xD1 + (b - b'J'),
            b'S'..=b'Z' => 0xE2 + (b - b'S'),
            b'0'..=b'9' => 0xF0 + (b - b'0'),
            b'=' => 0x7E,
            b':' => 0x7A,
            b'-' => 0x60,
            b' ' => 0x40,
            other => other,
        }
    }
}
