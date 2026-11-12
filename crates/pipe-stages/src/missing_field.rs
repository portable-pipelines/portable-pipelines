//! Missing `$field` notes. The extract still returns empty, matching SPEC.

use pipe_core::StageCtx;

use crate::input_range::{InputRange, Units};
use crate::json_row::{format_path, lookup_json_field, parse_json_object};

/// A `$field` path that was not on this JSON object record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingJsonField {
    /// Operand spelling, including `$` (`$owner_i`).
    pub spelled: String,
    /// Top-level keys the record does have, in object order.
    pub present: Vec<String>,
}

/// If `range` names a JSON field that this object record does not have.
pub fn missing_json_field(record: &[u8], range: &InputRange) -> Option<MissingJsonField> {
    if !range.has_json_field() {
        return None;
    }
    let Ok(value) = parse_json_object(record) else {
        return None;
    };
    let present = match value.as_object() {
        Some(map) => map.keys().cloned().collect(),
        None => return None,
    };
    for part in &range.parts {
        let Units::JsonField { path } = &part.units else {
            continue;
        };
        match lookup_json_field(record, path) {
            Ok(None) => {
                return Some(MissingJsonField {
                    spelled: format!("${}", format_path(path)),
                    present,
                });
            }
            Ok(Some(_)) => {}
            Err(_) => return None,
        }
    }
    None
}

/// Closest field name among `present`, if one is close enough to be useful.
pub fn suggest_field(want: &str, present: &[String]) -> Option<String> {
    let want_l = want.to_ascii_lowercase();
    let mut best: Option<(usize, bool, &str)> = None;
    for key in present {
        let key_l = key.to_ascii_lowercase();
        if key_l == want_l {
            return Some(key.clone());
        }
        let prefix = key_l.starts_with(&want_l) || want_l.starts_with(&key_l);
        let dist = edit_distance(&want_l, &key_l);
        let ok = prefix && want_l.len().abs_diff(key_l.len()) <= 4 || dist <= 2;
        if !ok {
            continue;
        }
        let rank = if prefix { dist.min(1) } else { dist };
        match best {
            None => best = Some((rank, prefix, key)),
            Some((r, p, _)) if rank < r || (rank == r && prefix && !p) => {
                best = Some((rank, prefix, key));
            }
            _ => {}
        }
    }
    best.map(|(_, _, k)| k.to_string())
}

/// Plain-language note. `stage` is the 1-based spec position when known.
pub fn missing_field_message(verb: &str, stage: Option<u32>, miss: &MissingJsonField) -> String {
    let where_ = match stage {
        Some(n) => format!("{verb} (stage {n})"),
        None => verb.to_string(),
    };
    let fields = if miss.present.is_empty() {
        "no fields".to_string()
    } else {
        miss.present.join(", ")
    };
    let mut msg = format!(
        "{where_}: field {} not found in input record; it has {fields}",
        miss.spelled
    );
    let want = miss.spelled.strip_prefix('$').unwrap_or(&miss.spelled);
    if let Some(hint) = suggest_field(want, &miss.present) {
        msg.push_str(&format!("; did you mean {hint}?"));
    }
    msg
}

/// Warn once per distinct missing field on this stage. Extract is unchanged.
pub fn note_missing_json_field(ctx: &mut StageCtx, verb: &str, record: &[u8], range: &InputRange) {
    let Some(miss) = missing_json_field(record, range) else {
        return;
    };
    ctx.note(missing_field_message(verb, ctx.origin_stage(), &miss));
}

/// Extract a range, noting a missing `$field` first.
pub fn extract_range_noted(
    ctx: &mut StageCtx,
    verb: &str,
    record: &[u8],
    range: &InputRange,
) -> Result<Vec<u8>, String> {
    note_missing_json_field(ctx, verb, record, range);
    crate::input_range::extract_input_range_checked(record, range)
}

/// Extract an optional key range (`None` means the whole record).
pub fn extract_key_noted(
    ctx: &mut StageCtx,
    verb: &str,
    record: &[u8],
    range: Option<&InputRange>,
) -> Result<Vec<u8>, String> {
    match range {
        None => Ok(record.to_vec()),
        Some(r) => extract_range_noted(ctx, verb, record, r),
    }
}

