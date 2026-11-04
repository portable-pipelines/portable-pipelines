# JSON rows and CSV

> **Experimental portable extension.** JSON and CSV are not part of IBM CMS
> Pipelines. They are a Portable Pipelines experiment: a record may be one
> JSON object, so you can name a field instead of a column. The syntax may
> change.

If you already write CMS Pipelines, the rest of the picture is the same.
Records still flow through stages connected by `|`. Stages still do not call
each other. SPEC, PICK, and friends still take an input range. `$name` is one
more kind of range: a JSON field instead of a column.

This page is the user-facing reference. Design notes for how the stages are
built live in [`design/json-rows.md`](design/json-rows.md). Sample files:
[`examples/orders.csv`](../examples/orders.csv),
[`examples/order.json`](../examples/order.json),
[`examples/catalog.json`](../examples/catalog.json).

Examples below were run from the repository root with a release `pipe`
binary. They use bash single quotes. See
[Quoting for bash, PowerShell, and cmd](#quoting-for-bash-powershell-and-cmd)
for PowerShell and cmd.

---

## The row shape

The record is still bytes. When those bytes are a JSON **object** (optional
surrounding blanks), `$name` and the JSON / CSV stages treat them as named
fields.

```text
{"name":"Ann","amount":120,"region":"East"}
```

That is one record. `COPY`, `TAKE`, and `FANOUT` pass the same bytes through.
`SPEC` writes plain text, so the JSON object is gone unless the new bytes are
themselves JSON.

**Numbers stay numbers. Strings stay strings.** After `csv`, the sample
`orders.csv` file becomes:

```text
./pipe '< examples/orders.csv | csv | console'
```

```text
{"name":"Ann","amount":120,"region":"East"}
{"name":"Ben","amount":80,"region":"West"}
{"name":"Cam","amount":200,"region":"East"}
```

`120` is a JSON number. `"Ann"` is a JSON string. With `csv strings`, the
amounts are strings too (`"120"`). That difference matters for `+` (see
[CSV](#csv)).

**The record must be an object** for `$name`, JSON ADD / SET / DELETE / KEEP /
GET / SPLIT / COLLECT, and CSV WRITE. A JSON array, a JSON primitive, plain
text, or invalid JSON is:

```text
./pipe 'literal hello | json get name | hole'
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get name"
```

SPEC and PICK say the same thing with their own stage name:

```text
./pipe 'literal hello | spec $name 1 | hole'
```

```text
PIPE: SPEC: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "spec $name 1"
```

A top-level JSON array is not an object record. Use `json array` to turn it
into objects first. `json get` on an array fails the same way:

```text
./pipe 'literal [1,2,3] | json get value | hole'
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get value"
```

Invalid JSON is that same object error, not a parser dump:

```text
./pipe 'literal {nope} | json get a | hole'
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get a"
```

When SPEC or JSON GET writes a field:

| JSON value | What you see |
|---|---|
| string | the text, unquoted |
| number | decimal spelling (`120`, `1.5`) |
| `true` / `false` | `true` / `false` |
| `null` | empty |
| object or array | compact JSON |

---

## What `$name` means

`$bleh` is a placeholder for the field named `bleh`. It is not the literal
characters dollar, b, l, e, h. Write `$qty` for the field `qty`, `$name` for
the field `name`, `$items.0.sku` for a nested path.

```text
./pipe '< examples/order.json | spec $items.0.sku 1 | console'
```

```text
NUT
```

Do not write `$name$`. A closing `$` would look like an IBM `$...$` delimited
string. `$` plus a letter or `_` starts a field word, so `$qty * $price` is
three tokens.

### Nested objects and array indexes

| Path | Meaning |
|---|---|
| `$user.name` | key `name` inside object `user` |
| `$items.0.sku` | `sku` of the first element of array `items` (0-based) |
| `$items[0].sku` | the same path; brackets are accepted, with no spaces |
| `$items.-1.sku` / `$items[-1].sku` | `sku` of the last element |

```text
./pipe '< examples/order.json | spec $items[1].sku 1 | console'
```

```text
BOLT
```

```text
./pipe '< examples/order.json | spec $items.-1.sku 1 | console'
```

```text
BOLT
```

A whole object or array prints as compact JSON:

```text
./pipe '< examples/order.json | spec $items 1 | console'
```

```text
[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]
```

A segment that looks like an integer is always an index. `$items.0` indexes
the array `items`. If you apply an index to something that is not an array,
the path is missing (same as an out-of-range index). An object key named `"0"`
is not selected by `$obj.0`.

### Missing fields

| Situation | SPEC | PICK | JSON ADD / SET expression | JSON GET |
|---|---|---|---|---|
| Field is missing, out of range, or an index on a non-array | empty (the record is still written) | the comparison does not hold, including `^=` / `^==` / `¬=` / `¬==`; the record is not selected | error `JSON: field $price is missing` | error `JSON: field $missing is missing` |
| Field is JSON `null` | empty | the comparison does not hold | (the null value is used) | empty |
| Record is not a JSON object | `SPEC: record is not a JSON object` | `PICK: record is not a JSON object` | `JSON: record is not a JSON object` | same |

```text
./pipe 'literal {"name":"Ann"} | spec $name 1 /:/ next $missing next | console'
```

```text
Ann:
```

```text
./pipe 'literal {"region":"East"} | pick $amount > /0/ | console'
```

(no records)

```text
./pipe 'literal {"region":"East"} | pick $amount ^= /0/ | console'
```

(no records: a missing field never matches, even not-equal)

```text
./pipe 'literal {"qty":3} | json add total = $qty * $price | hole'
```

```text
PIPE: JSON: field $price is missing
... Issued from stage 2 of pipeline 1
... Running "json add total = $qty * $price"
```

In a pipeline specification, `/` starts a delimited string, so the PICK
synonym `/=` is a poor choice next to `/0/`. Use `^=` or `¬=`.

---

## Optional `$` on JSON subcommands

JSON **field-name operands** accept a name with or without a leading `$`.
John chose to keep this forgiving. `json split items keep` and
`json split $items keep` both work. So do `json get $items` and
`json delete $order`.

**Inside SPEC, PICK, and JSON ADD / SET expressions, the `$` is required.**
There `qty` is not a field. `$qty` is.

| Place | `$` | Verified |
|---|---|---|
| `json add name = …` / `json add $name = …` (the name being written) | optional | both write `total` |
| `json set n = …` / `json set $n = …` | optional | both produce `5` |
| `json delete b` / `json delete $b`; `json delete order` / `json delete $order` | optional | both drop the named field |
| `json keep a c` / `json keep $a $c` | optional | both keep `a` and `c` |
| `json get a` / `json get $a`; `json get items.0.sku` / `json get $items.0.sku` | optional | both write `1` or `NUT` |
| `json split items` / `json split $items` (and the same with `keep`) | optional | both write `NUT` then `BOLT` |
| `json collect items` / `json collect $items` | optional | both collect into `items` |
| `json array` | no field name | |
| `$qty` in an ADD / SET **expression** | required | `json add shout = bleh + "!"` is `JSON: unexpected b in expression` |
| `$name` in SPEC | required | `spec name 1` is `SPEC: unexpected operand name` |
| `$region` in PICK | required | `pick region == /East/` is `PICK: unexpected operand region` |

KEEP and COLLECT take a **top-level** name only (`a` or `$a`, not `$a.b`).

The GET / SPLIT missing-field messages always print a `$` in the path
(`JSON: field $missing is missing`), even if you wrote the name without one.

```text
./pipe '< examples/order.json | json split items | spec $sku 1 | console'
./pipe '< examples/order.json | json split $items | spec $sku 1 | console'
```

Both write:

```text
NUT
BOLT
```

```text
./pipe '< examples/orders.csv | csv | spec name 1 | console'
```

```text
PIPE: SPEC: unexpected operand name
... Issued from stage 3 of pipeline 1
... Running "spec name 1"
```

---

## JSON subcommands

There is one stage verb, `JSON`. The first operand chooses the subcommand.
There are eight: ADD, SET, DELETE, KEEP, GET, SPLIT, COLLECT, and ARRAY.

| Subcommand | Syntax | What it does |
|---|---|---|
| ADD | `json add name = expr` | Evaluate `expr` and insert `name`. Error if that leaf already exists. |
| SET | `json set name = expr` | Evaluate `expr` and create or replace `name`. |
| DELETE | `json delete names...` | Remove each named leaf. A missing leaf is a no-op. An array index is removed and later elements shift down. |
| KEEP | `json keep names...` | Keep only those top-level fields. |
| GET | `json get name` | Write the field as plain text (same display rules as SPEC). Error if missing. |
| SPLIT | `json split name [keep]` | The named field must be a JSON array. Write one object record per element. `KEEP` copies the parent's other fields onto each child. |
| COLLECT | `json collect name` | Read remaining object records and write one record `{"name":[...]}`. Held in memory. |
| ARRAY | `json array` | Read a top-level JSON array (one record, or pretty-printed over many records from `<`) and write one object record per element. Held in memory. |

ADD / SET do not grow arrays. A missing array or an out-of-range index is an
error (`JSON: index 9 is out of range`). Intermediate objects for missing
keys are created when the next step is a key (`json add user.city = "Oslo"`).

Non-object input to every subcommand except ARRAY is
`JSON: record is not a JSON object`.

### ADD

```text
./pipe 'literal {"customer":"Ann","qty":3,"price":40} | json add total = $qty * $price | console'
```

```text
{"customer":"Ann","qty":3,"price":40,"total":120}
```

```text
./pipe 'literal {"n":1} | json add n = 2 | hole'
```

```text
PIPE: JSON: field n already exists; use SET to replace it
... Issued from stage 2 of pipeline 1
... Running "json add n = 2"
```

### SET

```text
./pipe 'literal {"n":1} | json set n = $n + 4 | json get n | console'
```

```text
5
```

SET will also create a field that was not there:

```text
./pipe 'literal {"n":1} | json set extra = "x" | console'
```

```text
{"n":1,"extra":"x"}
```

### DELETE

```text
./pipe 'literal {"a":1,"b":2,"c":3} | json delete b | console'
```

```text
{"a":1,"c":3}
```

On the sample order, `json delete $order` (or `json delete order`) drops the
order id and leaves the items:

```text
./pipe '< examples/order.json | json delete $order | console'
```

```text
{"items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]}
```

Deleting an index shifts the array:

```text
./pipe 'literal {"tags":["a","b","c"]} | json delete tags.1 | console'
```

```text
{"tags":["a","c"]}
```

### KEEP

```text
./pipe 'literal {"a":1,"b":2,"c":3}| json keep a c | json get a | console'
```

```text
1
```

The object after `json keep a c` is `{"a":1,"c":3}`. Nested paths are
rejected: `json keep a.b` is `JSON: KEEP takes top-level field names (not $a.b)`.

### GET

```text
./pipe '< examples/order.json | json get $order | console'
```

```text
A-1
```

```text
./pipe 'literal {"a":1} | json get missing | hole'
```

```text
PIPE: JSON: field $missing is missing
... Issued from stage 2 of pipeline 1
... Running "json get missing"
```

### SPLIT

Without `KEEP`, each array element becomes the new record. Object elements
stay objects. Anything else (`1`, `"a"`, `null`, `true`, a nested array) is
wrapped as `{"value": x}` so the record stays an object.

```text
./pipe '< examples/order.json | json split items | spec $sku 1 | console'
```

```text
NUT
BOLT
```

The records themselves:

```text
./pipe '< examples/order.json | json split items | console'
```

```text
{"sku":"NUT","qty":2}
{"sku":"BOLT","qty":10}
```

`KEEP` copies the parent object's other fields into each child, then overlays
the element (element keys win). The array field itself is dropped, so the
child does not carry the whole array.

```text
./pipe '< examples/order.json | json split items keep | console'
```

```text
{"order":"A-1","sku":"NUT","qty":2}
{"order":"A-1","sku":"BOLT","qty":10}
```

```text
./pipe '< examples/order.json | json split items keep | spec $order 1 / / next $sku next | console'
```

```text
A-1 NUT
A-1 BOLT
```

An empty array writes nothing for that input record. A missing field is
`JSON: field $items is missing`. A non-array field (including `null`) is
`JSON: $items is not a JSON array`. SPLIT does not hold the rest of the
stream; it writes the elements of one record, then reads the next.

### COLLECT

The small inverse of SPLIT for object elements. It reads every remaining
object record, holds them, and writes one record. Empty input writes
`{"items":[]}`. There is no count operand and no undo of KEEP: scalar elements
that SPLIT wrapped as `{"value": x}` stay wrapped inside the collected array.
COLLECT does not write a top-level JSON array.

```text
./pipe '< examples/order.json | json split items | json collect items | console'
```

```text
{"items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]}
```

### ARRAY

`JSON ARRAY` is the verb that accepts a top-level array. It gathers records,
joining them with a newline, until the top-level array closes, then writes
one object record per element (same wrap rule as SPLIT). An empty array
writes nothing. A JSON object, a primitive, or plain text is
`JSON: input is not a JSON array`. Invalid JSON and an unclosed array at end
of file are errors. Several complete arrays in one stream are each expanded
in order. Trailing blanks after the last array are ignored. Nested arrays of
arrays are not flattened (`[[1],[2]]` yields two records each with a `value`
array).

The pretty-printed sample catalog is several records from `<`. ARRAY joins
them and expands the objects:

```text
./pipe '< examples/catalog.json | json array | spec $name 1 / / next $city next | console'
```

```text
Ann Oslo
Cam Leeds
```

```text
./pipe 'literal [1,2,3] | json array | spec $value 1 | console'
```

```text
1
2
3
```

```text
./pipe 'literal {"a":1} | json array | hole'
```

```text
PIPE: JSON: input is not a JSON array
... Issued from stage 2 of pipeline 1
... Running "json array"
```

---

## Expressions, strings, `/`, and SPEC separators

JSON ADD / SET expressions take JSON fields (`$qty`, `$items.0.sku`), numbers
(`3`, `1.5`), and string literals. Operators: `+` `-` `*` `/` on numbers.
`+` concatenates when either side is a string. `|` or `||` also concatenates.
In a pipeline specification you must write `||` for a bar, because a single
`|` is the stage separator.

The reliable string form is a quoted literal. `"..."` and `'...'` work, as do
the other IBM pair delimiters `%` `#` `!` `;` `,`.

```text
./pipe 'strliteral /{"bleh":"hi"}/ | json add shout = $bleh + "!" | json get shout | console'
```

```text
hi!
```

`/` is division when it sits after a value, so `$qty / $price` stays
arithmetic:

```text
./pipe 'literal {"qty":8,"price":2} | json add n = $qty / $price | json get n | console'
```

```text
4
```

At the start of an expression, or after `+` `-` `*` `(` or concat, `/.../` is
a string, so this also works:

```text
./pipe 'strliteral /{"bleh":"hi"}/ | json add shout = $bleh + /!/ | json get shout | console'
```

```text
hi!
```

Prefer `"!"` when in doubt. If `/` is treated as division, the `!` looks like
an unclosed string (`JSON: unclosed string in expression`).

`$qty-1` in an expression is the field `$qty` minus 1 (`$items.-1` is still
the last element of `items`).

### SPEC `next` and `nextword`

`nextword` inserts a blank between what is already in the output record and
this piece. `next` does not; it just appends.

```text
./pipe 'literal {"name":"Ann","amount":120} | spec $name 1 $amount next | console'
```

```text
Ann120
```

```text
./pipe 'literal {"name":"Ann","amount":120} | spec $name 1 $amount nextword | console'
```

```text
Ann 120
```

The examples that print `Ann spent 120` put the spaces inside the insert
text `/ spent /` and then use `next`:

```text
./pipe 'literal {"name":"Ann","amount":120} | spec $name 1 / spent / next $amount next | console'
```

```text
Ann spent 120
```

`nextword` after `/spent/` would add a blank before `spent`, then `next`
would glue the amount on with no blank (`Ann spent120`).

---

## CSV

```text
csv
csv strings
csv write
```

The reader takes CSV text records. The **first row is the header**. Later
rows become one JSON object each.

RFC 4180 quoting: a field may be wrapped in `"`, a quote inside a quoted
field is `""`, and a quoted comma stays one field. If a quoted field is still
open at the end of an input record, the reader reads more records and joins
them with a newline. That is how a quoted newline survives `<` (one record
per line). An unclosed quote at end of file is an error. A BOM on the first
header name is stripped. Duplicate header names are an error. A data row with
more fields than the header is an error. A short row fills missing cells with
empty.

**Number rule.** After unquoting, a field is a JSON number when the whole
field is an optional `+` or `-`, then either `0` or `[1-9][0-9]*`, then an
optional `.` with one or more digits, then an optional `e`/`E` exponent.
`007`, empty, and fields with blanks stay strings. Integers that fit in 64
bits stay JSON integers. Other matching fields become JSON floats.

`csv strings` stores every field as a string:

```text
./pipe '< examples/orders.csv | csv strings | console'
```

```text
{"name":"Ann","amount":"120","region":"East"}
{"name":"Ben","amount":"80","region":"West"}
{"name":"Cam","amount":"200","region":"East"}
```

That changes `+`. On numbers, `+` adds. On strings, `+` concatenates:

```text
./pipe '< examples/orders.csv | csv | json add doubled = $amount + $amount | spec $name 1 / / next $doubled next | console'
```

```text
Ann 240
Ben 160
Cam 400
```

```text
./pipe '< examples/orders.csv | csv strings | json add doubled = $amount + $amount | json get doubled | console'
```

```text
120120
8080
200200
```

`*` `/` `-` still try to parse a numeric-looking string, so
`csv strings | json add doubled = $n * 2` on a cell `3` writes `6`. Use
`csv strings` when you want to keep leading zeros (`007` stays `"007"` even
without `STRINGS`, because of the number rule). `STRINGS` is for the reader
only: `csv strings write` is `CSV: STRINGS applies to the reader, not WRITE`.

Quoted commas and doubled quotes:

```text
./pipe 'strliteral /Ann,"x,y","say ""hi"""/| strliteral /name,note,msg/ | csv | spec $note 1 / :: / next $msg next | console'
```

```text
x,y :: say "hi"
```

### CSV WRITE

`csv write` reads JSON objects. The first object's keys (insertion order)
are the header. Later objects use those keys; a missing key is empty; extra
keys on later objects are ignored. Fields that contain a comma, quote, CR,
LF, or a leading or trailing blank are quoted.

```text
./pipe 'literal Ann,3,40| literal customer,qty,price| csv | csv write | console'
```

```text
customer,qty,price
Ann,3,40
```

(Chained `literal` writes in reverse order, which is why the header line is
listed second in the specification and comes out first.)

```text
./pipe 'literal Ann,"x,y",3| literal name,note,qty| csv | csv write | console'
```

```text
name,note,qty
Ann,"x,y",3
```

Plain text is `CSV: record is not a JSON object`.

---

## Quoting for bash, PowerShell, and cmd

The specification you pass to `pipe` is one argument. `$qty` and `|` inside
it must reach `pipe` unchanged.

**bash / zsh.** Single quotes. `$` and `|` are ordinary inside them. That is
the form used on this page.

```text
./pipe '< examples/order.json | spec $items.0.sku 1 | console'
```

**PowerShell** (Windows PowerShell 5.1 and later). Single quotes, the same
reason: `$qty` is not expanded. The guided tour does this with
`-Spec '… | json add total = $qty * $price | …'` in
[`examples/run-examples.ps1`](../examples/run-examples.ps1). From the folder
that contains `pipe.exe`:

```text
.\pipe.exe '< examples/order.json | spec $items.0.sku 1 | console'
```

**cmd.** Double quotes around the whole specification. Existing notes
([`examples/json_rows.txt`](../examples/json_rows.txt),
[`examples/README.md`](../examples/README.md)) treat `$`, `|`, and `>` as
ordinary inside those double quotes. JSON `{` and `"` inside a `literal` are
awkward in Command Prompt, which is why the samples use checked-in files
(`< examples/order.json`) instead of a JSON literal on the command line.

This page was verified on Linux. cmd and PowerShell were not run here; the
advice above is from those existing scripts and docs.

---

## Cookbook

Run these from the repository root. They use the checked-in sample files.

### 1. Look at CSV as JSON objects

```text
./pipe '< examples/orders.csv | csv | console'
```

```text
{"name":"Ann","amount":120,"region":"East"}
{"name":"Ben","amount":80,"region":"West"}
{"name":"Cam","amount":200,"region":"East"}
```

### 2. Filter CSV rows

```text
./pipe '< examples/orders.csv | csv | pick $region == /East/ | spec $name 1 / / next $amount next | console'
```

```text
Ann 120
Cam 200
```

### 3. Compute a line total, then keep the large ones

`orders.csv` already has an amount. This example builds a total from quantity
and price, the same shape as the tour in `examples/json_rows.txt`:

```text
./pipe 'literal Ann,3,40| literal customer,qty,price| csv | json add total = $qty * $price | pick $total > /100/ | spec $customer 1 / spent / next $total next | console'
```

```text
Ann spent 120
```

### 4. Flatten order items with SPLIT KEEP

```text
./pipe '< examples/order.json | json split items keep | spec $order 1 / / next $sku next / x/ next $qty next | console'
```

```text
A-1 NUT x2
A-1 BOLT x10
```

### 5. Expand a whole-file JSON array

```text
./pipe '< examples/catalog.json | json array | spec $name 1 / / next $city next | console'
```

```text
Ann Oslo
Cam Leeds
```

### 6. Pull one field

```text
./pipe '< examples/order.json | json get $order | console'
```

```text
A-1
```

```text
./pipe '< examples/order.json | spec $items.0.sku 1 | console'
```

```text
NUT
```

### 7. Round trip CSV to JSON to CSV

```text
./pipe '< examples/orders.csv | csv | csv write | console'
```

```text
name,amount,region
Ann,120,East
Ben,80,West
Cam,200,East
```

### 8. Split items, then collect them back

```text
./pipe '< examples/order.json | json split items | json collect items | console'
```

```text
{"items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]}
```

---

## Known limits and ideas

These are not IBM stages. What is here is a first increment.

- **LOCATE, SORT, and CHANGE do not take `$name` yet.** The range may already
  parse in some stages (SUBSTRING, UNIQUE, LOOKUP, ZONE extract the same
  display text). SORT currently rejects `sort $amount`
  (`SORT: unrecognized operand $amount`). CHANGE still wants two delimited
  strings. Whether `$name` should do more in LOCATE / SORT / CHANGE is open.
- **A top-level JSON array is not a row.** Use `json array` (or SPLIT on an
  array field) to get object records. There is no stage that writes a
  top-level JSON array; COLLECT wraps the array in an object field so later
  `$name` stages still see an object.
- **JSON ARRAY and JSON COLLECT hold the whole collection in memory.** There
  is no size cap. SPLIT does not; it walks one input record at a time.
- **SET / ADD do not grow arrays.** An out-of-range index is an error.
- **No `$["odd key"]`.** Field names in `$` refs are ASCII identifiers. CSV
  headers that are not identifiers need a later syntax.
- **No sidecar on the record.** REXX and Python still see bytes.
- **No JSON Lines file driver** as its own stage (the record already is one
  object; `<` plus `json array` is for one array document).
- **No Parquet, SQL, or Excel yet.** Those sit on the ideas list in
  [`STATUS.md`](STATUS.md). The object shape here is meant to be what a later
  reader or writer would produce and consume.
- SPEC writes plain text, so a JSON object does not survive SPEC unless you
  write JSON back out.

The harvested developer catalog still has a short JSON / CSV section in
[`examples-reference.md`](examples-reference.md). This page is the reference
to read and copy from.
