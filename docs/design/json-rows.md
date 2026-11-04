# JSON-per-row records

**Audience:** people writing pipelines that carry named fields

User-facing reference (copy-and-paste examples, quoting, optional `$`): [`../json-and-fields.md`](../json-and-fields.md). Counted beside the IBM catalog in [`../STATUS.md`](../STATUS.md) and [`../builtins-roadmap.md`](../builtins-roadmap.md).

---

## 1. Goal

A record may be one JSON object. Built-in stages that already take an input range can then name a field (`$qty`, `$user.city`, `$books.0.title`) instead of a column. Records that are not JSON stay ordinary bytes. SPEC writes plain text, so JSON is only on the path while a later stage still wants it.

The same object shape is what a later CSV / SQL / Parquet / xlsx reader or writer would produce and consume. This increment ships JSON and CSV only.

## 2. Record model

The record is still bytes. When the bytes are a JSON object (optional surrounding blanks), `$qty` and the `JSON` / `CSV` stages treat them as fields. There is no sidecar metadata on `Record`. Stages that ignore JSON (`COPY`, `TAKE`, `FANOUT`, …) pass the same bytes through. Stages that rebuild the record (`STRIP`, `CHANGE`, `SPEC`) write new bytes and drop the object unless the new bytes are themselves JSON.

`JSON ARRAY` is the exception that reads a top-level JSON array and writes one object record per element. After that, the records are objects again.

`serde_json` (with `preserve_order`) parses and writes the object. That crate was added for this work so numbers, strings, nested objects, and arrays round-trip without a hand-rolled parser. Integers that fit in 64 bits stay integers. Unicode in strings is kept. Non-object records and non-JSON input are unchanged.

## 3. `$field` as an input range

`$qty`, `$a.b` (nested object keys, ASCII identifiers), and array indexes parse as one more kind of `inputRange`. SPEC and PICK are the stages this increment supports in full. JSON ADD / SET expressions and JSON GET use the same paths.

**Primary index spelling** is dot plus a 0-based integer: `$books.0.title`. Brackets are also accepted, with no spaces: `$books[0].title`. Negative indexes count from the end (`$books.-1` and `$books[-1]` are the last element). Do not put spaces in the path.

| Situation | SPEC | PICK |
|---|---|---|
| Field is a JSON string | unquoted text | string compare uses that text |
| Field is a JSON number | decimal spelling (`120`, `1.5`) | numeric compare uses the number (no "not a floating-point number") |
| Field is an object or array | compact JSON | string compare uses that compact JSON |
| Field is `null` or missing | empty | the comparison does not hold (record not selected) |
| Index is out of range, or an index is applied to a non-array | empty (same as missing) | the comparison does not hold |
| Record is not a JSON object | stage error `record is not a JSON object` | same error |

A missing field never matches, including `¬=` / `¬==`. That is a first-increment rule, not IBM. A path segment that looks like an integer is always an index: `$books.0` indexes an array. When the value is not an array, that index is missing (empty in SPEC, PICK comparison does not hold), even if an object has a key named `"0"`.

Other stages that extract a range (`SUBSTRING`, `UNIQUE`, `LOOKUP`, `ZONE`, …) accept `$qty` and extract the same display text. A non-object record extracts as empty there (no panic). Stages that need a byte span in the original record (`INSERT`, `THREEWAY`, XLATE positions) do not get a useful column address for `$qty`. Whether `$field` should do more in LOCATE, SORT, and CHANGE is an open question.

Do not write a closing `$` after the field word. `$qty$` would look like an IBM `$...$` delimited string. This port treats `$` plus a letter or `_` as a field word so `$qty * $amount` is three tokens, not one `$qty * $` string. `$qty-1` is the field `$qty` minus 1; `$books.-1` is the last element of `books`.

## 4. JSON stage

```text
JSON ADD $total = expr
JSON SET $qty = expr
JSON DELETE $field...
JSON KEEP $field...
JSON GET $field
JSON SPLIT $books [KEEP]
JSON COLLECT $books
JSON ARRAY
```

Write `$field` on JSON subcommand operands (ADD/SET target, DELETE, KEEP, GET, SPLIT, COLLECT). The `$` may be left off there. A path may be `$user.city` or `$books.0.title`. SPEC, PICK, and ADD/SET expressions still require `$qty`. ADD fails if the leaf exists. SET creates or replaces. SET / ADD do not grow arrays: a missing array or an out-of-range index is an error. DELETE of a missing leaf is a no-op. DELETE of an index removes that element (later elements shift down). KEEP keeps top-level names only. GET writes the field as plain text (same display rules as SPEC) and errors if the field is missing.

