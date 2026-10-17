//! PICK — Author's Edition 1.1.11, pp. 454–456 (tutorial pp.53–54, 68).
//!
//! Syntax (fixed order):
//! `PICK [NOPAD | PAD xorc] [ANYcase]
//!      (inputRange | delimitedString) operator (inputRange | delimitedString)`
//!
//! String (REXX strict) operators: `==` `¬==` `<<` `<<=` `>>` `>>=`.
//! Numeric operators: `=` `¬=` `<` `<=` `>` `>=`.
//! Note 1 synonyms: `\==` `/==` for `¬==`; `\=` `/=` for `¬=`; caret (`^`) maps `¬`.

use std::cmp::Ordering;

use pipe_core::{StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase, matches_abbrev};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, parse_xorc, InputRange,
};

pub fn pick(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_pick(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("PICK: {e}"));
                return;
            }
        };

        if ctx.input_connected(StreamId::SECONDARY) {
            ctx.fail("PICK: secondary input stream must not be connected");
            return;
        }

        if !outputs_live(ctx) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        while outputs_live(ctx) {
            let rec = match ctx.read() {
                Ok(Some(r)) => r,
                Ok(None) | Err(_) => return,
            };
            let selected = match evaluate(rec.as_bytes(), &spec) {
                Ok(v) => v,
                Err(e) => {
                    ctx.fail(format!("PICK: {e}"));
                    return;
                }
            };
            let stream = if selected {
                StreamId::PRIMARY
            } else {
                StreamId::SECONDARY
            };
            if ctx.output_connected(stream) {
                ctx.select_output(stream);
                if ctx.write(rec).is_err() && !outputs_live(ctx) {
                    return;
                }
            }
        }
    })
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    StrictEq,
    StrictNe,
    StrictLt,
    StrictLe,
    StrictGt,
    StrictGe,
    NumEq,
    NumNe,
    NumLt,
    NumLe,
    NumGt,
    NumGe,
}

impl Op {
    fn is_numeric(self) -> bool {
        matches!(
            self,
            Op::NumEq | Op::NumNe | Op::NumLt | Op::NumLe | Op::NumGt | Op::NumGe
        )
    }
}

#[derive(Clone, Debug)]
enum Side {
    Range(InputRange),
    Literal(Vec<u8>),
}

struct PickSpec {
    pad: Option<u8>,
    anycase: bool,
    left: Side,
    op: Op,
    right: Side,
}

fn parse_pick(args: &[String]) -> Result<PickSpec, String> {
    let mut i = 0;

    // [NOPAD | PAD xorc] — default NOPAD; whole words (p.454).
    let mut pad = None;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "PAD" {
            i += 1;
            if i >= args.len() {
                return Err("PAD requires an xorc".into());
            }
            pad = Some(parse_xorc(&args[i])?);
            i += 1;
        } else if upper == "NOPAD" {
            i += 1;
        }
    }

    // [ANYcase] — min 3 (no ANYOF on this stage). Synonyms Note 3.
    let mut anycase = false;
    if i < args.len() && is_pick_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    if i >= args.len() {
        return Err("missing input range or delimited string".into());
    }
    let left = parse_side(args, &mut i)?;
    if i >= args.len() {
        return Err("missing relational operator".into());
    }
    let op = parse_operator(&args[i]).ok_or_else(|| unexpected_operand(&args[i]))?;
    i += 1;
    if i >= args.len() {
        return Err("missing input range or delimited string after the operator".into());
    }
    let right = parse_side(args, &mut i)?;
    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    Ok(PickSpec {
        pad,
        anycase,
        left,
        op,
        right,
    })
}

fn parse_side(args: &[String], i: &mut usize) -> Result<Side, String> {
    if *i >= args.len() {
        return Err("missing input range or delimited string".into());
    }
    if pipe_parser::is_dstring(&args[*i]) {
        let bytes = decode_dstring(&args[*i])?;
        *i += 1;
        return Ok(Side::Literal(bytes));
    }
    if looks_like_input_range_start(&args[*i]) {
        let range = parse_input_range(args, i)?;
        return Ok(Side::Range(range));
    }
    Err(unexpected_operand(&args[*i]))
}

fn parse_operator(s: &str) -> Option<Op> {
    match s {
        "==" => Some(Op::StrictEq),
        "<<" => Some(Op::StrictLt),
        "<<=" => Some(Op::StrictLe),
        ">>" => Some(Op::StrictGt),
        ">>=" => Some(Op::StrictGe),
        "=" => Some(Op::NumEq),
        "<" => Some(Op::NumLt),
        "<=" => Some(Op::NumLe),
        ">" => Some(Op::NumGt),
        ">=" => Some(Op::NumGe),
        _ => {
            if is_strict_ne(s) {
                Some(Op::StrictNe)
            } else if is_num_ne(s) {
                Some(Op::NumNe)
            } else {
                None
            }
        }
    }
}

fn is_strict_ne(s: &str) -> bool {
    s == "\\==" || s == "/==" || s == "^==" || s == "\u{00AC}=="
}

