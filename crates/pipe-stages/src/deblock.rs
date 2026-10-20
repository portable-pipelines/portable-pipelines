//! DEBLOCK — deblock external data formats (portable subset).
//!
//! Author's Edition 1.1.11 pp.317–320.
//!
//! Syntax (diagram all-caps; PC-style branch only):
//! `DEBLOCK LINEND [xorc] [TERMinate] [EOF [xorc]]`
//! `DEBLOCK CRLF [TERMinate] [EOF [xorc]]`
//!
//! Every other format (FIXED, STRING, C, CMS, V/VB/VS/VBS, SF, AWSTAPE,
//! NETDATA, CKD, …) is rejected. Bare `DEBLOCK` is FIXED 80 and is rejected.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::matches_abbrev;
use crate::xrange::parse_xorc;

/// Portable ASCII stand-in for the railroad default X'15' (EBCDIC NL).
const ASCII_LINEND: u8 = b'\n';
/// Portable ASCII CRLF (book X'0D25' is EBCDIC CR+LF; HTTPSPLIT Note 3).
const ASCII_CRLF: &[u8] = b"\r\n";
/// Portable ASCII stand-in for X'3F' (EBCDIC substitute / PC Ctrl-Z after A2E).
const ASCII_EOF: u8 = 0x1A;

#[derive(Debug, Clone)]
struct DeblockSpec {
    delim: Vec<u8>,
    terminate: bool,
    eof: Option<u8>,
}

pub fn deblock(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_deblock(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("DEBLOCK: {e}"));
                return;
            }
        };
        run_deblock(ctx, &spec);
    })
}

fn parse_deblock(args: &[String]) -> Result<DeblockSpec, String> {
    if args.is_empty() {
        return Err("FIXED (the default) is not implemented; use LINEND or CRLF".into());
    }

    let mut i = 0;
    let upper = args[i].to_ascii_uppercase();
    let delim = if upper == "LINEND" {
        i += 1;
        let mut d = vec![ASCII_LINEND];
        if i < args.len() && !is_terminate(&args[i]) && !is_eof_kw(&args[i]) {
            d = vec![parse_xorc(&args[i])?];
            i += 1;
        }
        d
    } else if upper == "CRLF" {
        i += 1;
        ASCII_CRLF.to_vec()
    } else if is_unimplemented_format(&upper) || upper.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("{upper} is not implemented; use LINEND or CRLF"));
    } else {
        return Err(format!("unrecognized operand {}", args[i]));
    };

    let mut terminate = false;
    if i < args.len() && is_terminate(&args[i]) {
        terminate = true;
        i += 1;
    }

    let mut eof = None;
    if i < args.len() && is_eof_kw(&args[i]) {
        i += 1;
        eof = Some(ASCII_EOF);
        if i < args.len() {
            eof = Some(parse_xorc(&args[i])?);
            i += 1;
        }
    }

    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }
    Ok(DeblockSpec {
        delim,
        terminate,
        eof,
    })
}

fn is_terminate(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "TERMINATE", 4)
}

fn is_eof_kw(s: &str) -> bool {
    s.eq_ignore_ascii_case("EOF")
}

fn is_unimplemented_format(upper: &str) -> bool {
    matches!(
        upper,
        "FIXED"
            | "VARIABLE"
            | "V"
            | "VB"
            | "VS"
            | "VBS"
            | "CMS"
            | "CMS4"
            | "SF"
            | "SF4"
            | "RDW"
            | "RFC959"
            | "ONEBYTE"
            | "ADMSF"
            | "GDFORDERS"
            | "GDF"
            | "AWSTAPE"
            | "DECIMAL"
            | "NETDATA"
            | "NET"
            | "TEXTUNIT"
            | "MONITOR"
            | "TEXTFILE"
            | "TEXT"
            | "C"
            | "STRING"
            | "STR"
            | "FROMRIGHT"
            | "CKD"
    ) || matches_abbrev(upper, "STRING", 3)
        || matches_abbrev(upper, "NETDATA", 3)
        || matches_abbrev(upper, "TEXTFILE", 4)
        || matches_abbrev(upper, "GDFORDERS", 3)
}

pub(crate) fn find_first_delim(haystack: &[u8], needles: &[&[u8]]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for n in needles {
        if n.is_empty() {
            continue;
        }
        if let Some(pos) = find_subseq(haystack, n) {
            match best {
                None => best = Some((pos, n.len())),
                Some((b, _)) if pos < b => best = Some((pos, n.len())),
                _ => {}
            }
        }
    }
    best
}

