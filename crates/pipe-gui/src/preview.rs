//! Bounded record previews. No execution. Structure comes from the bytes
//! themselves (JSON objects) or is treated as a single text cell — never guessed CSV.

use pipe_core::{DebugEvent, DebugEventKind, RecordPreview, SlotSnapshot, StageId, StreamId};

const TABLE_ROWS: usize = 16;
const TABLE_COLS: usize = 12;
const CELL_CHARS: usize = 48;
const HOVER_RECENT: usize = 3;
const PANE_RECENT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordFocus {
    Stage(StageId),
    /// One connection. Outgoing history is the writer's writes on `writer_stream` only.
    Edge {
        writer: StageId,
        writer_stream: u32,
        reader: StageId,
        reader_stream: u32,
    },
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusCounts {
    pub received: usize,
    pub sent: usize,
    pub discarded: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingKind {
    Waiting,
    Empty,
    WriterFinished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingItem {
    pub kind: WaitingKind,
    pub role: &'static str,
    pub record: Option<RecordPreview>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedKind {
    CameIn,
    WentOut,
    Discarded,
    InputEnded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedItem {
    pub kind: ObservedKind,
    pub stream: u32,
    pub record: Option<RecordPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiniCell {
    Text(String),
    Nested { summary: String, json: String },
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiniKind {
    /// Unstructured bytes shown as one column of rows. No field names.
    Text,
    /// JSON object or array of objects — keys are the real fields.
    Structured,
    Binary,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniTable {
    pub kind: MiniKind,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<MiniCell>>,
    pub truncated: bool,
}

pub fn hover_recent_limit() -> usize {
    HOVER_RECENT
}

pub fn pane_recent_limit() -> usize {
    PANE_RECENT
}

pub fn counts_for_focus(events: &[DebugEvent], focus: RecordFocus) -> FocusCounts {
    match focus {
        RecordFocus::Stage(id) => {
            let mut c = FocusCounts::default();
            for e in events.iter().filter(|e| e.stage == id) {
                match e.kind {
                    DebugEventKind::ReadTaken { .. } => c.received += 1,
                    DebugEventKind::WriteOffered { .. } => c.sent += 1,
                    DebugEventKind::WriteDiscard { .. } => c.discarded += 1,
                    _ => {}
                }
            }
            c
        }
        RecordFocus::Edge {
            writer,
            writer_stream,
            ..
        } => {
            let mut c = FocusCounts::default();
            for e in events.iter().filter(|e| e.stage == writer) {
                match &e.kind {
                    DebugEventKind::WriteOffered { stream, .. } if stream.0 == writer_stream => {
                        c.sent += 1;
                    }
                    DebugEventKind::WriteDiscard { stream, .. } if stream.0 == writer_stream => {
                        c.discarded += 1;
                    }
                    _ => {}
                }
            }
            c
        }
    }
}

pub fn waiting_items(slots: &[SlotSnapshot], focus: RecordFocus) -> Vec<WaitingItem> {
    match focus {
        RecordFocus::Stage(id) => slots
            .iter()
            .filter(|s| s.writer == id || s.reader == id)
            .map(|s| waiting_from_slot(s, if s.writer == id { "output" } else { "input" }))
            .collect(),
        RecordFocus::Edge {
            writer,
            writer_stream,
            reader,
            reader_stream,
        } => slots
            .iter()
            .filter(|s| {
                s.writer == writer
                    && s.reader == reader
                    && s.writer_stream.0 == writer_stream
                    && s.reader_stream.0 == reader_stream
            })
            .map(|s| waiting_from_slot(s, "on this connection"))
            .collect(),
    }
}

fn waiting_from_slot(slot: &SlotSnapshot, role: &'static str) -> WaitingItem {
    let kind = if slot.record.is_some() {
        WaitingKind::Waiting
    } else if slot.severed || slot.writer_done {
        WaitingKind::WriterFinished
    } else {
        WaitingKind::Empty
    };
    WaitingItem {
        kind,
        role,
        record: slot.record.clone(),
    }
}

/// Recent observations in the scope of `focus`. Edge scope is writer *outgoing*
/// on that stream only — never the writer's input, never the reader's input.
pub fn recent_items(events: &[DebugEvent], focus: RecordFocus, n: usize) -> Vec<ObservedItem> {
    let mut out: Vec<ObservedItem> = match focus {
        RecordFocus::Stage(id) => events
            .iter()
            .filter(|e| e.stage == id)
            .filter_map(observed_from_event)
            .collect(),
        RecordFocus::Edge {
            writer,
            writer_stream,
            ..
        } => events
            .iter()
            .filter(|e| e.stage == writer)
            .filter_map(|e| match &e.kind {
                DebugEventKind::WriteOffered { record, stream } if stream.0 == writer_stream => {
                    Some(ObservedItem {
                        kind: ObservedKind::WentOut,
                        stream: stream.0,
                        record: Some(record.clone()),
                    })
                }
                DebugEventKind::WriteDiscard { record, stream } if stream.0 == writer_stream => {
                    Some(ObservedItem {
                        kind: ObservedKind::Discarded,
                        stream: stream.0,
                        record: Some(record.clone()),
                    })
                }
                _ => None,
            })
            .collect(),
    };
    if out.len() > n {
        out = out.split_off(out.len() - n);
    }
    out
}

fn observed_from_event(e: &DebugEvent) -> Option<ObservedItem> {
    match &e.kind {
        DebugEventKind::ReadTaken { record, stream } => Some(ObservedItem {
            kind: ObservedKind::CameIn,
            stream: stream.0,
            record: Some(record.clone()),
        }),
        DebugEventKind::WriteOffered { record, stream } => Some(ObservedItem {
            kind: ObservedKind::WentOut,
            stream: stream.0,
            record: Some(record.clone()),
        }),
        DebugEventKind::WriteDiscard { record, stream } => Some(ObservedItem {
            kind: ObservedKind::Discarded,
            stream: stream.0,
            record: Some(record.clone()),
        }),
        DebugEventKind::ReadEof { stream } => Some(ObservedItem {
            kind: ObservedKind::InputEnded,
            stream: stream.0,
            record: None,
        }),
        _ => None,
    }
}

pub fn mini_table(rec: &RecordPreview) -> MiniTable {
    let truncated = rec.len > rec.bytes.len();
    if rec.len == 0 && rec.bytes.is_empty() {
        return MiniTable {
            kind: MiniKind::Empty,
            headers: Vec::new(),
            rows: Vec::new(),
            truncated,
        };
    }
    match std::str::from_utf8(&rec.bytes) {
        Ok(s) => text_or_structured(s, truncated),
        Err(_) => MiniTable {
            kind: MiniKind::Binary,
            headers: Vec::new(),
            rows: vec![vec![MiniCell::Text(hex_preview(&rec.bytes))]],
            truncated,
        },
    }
}

fn text_or_structured(s: &str, truncated: bool) -> MiniTable {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        if let Some(table) = structured_table(&v, truncated) {
            return table;
        }
    }
    MiniTable {
        kind: MiniKind::Text,
        headers: Vec::new(),
        rows: vec![vec![MiniCell::Text(clip(s))]],
        truncated,
    }
}

fn structured_table(v: &serde_json::Value, truncated: bool) -> Option<MiniTable> {
    match v {
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                return Some(MiniTable {
                    kind: MiniKind::Structured,
                    headers: Vec::new(),
                    rows: Vec::new(),
                    truncated,
                });
            }
            let headers: Vec<String> = map.keys().cloned().collect();
            if headers.len() > TABLE_COLS {
                return None;
            }
            Some(MiniTable {
                kind: MiniKind::Structured,
                headers: headers.clone(),
                rows: vec![headers
                    .iter()
                    .map(|k| cell_from_json(map.get(k).unwrap_or(&serde_json::Value::Null)))
                    .collect()],
                truncated,
            })
        }
        serde_json::Value::Array(arr) if !arr.is_empty() && arr.iter().all(|x| x.is_object()) => {
            let mut headers = Vec::new();
            for item in arr.iter().take(TABLE_ROWS) {
                if let Some(map) = item.as_object() {
                    for k in map.keys() {
                        if !headers.iter().any(|e| e == k) {
                            headers.push(k.clone());
                        }
                    }
                }
            }
            if headers.is_empty() || headers.len() > TABLE_COLS {
                return None;
            }
            let rows = arr
                .iter()
                .take(TABLE_ROWS)
                .map(|item| {
                    let map = item.as_object().unwrap();
                    headers
                        .iter()
                        .map(|k| cell_from_json(map.get(k).unwrap_or(&serde_json::Value::Null)))
                        .collect()
                })
                .collect();
            Some(MiniTable {
                kind: MiniKind::Structured,
                headers,
                rows,
                truncated: truncated || arr.len() > TABLE_ROWS,
            })
        }
        _ => None,
    }
}

fn cell_from_json(v: &serde_json::Value) -> MiniCell {
    match v {
        serde_json::Value::Null => MiniCell::Empty,
        serde_json::Value::String(s) => MiniCell::Text(clip(s)),
        serde_json::Value::Number(n) => MiniCell::Text(n.to_string()),
        serde_json::Value::Bool(b) => MiniCell::Text(b.to_string()),
        nested @ (serde_json::Value::Object(_) | serde_json::Value::Array(_)) => MiniCell::Nested {
            summary: clip(&compact_json(nested)),
            json: nested.to_string(),
        },
    }
}

fn compact_json(v: &serde_json::Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
}

fn clip(s: &str) -> String {
    let mut it = s.chars();
    let taken: String = it.by_ref().take(CELL_CHARS).collect();
    if it.next().is_some() {
        format!("{taken}…")
    } else {
        taken
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(32)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn rec(s: &str) -> RecordPreview {
    RecordPreview {
        len: s.len(),
        bytes: s.as_bytes().to_vec(),
    }
}

pub fn stream(n: u32) -> StreamId {
    StreamId(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_core::DebugEvent;

    fn ev(stage: usize, kind: DebugEventKind) -> DebugEvent {
        DebugEvent {
            stage: StageId(stage),
            name: "t".into(),
            kind,
        }
    }

    fn slot(writer: usize, reader: usize, record: Option<&str>, writer_done: bool) -> SlotSnapshot {
        SlotSnapshot {
            writer: StageId(writer),
            writer_stream: StreamId(0),
            reader: StageId(reader),
            reader_stream: StreamId(0),
            record: record.map(rec),
            peeked: false,
            severed: false,
            writer_done,
        }
    }

    #[test]
    fn plain_apple_is_one_column_not_csv() {
        let t = mini_table(&rec("apple"));
        assert_eq!(t.kind, MiniKind::Text);
        assert!(
            t.headers.is_empty(),
            "no invented field names: {:?}",
            t.headers
        );
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0], vec![MiniCell::Text("apple".into())]);
    }

    #[test]
    fn raw_csv_header_is_not_split_into_columns() {
        let line = "vessel_id,name,type,berth_id,owner_id";
        let t = mini_table(&rec(line));
        assert_eq!(t.kind, MiniKind::Text);
        assert!(t.headers.is_empty());
        assert_eq!(t.rows[0].len(), 1);
        match &t.rows[0][0] {
            MiniCell::Text(s) => assert!(s.contains("vessel_id,name"), "{s}"),
            other => panic!("expected one text cell, got {other:?}"),
        }
    }

    #[test]
    fn json_object_is_field_columns() {
        let t = mini_table(&rec(r#"{"vessel_id":"V1","name":"Kestrel","type":"sail"}"#));
        assert_eq!(t.kind, MiniKind::Structured);
        assert_eq!(t.headers, vec!["vessel_id", "name", "type"]);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(
            t.rows[0],
            vec![
                MiniCell::Text("V1".into()),
                MiniCell::Text("Kestrel".into()),
                MiniCell::Text("sail".into()),
            ]
        );
    }

    #[test]
    fn json_object_array_is_multirow_grid() {
        let t = mini_table(&rec(r#"[{"a":1},{"a":2,"b":"x"}]"#));
        assert_eq!(t.kind, MiniKind::Structured);
        assert!(t.headers.contains(&"a".to_string()));
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn nested_object_cell_is_inspectable_not_flattened() {
        let t = mini_table(&rec(r#"{"id":"V1","owner":{"name":"Ann"}}"#));
        assert_eq!(t.kind, MiniKind::Structured);
        assert!(matches!(t.rows[0][1], MiniCell::Nested { .. }));
    }

    #[test]
    fn edge_recent_is_writer_outgoing_only() {
        let events = vec![
            ev(
                1,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("vessel_id,name,type,berth_id,owner_id"),
                },
            ),
            ev(
                1,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec(r#"{"vessel_id":"V1"}"#),
                },
            ),
            ev(
                2,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec(r#"{"vessel_id":"V1"}"#),
                },
            ),
        ];
        let focus = RecordFocus::Edge {
            writer: StageId(1),
            writer_stream: 0,
            reader: StageId(2),
            reader_stream: 0,
        };
        let recent = recent_items(&events, focus, 8);
        assert_eq!(
            recent.len(),
            1,
            "must not include CSV input or LOOKUP input: {recent:?}"
        );
        assert_eq!(recent[0].kind, ObservedKind::WentOut);
        let bytes = recent[0].record.as_ref().unwrap();
        assert!(std::str::from_utf8(&bytes.bytes).unwrap().contains("V1"));
        let c = counts_for_focus(&events, focus);
        assert_eq!(c.received, 0);
        assert_eq!(c.sent, 1);
    }

    #[test]
    fn csv_pre_output_edge_claims_no_outgoing() {
        let events = vec![ev(
            1,
            DebugEventKind::ReadTaken {
                stream: stream(0),
                record: rec("vessel_id,name"),
            },
        )];
        let slots = vec![slot(1, 2, None, false)];
        let focus = RecordFocus::Edge {
            writer: StageId(1),
            writer_stream: 0,
            reader: StageId(2),
            reader_stream: 0,
        };
        assert!(recent_items(&events, focus, 8).is_empty());
        assert_eq!(counts_for_focus(&events, focus).sent, 0);
        let wait = waiting_items(&slots, focus);
        assert_eq!(wait.len(), 1);
        assert_eq!(wait[0].kind, WaitingKind::Empty);
        assert!(wait[0].record.is_none());
    }

    #[test]
    fn first_literal_edge_waiting_is_apple() {
        let events = vec![ev(
            0,
            DebugEventKind::WriteOffered {
                stream: stream(0),
                record: rec("apple"),
            },
        )];
        let slots = vec![slot(0, 1, Some("apple"), false)];
        let focus = RecordFocus::Edge {
            writer: StageId(0),
            writer_stream: 0,
            reader: StageId(1),
            reader_stream: 0,
        };
        let wait = waiting_items(&slots, focus);
        assert_eq!(wait[0].kind, WaitingKind::Waiting);
        let t = mini_table(wait[0].record.as_ref().unwrap());
        assert_eq!(t.rows[0][0], MiniCell::Text("apple".into()));
        let recent = recent_items(&events, focus, 8);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].kind, ObservedKind::WentOut);
        assert_eq!(counts_for_focus(&events, focus).sent, 1);
        assert_eq!(counts_for_focus(&events, focus).received, 0);
    }

    #[test]
    fn stage_focus_keeps_both_directions() {
        let events = vec![
            ev(
                3,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
            ev(
                3,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
        ];
        let focus = RecordFocus::Stage(StageId(3));
        let recent = recent_items(&events, focus, 8);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].kind, ObservedKind::CameIn);
        assert_eq!(recent[1].kind, ObservedKind::WentOut);
        let c = counts_for_focus(&events, focus);
        assert_eq!(c.received, 1);
        assert_eq!(c.sent, 1);
    }

    #[test]
    fn empty_and_binary_states() {
        let empty = mini_table(&RecordPreview {
            len: 0,
            bytes: vec![],
        });
        assert_eq!(empty.kind, MiniKind::Empty);
        let bin = mini_table(&RecordPreview {
            len: 2,
            bytes: vec![0xff, 0xfe],
        });
        assert_eq!(bin.kind, MiniKind::Binary);
        let trunc = mini_table(&RecordPreview {
            len: 100,
            bytes: b"short".to_vec(),
        });
        assert!(trunc.truncated);
    }
}
