//! Streaming splitter for consecutive top-level JSON objects.
//!
//! The buffer holds the current incomplete object plus any still-unparsed
//! bytes from the last fed record. Scan position, string state, and the
//! brace/bracket stack are retained so already-seen bytes are not scanned
//! again.

use serde_json::Value;

use crate::json_row::write_json_object;

/// Result of asking the scanner for the next complete object.
#[derive(Debug)]
pub enum ObjectTake {
    /// The buffer does not yet hold a complete object.
    NeedMore,
    /// A complete JSON object, already parsed.
    Object(Value),
}

/// Incremental scanner for concatenated JSON objects.
#[derive(Debug, Default)]
pub struct JsonObjectScan {
    buf: Vec<u8>,
    /// Next byte to inspect.
    pos: usize,
    stack: Vec<u8>,
    in_string: bool,
    escape: bool,
    /// Index of the `{` that opened the current object.
    obj_start: Option<usize>,
}

impl JsonObjectScan {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one pipeline record. A newline is inserted before the record
    /// when the buffer already holds bytes, so a pretty-printed object that
    /// `<` split into lines is joined back together.
    pub fn feed_record(&mut self, rec: &[u8]) {
        if !self.buf.is_empty() {
            self.buf.push(b'\n');
        }
        self.append_bytes(rec);
    }

    /// Append bytes without inserting a record separator.
    fn append_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Drop unread bytes without parsing them. Used when the output has
    /// closed and later objects must not be diagnosed.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.pos = 0;
        self.stack.clear();
        self.in_string = false;
        self.escape = false;
        self.obj_start = None;
    }

    /// Take the next complete object, or ask for more input.
    pub fn take_next(&mut self) -> Result<ObjectTake, String> {
        loop {
            if self.obj_start.is_none() {
                self.skip_json_ws();
                self.skip_utf8_bom();
                self.skip_json_ws();
                if self.pos >= self.buf.len() {
                    self.compact_consumed();
                    return Ok(ObjectTake::NeedMore);
                }
                match self.buf[self.pos] {
                    b'{' => {
                        self.obj_start = Some(self.pos);
                        self.stack.push(b'{');
                        self.pos += 1;
                    }
                    b'[' => return Err(array_message().into()),
                    _ => return Err(non_object_message(&self.buf[self.pos..])),
                }
            }

            while self.pos < self.buf.len() {
                let b = self.buf[self.pos];
                if self.in_string {
                    if self.escape {
                        self.escape = false;
                    } else if b == b'\\' {
                        self.escape = true;
                    } else if b == b'"' {
                        self.in_string = false;
                    }
                    self.pos += 1;
                    continue;
                }
                match b {
                    b'"' => self.in_string = true,
                    b'[' | b'{' => self.stack.push(b),
                    b']' => {
                        if self.stack.last() != Some(&b'[') {
                            return Err(invalid_json_message(&self.buf[self.obj_slice_start()..]));
                        }
                        self.stack.pop();
                    }
                    b'}' => {
                        if self.stack.last() != Some(&b'{') {
                            return Err(invalid_json_message(&self.buf[self.obj_slice_start()..]));
                        }
                        self.stack.pop();
                    }
                    _ => {}
                }
                self.pos += 1;
                if self.stack.is_empty() && !self.in_string {
                    return self.finish_object();
                }
            }

            return Ok(ObjectTake::NeedMore);
        }
    }

    /// End of file: error if an object is still open; ignore trailing blanks.
    pub fn finish_eof(&self) -> Result<(), String> {
        if self.in_string || self.escape || self.obj_start.is_some() || !self.stack.is_empty() {
            return Err("unclosed JSON object".into());
        }
        let mut i = self.pos;
        while i < self.buf.len() && is_json_ws(self.buf[i]) {
            i += 1;
        }
        if i < self.buf.len() {
            return Err(non_object_message(&self.buf[i..]));
        }
        Ok(())
    }

    fn finish_object(&mut self) -> Result<ObjectTake, String> {
        let start = self.obj_start.expect("object start");
        let end = self.pos;
        let slice = &self.buf[start..end];
        let text =
            std::str::from_utf8(slice).map_err(|_| "input is not valid UTF-8".to_string())?;
        let value: Value = serde_json::from_str(text).map_err(|_| invalid_json_message(slice))?;
        if !value.is_object() {
            return Err(non_object_message(slice));
        }
        self.drain_to(end);
        Ok(ObjectTake::Object(value))
    }

    fn obj_slice_start(&self) -> usize {
        self.obj_start.unwrap_or(self.pos)
    }

    fn skip_json_ws(&mut self) {
        while self.pos < self.buf.len() && is_json_ws(self.buf[self.pos]) {
            self.pos += 1;
        }
        if self.obj_start.is_none() {
            self.compact_consumed();
        }
    }

    fn skip_utf8_bom(&mut self) {
        const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
        if self.obj_start.is_none() && self.buf[self.pos..].starts_with(BOM) {
            self.pos += 3;
        }
    }

    fn compact_consumed(&mut self) {
        if self.pos == 0 || self.obj_start.is_some() {
            return;
        }
        let n = self.buf.len();
        if self.pos >= n {
            self.buf.clear();
        } else {
            self.buf.copy_within(self.pos.., 0);
            self.buf.truncate(n - self.pos);
        }
        self.pos = 0;
    }

    fn drain_to(&mut self, end: usize) {
        let n = self.buf.len();
        if end >= n {
            self.buf.clear();
        } else {
            self.buf.copy_within(end.., 0);
            self.buf.truncate(n - end);
        }
        self.pos = 0;
        self.stack.clear();
        self.in_string = false;
        self.escape = false;
        self.obj_start = None;
    }
}