fn find_subseq(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn run_deblock(ctx: &mut StageCtx, spec: &DeblockSpec) {
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::PRIMARY.0 {
            ctx.fail("DEBLOCK: no input stream other than the primary may be connected");
            return;
        }
    }

    if !ctx.output_connected(StreamId::PRIMARY) {
        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
        }
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    let mut pending = Vec::new();
    let mut saw_data = false;
    let mut ended_with_delim = false;
    let mut stream_done = false;
    let needles: Vec<&[u8]> = vec![spec.delim.as_slice()];

    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }

        while let Some((pos, nlen)) = find_first_delim(&pending, &needles) {
            let rec = pending.drain(..pos).collect::<Vec<_>>();
            pending.drain(..nlen);
            ended_with_delim = true;
            if ctx.write(Record::from(rec)).is_err() {
                return;
            }
            if !ctx.output_connected(StreamId::PRIMARY) {
                return;
            }
        }

        if stream_done {
            flush_tail(
                ctx,
                &mut pending,
                spec.terminate,
                ended_with_delim,
                saw_data,
            );
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if rec.is_empty() {
                let _ = ctx.read();
                continue;
            }
            saw_data = true;
            let bytes = rec.as_bytes();
            if let Some(eof) = spec.eof {
                if let Some(pos) = bytes.iter().position(|&b| b == eof) {
                    pending.extend_from_slice(&bytes[..pos]);
                    stream_done = true;
                    let _ = ctx.read();
                    continue;
                }
            }
            pending.extend_from_slice(bytes);
            ended_with_delim = false;
            let _ = ctx.read();
            continue;
        }

        if ctx.input_at_eof(StreamId::PRIMARY) {
            stream_done = true;
            continue;
        }

        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

fn flush_tail(
    ctx: &mut StageCtx,
    pending: &mut Vec<u8>,
    terminate: bool,
    ended_with_delim: bool,
    saw_data: bool,
) {
    if !pending.is_empty() {
        let _ = ctx.write(Record::from(std::mem::take(pending)));
        return;
    }
    if saw_data && ended_with_delim && !terminate {
        let _ = ctx.write(Record::empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> DeblockSpec {
        parse_deblock(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn linend_default_is_ascii_lf() {
        assert_eq!(spec(&["LINEND"]).delim, b"\n");
        assert!(!spec(&["LINEND"]).terminate);
        assert_eq!(spec(&["LINEND"]).eof, None);
    }

    #[test]
    fn linend_xorc_and_terminate_eof() {
        let s = spec(&["LINEND", "15", "TERM", "EOF"]);
        assert_eq!(s.delim, vec![0x15]);
        assert!(s.terminate);
        assert_eq!(s.eof, Some(ASCII_EOF));
        let s = spec(&["CRLF", "TERMINATE", "EOF", "3F"]);
        assert_eq!(s.delim, b"\r\n");
        assert_eq!(s.eof, Some(0x3F));
    }

    #[test]
    fn default_and_unimplemented_rejected() {
        assert!(parse_deblock(&[]).unwrap_err().contains("FIXED"));
        assert!(parse_deblock(&["80".into()]).unwrap_err().contains("80"));
        assert!(parse_deblock(&["C".into()]).unwrap_err().contains("C"));
        assert!(parse_deblock(&["STRING".into(), "/x/".into()])
            .unwrap_err()
            .contains("STRING"));
        assert!(parse_deblock(&["FIXED".into(), "80".into()])
            .unwrap_err()
            .contains("FIXED"));
        assert!(parse_deblock(&["CMS".into()]).unwrap_err().contains("CMS"));
        assert!(parse_deblock(&["V".into()]).unwrap_err().contains("V"));
        assert!(parse_deblock(&["NETDATA".into()])
            .unwrap_err()
            .contains("NETDATA"));
        assert!(parse_deblock(&["CKD".into()]).unwrap_err().contains("CKD"));
    }

    #[test]
    fn find_delim_spans() {
        assert_eq!(find_first_delim(b"ab\r\ncd", &[b"\r\n"]), Some((2, 2)));
        assert_eq!(find_first_delim(b"ab\r", &[b"\r\n"]), None);
        assert_eq!(
            find_first_delim(b"x\x0d\x15y", &[b"\x0d\x25", b"\x0d\x15"]),
            Some((1, 2))
        );
    }
}