fn is_num_ne(s: &str) -> bool {
    s == "\\=" || s == "/=" || s == "^=" || s == "\u{00AC}="
}

fn is_pick_anycase(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    matches_abbrev(&u, "ANYCASE", 3)
        || u == "CASEANY"
        || u == "IGNORECASE"
        || u == "CASEIGNORE"
        || is_anycase(s)
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if upper == "AND" || upper == "OR" {
        return format!("{upper} is not a PICK operator; cascade pick stages (1.1.11 p.456)");
    }
    if upper == "NOT" {
        return "NOT is not a PICK keyword; use ¬== / ¬= (and \\== /== ^==, \\= /= ^=)".into();
    }
    if upper == "PAD" || upper == "NOPAD" || is_pick_anycase(arg) || parse_operator(arg).is_some() {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn evaluate(record: &[u8], spec: &PickSpec) -> Result<bool, String> {
    let left = resolve(record, &spec.left);
    let right = resolve(record, &spec.right);
    if spec.op.is_numeric() {
        let ln = parse_fp(&left)
            .map_err(|e| format!("left field is not a floating-point number ({e})"))?;
        let rn = parse_fp(&right)
            .map_err(|e| format!("right field is not a floating-point number ({e})"))?;
        Ok(num_holds(spec.op, &ln, &rn))
    } else {
        Ok(str_holds(spec.op, &left, &right, spec.pad, spec.anycase))
    }
}

fn resolve(record: &[u8], side: &Side) -> Vec<u8> {
    match side {
        Side::Literal(b) => b.clone(),
        Side::Range(r) => extract_input_range(record, r),
    }
}

fn str_holds(op: Op, left: &[u8], right: &[u8], pad: Option<u8>, anycase: bool) -> bool {
    let ord = cmp_bytes(left, right, pad, anycase);
    match op {
        Op::StrictEq => ord == Ordering::Equal,
        Op::StrictNe => ord != Ordering::Equal,
        Op::StrictLt => ord == Ordering::Less,
        Op::StrictLe => ord != Ordering::Greater,
        Op::StrictGt => ord == Ordering::Greater,
        Op::StrictGe => ord != Ordering::Less,
        _ => unreachable!("numeric op in str_holds"),
    }
}

fn cmp_bytes(left: &[u8], right: &[u8], pad: Option<u8>, anycase: bool) -> Ordering {
    let mut a = left.to_vec();
    let mut b = right.to_vec();
    if anycase {
        for x in &mut a {
            *x = x.to_ascii_uppercase();
        }
        for x in &mut b {
            *x = x.to_ascii_uppercase();
        }
    }
    if let Some(p) = pad {
        if a.len() < b.len() {
            a.resize(b.len(), p);
        } else if b.len() < a.len() {
            b.resize(a.len(), p);
        }
        a.cmp(&b)
    } else {
        // NOPAD: unpaired bytes of the longer string compare high (p.455).
        let n = a.len().min(b.len());
        match a[..n].cmp(&b[..n]) {
            Ordering::Equal => a.len().cmp(&b.len()),
            o => o,
        }
    }
}

/// Decimal floating-point from the FPnumber railroad (1.1.11 p.632).
#[derive(Clone, Debug, PartialEq, Eq)]
struct FpNum {
    negative: bool,
    /// Significant digits, no leading or trailing zeros (`"0"` for zero).
    digits: String,
    /// `value = digits * 10^exp` (after normalize).
    exp: i32,
}

impl FpNum {
    fn is_zero(&self) -> bool {
        self.digits == "0"
    }
}

fn parse_fp(bytes: &[u8]) -> Result<FpNum, String> {
    let s = std::str::from_utf8(bytes).map_err(|_| "not a floating-point number".to_string())?;
    if s.is_empty() {
        return Err("empty field".into());
    }
    if s.as_bytes().iter().any(|b| b.is_ascii_whitespace()) {
        return Err("blanks are not allowed in a floating-point number (1.1.11 p.632)".into());
    }
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut negative = false;
    if i < chars.len() && chars[i] == '+' {
        i += 1;
    } else if i < chars.len() && chars[i] == '-' {
        negative = true;
        i += 1;
    }

    let mut digits = String::new();
    let mut frac_len: i32 = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;

    while i < chars.len() && chars[i].is_ascii_digit() {
        digits.push(chars[i]);
        seen_digit = true;
        i += 1;
    }
    if i < chars.len() && chars[i] == '.' {
        seen_dot = true;
        i += 1;
        let frac_start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            digits.push(chars[i]);
            seen_digit = true;
            i += 1;
        }
        frac_len = (i - frac_start) as i32;
        // Leading-dot form (`.5`) needs at least one fraction digit (p.632).
        if !seen_digit {
            return Err("not a floating-point number".into());
        }
        if digits.is_empty() && frac_len == 0 {
            return Err("not a floating-point number".into());
        }
    }
    if !seen_digit {
        return Err("not a floating-point number".into());
    }
    // Integer-only with a trailing period (`1.`) is allowed by the railroad.
    let _ = seen_dot;

    let mut exp = -frac_len;
    if i < chars.len() && (chars[i] == 'E' || chars[i] == 'e') {
        i += 1;
        let mut exp_neg = false;
        if i < chars.len() && chars[i] == '+' {
            i += 1;
        } else if i < chars.len() && chars[i] == '-' {
            exp_neg = true;
            i += 1;
        }
        if i >= chars.len() || !chars[i].is_ascii_digit() {
            return Err("exponent requires at least one digit".into());
        }
        let mut e: i32 = 0;
        while i < chars.len() && chars[i].is_ascii_digit() {
            let d = chars[i].to_digit(10).unwrap() as i32;
            e = e
                .checked_mul(10)
                .and_then(|v| v.checked_add(d))
                .ok_or_else(|| "exponent overflow".to_string())?;
            i += 1;
        }
        if exp_neg {
            e = -e;
        }
        exp = exp
            .checked_add(e)
            .ok_or_else(|| "exponent overflow".to_string())?;
    }
    if i != chars.len() {
        return Err("not a floating-point number".into());
    }
    Ok(normalize(digits, exp, negative))
}

fn normalize(mut digits: String, mut exp: i32, negative: bool) -> FpNum {
    let lead = digits.bytes().take_while(|&b| b == b'0').count();
    if lead == digits.len() {
        return FpNum {
            negative,
            digits: "0".into(),
            exp: 0,
        };
    }
    if lead > 0 {
        digits = digits[lead..].to_string();
    }
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
        exp += 1;
    }
    FpNum {
        negative,
        digits,
        exp,
    }
}