/// Compact JSON bytes for an object record.
pub fn emit_object_bytes(value: &Value) -> Result<Vec<u8>, String> {
    Ok(write_json_object(value)?.into_bytes())
}

fn is_json_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn array_message() -> &'static str {
    "input is not a JSON object. json objects reads consecutive JSON objects, not a top-level array. Use json array for an array document."
}

fn non_object_message(bytes: &[u8]) -> String {
    if looks_like_mongo_shell(bytes) {
        return mongo_message().into();
    }
    if bytes.first() == Some(&b'[') {
        return array_message().into();
    }
    "input is not a JSON object. json objects reads only JSON objects.".into()
}

fn invalid_json_message(bytes: &[u8]) -> String {
    if looks_like_mongo_shell(bytes) {
        mongo_message().into()
    } else {
        "invalid JSON".into()
    }
}

fn mongo_message() -> &'static str {
    "this is not standard JSON. MongoDB shell constructors such as ObjectId(...), ISODate(...), NumberLong(...), and NumberInt(...) are not JSON. json objects does not convert them. Export the data as JSON, or as Extended JSON (for example {\"$oid\":\"...\"}), and run json objects on that."
}

fn looks_like_mongo_shell(bytes: &[u8]) -> bool {
    const MARKERS: [&[u8]; 4] = [b"ObjectId(", b"ISODate(", b"NumberLong(", b"NumberInt("];
    MARKERS
        .iter()
        .any(|m| bytes.windows(m.len()).any(|w| w == *m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take_all(chunks: &[&[u8]]) -> Result<Vec<Value>, String> {
        let mut scan = JsonObjectScan::new();
        let mut out = Vec::new();
        for chunk in chunks {
            scan.feed_record(chunk);
            loop {
                match scan.take_next()? {
                    ObjectTake::NeedMore => break,
                    ObjectTake::Object(v) => out.push(v),
                }
            }
        }
        scan.finish_eof()?;
        Ok(out)
    }

    #[test]
    fn two_objects_in_one_chunk() {
        let v = take_all(&[br#"{"a":1}{"b":2}"#]).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["a"], 1);
        assert_eq!(v[1]["b"], 2);
    }

    #[test]
    fn pretty_printed_across_records() {
        let v = take_all(&[
            b"{",
            br#"  "title": "A""#,
            b"}",
            b"{",
            br#"  "title": "B""#,
            b"}",
        ])
        .unwrap();
        assert_eq!(v[0]["title"], "A");
        assert_eq!(v[1]["title"], "B");
    }

    #[test]
    fn braces_and_escaped_quotes_in_strings() {
        let rec = br#"{"s":"say \"hi\" and {braces} and \\ slash"}{"n":1}"#;
        let v = take_all(&[rec]).unwrap();
        assert_eq!(v[0]["s"], "say \"hi\" and {braces} and \\ slash");
        assert_eq!(v[1]["n"], 1);
    }

    #[test]
    fn nested_object_and_array() {
        let rec = br#"{"a":{"b":[1,{"c":"x"}]},"d":true}"#;
        let v = take_all(&[rec]).unwrap();
        assert_eq!(v[0]["a"]["b"][1]["c"], "x");
        assert_eq!(v[0]["d"], true);
    }

    #[test]
    fn empty_and_whitespace_is_ok() {
        assert!(take_all(&[b""]).unwrap().is_empty());
        assert!(take_all(&[b"  \n\r\t  "]).unwrap().is_empty());
        assert!(take_all(&[b"  ", b"\n", b"\r\n"]).unwrap().is_empty());
    }

    #[test]
    fn crlf_inside_records() {
        let v =
            take_all(&[b"{\r\n  \"title\": \"A\"\r\n}\r\n{\r\n  \"title\": \"B\"\r\n}"]).unwrap();
        assert_eq!(v[0]["title"], "A");
        assert_eq!(v[1]["title"], "B");
    }

    #[test]
    fn unclosed_at_eof() {
        let err = take_all(&[b"{\"a\":1"]).unwrap_err();
        assert!(err.contains("unclosed"), "{err}");
    }

    #[test]
    fn malformed_object() {
        let err = take_all(&[b"{\"a\":1,}"]).unwrap_err();
        assert!(err.contains("invalid JSON"), "{err}");
    }

    #[test]
    fn leftover_text() {
        let err = take_all(&[b"{\"a\":1} hello"]).unwrap_err();
        assert!(err.contains("not a JSON object"), "{err}");
    }

    #[test]
    fn top_level_array_is_rejected() {
        let err = take_all(&[b"[1,2]"]).unwrap_err();
        assert!(err.contains("json array"), "{err}");
        assert!(err.contains("not a JSON object"), "{err}");
    }

    #[test]
    fn mongo_shell_is_not_converted() {
        let rec = br#"{ "_id" : ObjectId("507f1f77bcf86cd799439011"), "n": 1 }"#;
        let err = take_all(&[rec]).unwrap_err();
        assert!(err.contains("ObjectId"), "{err}");
        assert!(err.contains("not standard JSON"), "{err}");
        assert!(err.contains("does not convert"), "{err}");
        assert!(err.contains("Extended JSON"), "{err}");
    }

    #[test]
    fn extended_json_oid_is_ordinary_json() {
        let rec = br#"{"_id":{"$oid":"507f1f77bcf86cd799439011"},"n":1}"#;
        let v = take_all(&[rec]).unwrap();
        assert_eq!(v[0]["_id"]["$oid"], "507f1f77bcf86cd799439011");
        assert_eq!(v[0]["n"], 1);
    }

    #[test]
    fn scalar_is_rejected() {
        let err = take_all(&[b"true"]).unwrap_err();
        assert!(err.contains("only JSON objects"), "{err}");
    }

    #[test]
    fn clear_drops_remainder_without_error() {
        let mut scan = JsonObjectScan::new();
        scan.feed_record(br#"{"a":1}{"bad":}"#);
        match scan.take_next().unwrap() {
            ObjectTake::Object(v) => assert_eq!(v["a"], 1),
            ObjectTake::NeedMore => panic!("expected first object"),
        }
        scan.clear();
        scan.finish_eof().unwrap();
    }

    #[test]
    fn byte_at_a_time_is_linear_and_complete() {
        let rec = br#"{"title":"A","note":"say \"hi\" {x}","xs":[1,2,{"k":true}]}"#;
        let mut scan = JsonObjectScan::new();
        for &b in rec.iter() {
            scan.append_bytes(&[b]);
        }
        match scan.take_next().unwrap() {
            ObjectTake::Object(v) => {
                assert_eq!(v["title"], "A");
                assert_eq!(v["xs"][2]["k"], true);
            }
            ObjectTake::NeedMore => panic!("complete object"),
        }
        scan.finish_eof().unwrap();
    }

    #[test]
    fn emit_is_compact() {
        let v = serde_json::json!({"title":"A","n":1});
        let bytes = emit_object_bytes(&v).unwrap();
        assert_eq!(bytes, br#"{"title":"A","n":1}"#);
    }
}