### Expressions and string literals

Expression operands: JSON fields (`$qty`, `$books.0.title`), numbers (`3`, `1.5`), and string literals. Operators: `+` `-` `*` `/` on numbers; `+` concatenates when either side is a string; `|` or `||` also concatenates. In a pipeline specification, write `||` if you want a bar (the stage separator is self-escaping).

String literals do **not** use `/text/` the way SPEC insert text does, except in one case below. The reliable form is a quoted string:

```text
json add $shout = $city + "!"
```

`"..."` and `'...'` work, as do the other IBM pair delimiters `%` `#` `!` `;` `,`. `/` is division when it sits after a value, so `$qty / $price` stays arithmetic. At the start of an expression, or after `+` `-` `*` `(` or concat, `/.../` is a string, so `json add $shout = $city + /!/` works too. If you write `json add $shout = $city + /!/` and the `/` is treated as division, the `!` looks like an unclosed string (`JSON: unclosed string in expression`). Prefer `"!"` when in doubt.

### JSON SPLIT

`JSON SPLIT $books` reads one object record. The named field (a path is allowed) must be a JSON array. Each element becomes one output record:

- An object element is that object (the new record).
- Any other element (`1`, `"a"`, `null`, `true`, a nested array) is wrapped as `{"value": x}` so the record stays an object.
- An empty array writes nothing for that input record.
- A missing field is `JSON: field $books is missing`.
- A non-array field (including `null`) is `JSON: $books is not a JSON array`.

`JSON SPLIT $books KEEP` copies the parent object's other fields into each output record, then overlays the element. Element keys win. The array field itself is dropped from the copy so each child does not carry the whole array. For a scalar element, `value` is the element; if the parent already had `value`, the element wins.

SPLIT writes one record per element, in array order, then reads the next input record. It does not hold the rest of the stream.

### JSON COLLECT

`JSON COLLECT $books` is the small inverse of SPLIT for object elements. It reads every remaining object record, holds them, and writes one record `{"books":[...]}`. Empty input writes `{"books":[]}`. Non-object input is an error. The whole collection is held in memory. There is no count operand and no "undo KEEP": scalar elements that SPLIT wrapped as `{"value": x}` stay wrapped inside the collected array.

COLLECT does not write a top-level JSON array. Use it when a later stage still wants an object record with a named field.

### JSON ARRAY

`JSON ARRAY` reads input that is one JSON array (a single record, or pretty-printed over many records from `<`). It gathers records, joining them with a newline, until the top-level array closes, then writes one object record per element. The same wrap rule as SPLIT applies: object elements become records; anything else is `{"value": x}`. Nested arrays of arrays do not panic and are not flattened (`[[1],[2]]` yields two records each with a `value` array). An empty array writes nothing. Input that is not an array (a JSON object, a primitive, or plain text) is `JSON: input is not a JSON array`. Invalid JSON and an unclosed array at end of file are errors.

The whole array document is held in memory. There is no size cap. Several complete arrays in one stream are each expanded in order. Trailing blanks after the last array are ignored.

Non-JSON input to ADD / SET / DELETE / KEEP / GET / SPLIT / COLLECT is `JSON: record is not a JSON object` (no panic). `JSON ARRAY` is the verb that accepts a top-level array.

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

- `$field` in LOCATE, SORT, and CHANGE (syntax may already parse as a range; behaviour is not specified yet)
- A JSON Lines reader/writer as its own stage (the record already is one object; `JSON ARRAY` is for one array document)
- Writing a top-level JSON array (COLLECT wraps the array in an object field)
- Auto-growing arrays on SET / ADD
- SQL, Parquet, xlsx
- A sidecar on `Record` (REXX/Python still see bytes)
- `$["odd key"]` and non-identifier field names in `$` refs (CSV headers that are not identifiers need a later syntax)

## 7. Tests

Unit tests live in `crates/pipe-stages/src/json_row.rs`, `csv_stage.rs`, `input_range.rs`, and `crates/pipe-parser` (`$qty` and `$books[0].title` are words). Conformance tests are `crates/pipe-stages/tests/json_csv.rs`. Fixture cases are `tests/pipelines/json-csv.pipefix`.
