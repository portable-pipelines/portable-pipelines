//! URLDEBLOCK — process a universal resource locator into fields.
//!
//! Author's Edition 1.1.11 pp.577–578.
//!
//! Syntax (diagram all-caps):
//! `URLDEBLOCK [EBCDIC]`
//!
//! CGI/form-urlencoded deblocking in the ASCII domain unless `EBCDIC` is
//! specified. Special characters (table on p.578):
//! - `&` (38) / `;` (59) split the record and are discarded
//! - `+` (43) becomes a blank
//! - `%` (37) plus two hex digits becomes that byte (`%2b` → ASCII plus)

use pipe_core::{Record, StageCtx, StreamId};

/// EBCDIC CP037/1047 encodings of the p.578 special characters.
const EBCDIC_AMP: u8 = 0x50;
const EBCDIC_SEMI: u8 = 0x5E;
const EBCDIC_PLUS: u8 = 0x4E;
const EBCDIC_PCT: u8 = 0x6C;
const EBCDIC_BLANK: u8 = 0x40;

pub fn urldeblock(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let ebcdic = match parse_urldeblock(&args) {
            Ok(e) => e,
            Err(e) => {
                ctx.fail(format!("URLDEBLOCK: {e}"));
                return;
            }
        };
        run_urldeblock(ctx, ebcdic);
    })
}

fn parse_urldeblock(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [s] if s.eq_ignore_ascii_case("EBCDIC") => Ok(true),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

struct Specials {
    amp: u8,
    semi: u8,
    plus: u8,
    pct: u8,
    blank: u8,
}

fn specials(ebcdic: bool) -> Specials {
    if ebcdic {
        Specials {
            amp: EBCDIC_AMP,
            semi: EBCDIC_SEMI,
            plus: EBCDIC_PLUS,
            pct: EBCDIC_PCT,
            blank: EBCDIC_BLANK,
        }
    } else {
        Specials {
            amp: b'&',
            semi: b';',
            plus: b'+',
            pct: b'%',
            blank: b' ',
        }
    }
}

fn hex_digit(b: u8, ebcdic: bool) -> Option<u8> {
    if ebcdic {
        match b {
            0xF0..=0xF9 => Some(b - 0xF0),
            0xC1..=0xC6 => Some(10 + (b - 0xC1)),
            0x81..=0x86 => Some(10 + (b - 0x81)),
            _ => None,
        }
    } else {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'A'..=b'F' => Some(10 + (b - b'A')),
            b'a'..=b'f' => Some(10 + (b - b'a')),
            _ => None,
        }
    }
}

/// Split one URL record into fields (p.578 table). Never called on a null input.
pub(crate) fn deblock_url(record: &[u8], ebcdic: bool) -> Vec<Vec<u8>> {
    let s = specials(ebcdic);
    let mut fields = Vec::new();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < record.len() {
        let b = record[i];
        if b == s.amp || b == s.semi {
            fields.push(std::mem::take(&mut cur));
            i += 1;
        } else if b == s.plus {
            cur.push(s.blank);
            i += 1;
        } else if b == s.pct {
            if i + 2 < record.len() {
                if let (Some(hi), Some(lo)) = (
                    hex_digit(record[i + 1], ebcdic),
                    hex_digit(record[i + 2], ebcdic),
                ) {
                    cur.push((hi << 4) | lo);
                    i += 3;
                    continue;
                }
            }
            // Incomplete or non-hex `%` is copied unchanged (p.578 does not say).
            cur.push(b);
            i += 1;
        } else {
            cur.push(b);
            i += 1;
        }
    }
    fields.push(cur);
    fields
}

fn run_urldeblock(ctx: &mut StageCtx, ebcdic: bool) {
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::PRIMARY.0 {
            ctx.fail("URLDEBLOCK: no input stream other than the primary may be connected");
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

    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if rec.is_empty() {
                // Null input records are discarded (p.578).
                let _ = ctx.read();
                continue;
            }
            let fields = deblock_url(rec.as_bytes(), ebcdic);
            for field in fields {
                if !ctx.output_connected(StreamId::PRIMARY) {
                    if ctx.input_connected(StreamId::PRIMARY) {
                        ctx.short_input();
                    }
                    return;
                }
                if ctx.write(Record::from(field)).is_err() {
                    if ctx.input_connected(StreamId::PRIMARY) {
                        ctx.short_input();
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_sample_url_fields() {
        let joined =
            b"name=Craig%20R.%20Doe&class=90&grad=&email=xyz@prodigy.com&bdate=03%2F15%2F68&url=";
        let got: Vec<String> = deblock_url(joined, false)
            .into_iter()
            .map(|f| String::from_utf8(f).unwrap())
            .collect();
        assert_eq!(
            got,
            vec![
                "name=Craig R. Doe",
                "class=90",
                "grad=",
                "email=xyz@prodigy.com",
                "bdate=03/15/68",
                "url=",
            ]
        );
    }

    #[test]
    fn plus_blank_percent_plus_stays_plus() {
        // Raw `+` → blank; `%2b` / `%2B` → ASCII plus, not a blank.
        let got = deblock_url(b"a+b=%2b%2B", false);
        assert_eq!(got, vec![b"a b=++".to_vec()]);
    }

    #[test]
    fn semicolon_and_ampersand_split() {
        let got = deblock_url(b"a&b;c", false);
        assert_eq!(got, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn encoded_ampersand_does_not_split() {
        let got = deblock_url(b"a=%26b&c=1", false);
        assert_eq!(got, vec![b"a=&b".to_vec(), b"c=1".to_vec()]);
    }

    #[test]
    fn incomplete_percent_is_literal() {
        assert_eq!(deblock_url(b"a%", false), vec![b"a%".to_vec()]);
        assert_eq!(deblock_url(b"a%2", false), vec![b"a%2".to_vec()]);
        assert_eq!(deblock_url(b"a%zz", false), vec![b"a%zz".to_vec()]);
    }

    #[test]
    fn ebcdic_specials() {
        // name=Craig%20R  in CP037, then EBCDIC `&` `class=90`
        let mut rec = Vec::new();
        for &b in b"name=Craig%20R" {
            rec.push(ascii_to_ebcdic(b));
        }
        rec.push(EBCDIC_AMP);
        for &b in b"class=90" {
            rec.push(ascii_to_ebcdic(b));
        }
        let got = deblock_url(&rec, true);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], {
            // `%20` inserts raw X'20', not EBCDIC blank X'40' (Note 2).
            let mut v = Vec::new();
            for &b in b"name=Craig" {
                v.push(ascii_to_ebcdic(b));
            }
            v.push(0x20);
            v.push(ascii_to_ebcdic(b'R'));
            v
        });
        assert_eq!(got[1], {
            let mut v = Vec::new();
            for &b in b"class=90" {
                v.push(ascii_to_ebcdic(b));
            }
            v
        });
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
            b'%' => EBCDIC_PCT,
            b' ' => EBCDIC_BLANK,
            other => other,
        }
    }
}