fn mag_cmp(a: &FpNum, b: &FpNum) -> Ordering {
    if a.is_zero() && b.is_zero() {
        return Ordering::Equal;
    }
    if a.is_zero() {
        return Ordering::Less;
    }
    if b.is_zero() {
        return Ordering::Greater;
    }
    let a_lead = a.digits.len() as i64 - 1 + i64::from(a.exp);
    let b_lead = b.digits.len() as i64 - 1 + i64::from(b.exp);
    match a_lead.cmp(&b_lead) {
        Ordering::Equal => {
            let n = a.digits.len().max(b.digits.len());
            let mut aa = a.digits.clone();
            let mut bb = b.digits.clone();
            aa.extend(std::iter::repeat('0').take(n - aa.len()));
            bb.extend(std::iter::repeat('0').take(n - bb.len()));
            aa.cmp(&bb)
        }
        o => o,
    }
}

fn signed_cmp(a: &FpNum, b: &FpNum) -> Ordering {
    let a_neg = a.negative && !a.is_zero();
    let b_neg = b.negative && !b.is_zero();
    match (a_neg, b_neg) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => mag_cmp(a, b).reverse(),
        (false, false) => mag_cmp(a, b),
    }
}

fn num_holds(op: Op, left: &FpNum, right: &FpNum) -> bool {
    match op {
        // Same sign and exactly equal (p.455). 0 and -0 differ in sign.
        Op::NumEq => left.negative == right.negative && mag_cmp(left, right) == Ordering::Equal,
        Op::NumNe => !(left.negative == right.negative && mag_cmp(left, right) == Ordering::Equal),
        Op::NumLt => signed_cmp(left, right) == Ordering::Less,
        Op::NumLe => signed_cmp(left, right) != Ordering::Greater,
        Op::NumGt => signed_cmp(left, right) == Ordering::Greater,
        Op::NumGe => signed_cmp(left, right) != Ordering::Less,
        _ => unreachable!("strict op in num_holds"),
    }
}

#[cfg(test)]
mod fp_tests {
    use super::*;

    fn n(s: &str) -> FpNum {
        parse_fp(s.as_bytes()).expect(s)
    }

    #[test]
    fn fp_normalize_and_eq() {
        assert!(num_holds(Op::NumEq, &n("1"), &n("1.0")));
        assert!(num_holds(Op::NumEq, &n("1"), &n("1e0")));
        assert!(num_holds(Op::NumEq, &n("12.30"), &n("12.3")));
        assert!(num_holds(Op::NumEq, &n("100"), &n("1e2")));
        assert!(num_holds(Op::NumEq, &n(".5"), &n("0.5")));
        assert!(!num_holds(Op::NumEq, &n("0"), &n("-0")));
        assert!(num_holds(Op::NumNe, &n("0"), &n("-0")));
        assert!(num_holds(Op::NumLt, &n("-1"), &n("0")));
        assert!(num_holds(Op::NumGt, &n("13"), &n("12.3")));
        assert!(num_holds(Op::NumGt, &n("100"), &n("99")));
        assert!(parse_fp(b"").is_err());
        assert!(parse_fp(b" 1").is_err());
        assert!(parse_fp(b"abc").is_err());
        assert!(parse_fp(b"1e").is_err());
        assert!(parse_fp(b".").is_err());
    }
}