/// `$field` tokens in operand text, as byte offsets into `text`.
pub fn json_field_tokens(text: &str) -> Vec<(usize, usize, String)> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() && (b[i + 1].is_ascii_alphabetic() || b[i + 1] == b'_') {
            let start = i;
            i += 2;
            loop {
                if i >= b.len() {
                    break;
                }
                let c = b[i];
                if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
                    i += 1;
                    continue;
                }
                if c == b'[' {
                    let mut k = i + 1;
                    if k < b.len() && b[k] == b'-' {
                        k += 1;
                    }
                    let digits = k;
                    while k < b.len() && b[k].is_ascii_digit() {
                        k += 1;
                    }
                    if k > digits && k < b.len() && b[k] == b']' {
                        i = k + 1;
                        continue;
                    }
                }
                break;
            }
            out.push((start, i, text[start..i].to_string()));
            continue;
        }
        i += 1;
    }
    out
}

/// Short Properties/Source hint when `spelled` is not among `present` keys.
pub fn operand_field_hint(spelled: &str, present: &[String]) -> Option<String> {
    let want = spelled.strip_prefix('$').unwrap_or(spelled);
    let top = want.split(['.', '[']).next().unwrap_or(want);
    if present.iter().any(|k| k == top) {
        return None;
    }
    let mut msg = format!("no field {top} in the input");
    if let Some(hint) = suggest_field(top, present) {
        msg.push_str(&format!("; did you mean {hint}?"));
    }
    Some(msg)
}

/// Same as [`extract_range_noted`], returning empty on a non-object `$field`.
pub fn extract_range_noted_lossy(
    ctx: &mut StageCtx,
    verb: &str,
    record: &[u8],
    range: &InputRange,
) -> Vec<u8> {
    extract_range_noted(ctx, verb, record, range).unwrap_or_default()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            let ins = cur[j] + 1;
            let del = prev[j + 1] + 1;
            let sub = prev[j] + cost;
            cur.push(ins.min(del).min(sub));
        }
        prev = cur;
    }
    *prev.last().unwrap_or(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_range::parse_input_range;

    fn range(words: &[&str]) -> InputRange {
        let args: Vec<String> = words.iter().map(|s| (*s).to_string()).collect();
        let mut i = 0;
        parse_input_range(&args, &mut i).expect("range")
    }

    #[test]
    fn missing_owner_i_lists_real_keys_and_suggests() {
        let rec = br#"{"owner_id":"O1","name":"Mara"}"#;
        let miss = missing_json_field(rec, &range(&["$owner_i"])).expect("missing");
        assert_eq!(miss.spelled, "$owner_i");
        assert_eq!(miss.present, vec!["owner_id", "name"]);
        let msg = missing_field_message("LOOKUP", Some(3), &miss);
        assert!(
            msg.contains("LOOKUP (stage 3): field $owner_i not found"),
            "{msg}"
        );
        assert!(msg.contains("owner_id, name"), "{msg}");
        assert!(msg.contains("did you mean owner_id?"), "{msg}");
    }

    #[test]
    fn present_field_is_not_missing() {
        let rec = br#"{"owner_id":"O1"}"#;
        assert!(missing_json_field(rec, &range(&["$owner_id"])).is_none());
    }

    #[test]
    fn column_range_is_not_a_json_field() {
        let rec = br#"{"owner_id":"O1"}"#;
        assert!(missing_json_field(rec, &range(&["1-5"])).is_none());
    }

    #[test]
    fn suggest_picks_prefix() {
        let keys = ["owner_id".into(), "name".into()];
        assert_eq!(suggest_field("owner_i", &keys).as_deref(), Some("owner_id"));
    }

    #[test]
    fn tokens_and_operand_hint_for_owner_i() {
        let ops = " $owner_i $owner_id $addr.city";
        let toks = json_field_tokens(ops);
        assert_eq!(
            toks.iter().map(|t| t.2.as_str()).collect::<Vec<_>>(),
            vec!["$owner_i", "$owner_id", "$addr.city"]
        );
        let keys = ["owner_id".into(), "name".into(), "addr".into()];
        assert_eq!(
            operand_field_hint("$owner_i", &keys).as_deref(),
            Some("no field owner_i in the input; did you mean owner_id?")
        );
        assert!(operand_field_hint("$owner_id", &keys).is_none());
        assert!(operand_field_hint("$addr.city", &keys).is_none());
    }
}
