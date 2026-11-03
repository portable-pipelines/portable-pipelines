# JSON-per-row records (experimental portable extension)

**Status:** first increment, experimental. Syntax may change.  
**Audience:** people writing pipelines that carry named fields  
**Authority:** this is **not** an IBM built-in. IBM `inputRange` in SPEC and PICK is the pattern `$name` extends. Stage verbs `JSON` and `CSV` are portable names. They are not in Author's Edition Appendix A. See [`../builtins-roadmap.md`](../builtins-roadmap.md) (Non-IBM extensions) and [`../STATUS.md`](../STATUS.md).

---

## 1. Goal

A record may be one JSON object. Built-in stages that already take an input range can then name a field (`$qty`, `$user.name`) instead of a column. Records that are not JSON stay ordinary bytes. SPEC writes plain text, so JSON is only on the path while a later stage still wants it.

The same object shape is what a later CSV / SQL / Parquet / xlsx reader or writer would produce and consume. This increment ships JSON and CSV only.

## 2. Record model

The record is still bytes. When the bytes are a JSON object (optional surrounding blanks), `$name` and the `JSON` / `CSV` stages treat them as fields. There is no sidecar metadata on `Record`. Stages that ignore JSON (`COPY`, `TAKE`, `FANOUT`, …) pass the same bytes through. Stages that rebuild the record (`STRIP`, `CHANGE`, `SPEC`) write new bytes and drop the object unless the new bytes are themselves JSON.

`serde_json` (with `preserve_order`) parses and writes the object. That crate was added for this work so numbers, strings, and nested objects round-trip without a hand-rolled parser.

## 3. `$name` as an input range

`$name` and `$a.b` (nested object keys, ASCII identifiers) parse as one more kind of `inputRange`. SPEC and PICK are the stages this increment supports in full.

| Situation | SPEC | PICK |
|---|---|---|
| Field is a JSON string | unquoted text | string compare uses that text |
| Field is a JSON number | decimal spelling (`120`, `1.5`) | numeric compare uses the number (no "not a floating-point number") |
| Field is an object or array | compact JSON | string compare uses that compact JSON |
| Field is `null` or missing | empty | the comparison does not hold (record not selected) |
| Record is not a JSON object | stage error `record is not a JSON object` | same error |

A missing field never matches, including `¬=` / `¬==`. That is a first-increment rule, not IBM.

Other stages that extract a range (`SUBSTRING`, `UNIQUE`, `LOOKUP`, `ZONE`, …) accept `$name` and extract the same display text. A non-object record extracts as empty there (no panic). Stages that need a byte span in the original record (`INSERT`, `THREEWAY`, XLATE positions) do not get a useful column address for `$name`. Whether `$name` should do more in LOCATE, SORT, and CHANGE is an open question.

`$name` must not be written as `$name$`. A closing `$` would look like an IBM `$...$` delimited string. This port treats `$` plus a letter or `_` as a field word so `$qty * $price` is three tokens, not one `$qty * $` string.

## 4. JSON stage

```text
JSON ADD name = expr
JSON SET name = expr
JSON DELETE names...
JSON KEEP names...
JSON GET name
```

`name` may be `total` or `$total`, and may be a path `user.city`. ADD fails if the leaf exists. SET creates or replaces. DELETE of a missing leaf is a no-op. KEEP keeps top-level names only. GET writes the field as plain text (same display rules as SPEC) and errors if the field is missing.

Expression operands: JSON fields (`$qty`), numbers (`3`, `1.5`), and quoted strings (`"hi "`). Operators: `+` `-` `*` `/` on numbers; `+` concatenates when either side is a string; `|` or `||` also concatenates. In a pipeline specification, write `||` if you want a bar (the stage separator is self-escaping). `/` is division, not a string delimiter.

Non-JSON input is `JSON: record is not a JSON object` (no panic).

## 5. CSV stage

```text
CSV
CSV STRINGS
CSV WRITE
```

The reader takes CSV text records. The first row is the header. Later rows become one JSON object each. RFC 4180 quoting: a field may be wrapped in `"`, a quote inside a quoted field is `""`, and a quoted comma stays one field. If a quoted field is still open at the end of an input record, the reader reads more records and joins them with a newline. That is how a quoted newline survives `<` (one record per line). An unclosed quote at end of file is an error.

**Number rule.** After unquoting, a field is a JSON number when the whole field is an optional `+` or `-`, then either `0` or `[1-9][0-9]*`, then an optional `.` with one or more digits, then an optional `e`/`E` exponent. `007`, empty, and fields with blanks stay strings. Integers that fit in 64 bits stay JSON integers. Other matching fields become JSON floats. `CSV STRINGS` stores every field as a string.

`CSV WRITE` reads JSON objects. The first object's keys (insertion order) are the header. Later objects use those keys; a missing key is empty; extra keys are ignored. Fields that contain a comma, quote, CR, LF, or a leading or trailing blank are quoted.

## 6. What this increment leaves out

- `$name` in LOCATE, SORT, and CHANGE (syntax may already parse as a range; behaviour is not specified yet)
- JSON arrays as the record (must be one object)
- JSON Lines reader/writer as its own stage (the record already is one object)
- SQL, Parquet, xlsx
- A sidecar on `Record` (REXX/Python still see bytes)
- `$["odd key"]` and non-identifier field names in `$` refs (CSV headers that are not identifiers need a later syntax)

## 7. Tests

Unit tests live in `crates/pipe-stages/src/json_row.rs`, `csv_stage.rs`, `input_range.rs`, and `crates/pipe-parser` ( `$qty` is a word). Conformance tests are `crates/pipe-stages/tests/json_csv.rs`. Fixture cases are `tests/pipelines/json-csv.pipefix`.
