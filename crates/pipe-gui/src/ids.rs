//! Stable graph node ids that survive source edits while the stage still exists.

use pipe_parser::SourceSpan;

pub type NodeId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageKey {
    pub id: NodeId,
    pub verb: String,
    pub span: SourceSpan,
}

fn verb_key(verb: &str) -> String {
    verb.to_ascii_uppercase()
}

fn overlap(a: SourceSpan, b: SourceSpan) -> usize {
    let start = a.start.max(b.start);
    let end = a.end.min(b.end);
    end.saturating_sub(start)
}

/// Match incoming stages to previous ids.
///
/// 1. Same verb and overlapping source span (largest overlap, then nearest start).
/// 2. Remaining same-verb stages in document order.
/// 3. New ids for unmatched incoming stages.
///
/// Different verbs never share an id.
pub fn match_stage_ids(
    previous: &[StageKey],
    incoming: &[(String, SourceSpan)],
    next_id: &mut NodeId,
) -> Vec<NodeId> {
    let mut used = vec![false; previous.len()];
    let mut assigned = vec![false; incoming.len()];
    let mut out = vec![0; incoming.len()];

    for (i, (verb, span)) in incoming.iter().enumerate() {
        let vk = verb_key(verb);
        let mut best: Option<(usize, usize, usize)> = None;
        for (j, prev) in previous.iter().enumerate() {
            if used[j] || verb_key(&prev.verb) != vk {
                continue;
            }
            let ov = overlap(prev.span, *span);
            if ov == 0 {
                continue;
            }
            let dist = prev.span.start.abs_diff(span.start);
            let better = match best {
                None => true,
                Some((_, bov, bdist)) => ov > bov || (ov == bov && dist < bdist),
            };
            if better {
                best = Some((j, ov, dist));
            }
        }
        if let Some((j, _, _)) = best {
            used[j] = true;
            assigned[i] = true;
            out[i] = previous[j].id;
        }
    }

    for (i, (verb, _)) in incoming.iter().enumerate() {
        if assigned[i] {
            continue;
        }
        let vk = verb_key(verb);
        if let Some(j) = previous
            .iter()
            .enumerate()
            .position(|(j, prev)| !used[j] && verb_key(&prev.verb) == vk)
        {
            used[j] = true;
            assigned[i] = true;
            out[i] = previous[j].id;
        }
    }

    for (i, slot) in out.iter_mut().enumerate() {
        if assigned[i] {
            continue;
        }
        *slot = *next_id;
        *next_id = next_id.saturating_add(1);
        if *next_id == 0 {
            *next_id = 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: u64, verb: &str, start: usize, end: usize) -> StageKey {
        StageKey {
            id,
            verb: verb.into(),
            span: SourceSpan::new(start, end),
        }
    }

    fn incoming(verb: &str, start: usize, end: usize) -> (String, SourceSpan) {
        (verb.into(), SourceSpan::new(start, end))
    }

    #[test]
    fn insert_in_the_middle_keeps_later_ids() {
        // "literal a | hole" then "literal a | spec | hole"
        let prev = vec![key(1, "LITERAL", 0, 9), key(2, "HOLE", 12, 16)];
        let incoming = vec![
            incoming("LITERAL", 0, 9),
            incoming("SPEC", 12, 16),
            incoming("HOLE", 19, 23),
        ];
        let mut next = 3;
        let ids = match_stage_ids(&prev, &incoming, &mut next);
        assert_eq!(ids, vec![1, 3, 2]);
        assert_eq!(next, 4);
    }

    #[test]
    fn overlapping_span_beats_order_for_the_same_verb() {
        let prev = vec![
            key(1, "LITERAL", 0, 10),
            key(2, "LITERAL", 13, 23),
            key(3, "HOLE", 26, 30),
        ];
        let incoming = vec![
            incoming("LITERAL", 0, 11),
            incoming("LITERAL", 14, 24),
            incoming("HOLE", 27, 31),
        ];
        let mut next = 4;
        let ids = match_stage_ids(&prev, &incoming, &mut next);
        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(next, 4);
    }

    #[test]
    fn changing_the_verb_allocates_a_new_id() {
        let prev = vec![key(1, "LITERAL", 0, 9), key(2, "HOLE", 12, 16)];
        let incoming = vec![incoming("LITERAL", 0, 9), incoming("CONSOLE", 12, 19)];
        let mut next = 3;
        let ids = match_stage_ids(&prev, &incoming, &mut next);
        assert_eq!(ids, vec![1, 3]);
        assert_eq!(next, 4);
    }

    #[test]
    fn delete_then_order_match_same_verb() {
        let prev = vec![
            key(1, "LITERAL", 0, 9),
            key(2, "SPEC", 12, 16),
            key(3, "HOLE", 19, 23),
        ];
        let incoming = vec![incoming("LITERAL", 0, 9), incoming("HOLE", 12, 16)];
        let mut next = 4;
        let ids = match_stage_ids(&prev, &incoming, &mut next);
        assert_eq!(ids, vec![1, 3]);
    }
}
