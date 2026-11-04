# JSON rows and CSV

This page teaches you how to work with named fields in Portable Pipelines.
When a record is a JSON object, you can pick a field by name, filter on it,
add a computed field, and walk an array, instead of counting columns.

Records still flow through stages connected by `|`. Stages still do not call
each other. SPEC, PICK, and friends still take an input range. A dollar sign
followed by a field name is one more kind of range: `$amount` is the value of
the amount field.

Sample files live next to the binary in a clone:

- [`examples/orders.csv`](../examples/orders.csv): three sales rows
- [`examples/order.json`](../examples/order.json): one order with an `items` array
- [`examples/catalog.json`](../examples/catalog.json): a pretty-printed array of customers
- [`examples/quoted.csv`](../examples/quoted.csv): a comma and a quote inside fields

Design notes for how the stages are built live in
[`design/json-rows.md`](design/json-rows.md).

## Running these examples in Command Prompt

Open Command Prompt and change directory to the folder that holds `pipe.exe`
(the repository root, if you built from source). Each example is one command.
The whole pipeline sits inside double quotes:

```text
pipe "< examples\orders.csv | csv | console"
```

If Command Prompt does not find `pipe`, write `pipe.exe` instead. Paths use a
backslash. JSON text itself uses double quotes, which Command Prompt also uses
to wrap the pipeline, so the examples read the sample files rather than typing
JSON on the command line. Tiny CSV rows with `literal` are easy to type, and
`csv` turns them into JSON objects. That command above is the first walkthrough
step; the output is shown there.

On Linux and macOS, swap those double quotes for single quotes, and you can
use a slash in paths.

## JSON subcommands at a glance

There is one stage verb, `JSON`. The first operand chooses the subcommand.
There are eight.

| Subcommand | Syntax | What it does |
|---|---|---|
| ADD | `json add total = expr` | Evaluate `expr` and insert `total`. Error if that leaf already exists. |
| SET | `json set n = expr` | Evaluate `expr` and create or replace `n`. |
| DELETE | `json delete fields...` | Remove each named leaf. A missing leaf is a no-op. An array index is removed and later elements shift down. |
| KEEP | `json keep fields...` | Keep only those top-level fields. |
| GET | `json get field` | Write the field as plain text (same display rules as SPEC). Error if missing. |
| SPLIT | `json split field [keep]` | The named field must be a JSON array. Write one object record per element. `KEEP` copies the parent's other fields onto each child. |
| COLLECT | `json collect field` | Read remaining object records and write one record `{"field":[...]}`. Held in memory. |
| ARRAY | `json array` | Read a top-level JSON array (one record, or pretty-printed over many records from `<`) and write one object record per element. Held in memory. |

CSV is a separate stage:

```text
csv
csv strings
csv write
```

The rest of this page walks through the ideas in a gentle order, then shows
each subcommand with a verified example.

---

## A short walkthrough

If you are new, start here. We will read a tiny CSV file, look at the JSON
objects it becomes, pick a field, keep some rows, add a computed field, and
then open an array.

### 1. A tiny CSV file

`examples\orders.csv` is three sales rows and a header:

```text
customer,amount,region
Ann,120,East
Ben,80,West
Cam,200,East
```

The first row names the fields. Later rows are the data.

### 2. See the JSON rows

`csv` reads that header, then writes one JSON object per data row. This is the
shape the rest of the pipe works with.

```text
pipe "< examples\orders.csv | csv | console"
```

```text
{"customer":"Ann","amount":120,"region":"East"}
{"customer":"Ben","amount":80,"region":"West"}
{"customer":"Cam","amount":200,"region":"East"}
```

Notice that `120` is a JSON number (no quotes around it) and `"Ann"` is a JSON
string. `COPY`, `TAKE`, and `FANOUT` pass those same bytes through. SPEC writes
plain text, so after SPEC the JSON object is gone unless the new bytes are
themselves JSON.

### 3. Pick a field

To name a field, write a dollar sign followed by the field name. So
`$customer` is the value of the customer field. Here SPEC copies just that
field to a new record:

```text
pipe "< examples\orders.csv | csv | spec $customer 1 | console"
```

```text
Ann
Ben
Cam
```

Notice you get the three customer values as ordinary text. The JSON wrapping
is gone, which is what you want when you are building a line to print.

### 4. Filter

PICK can compare a field. We keep the East rows, then print customer and
amount with a blank between them:

```text
pipe "< examples\orders.csv | csv | pick $region == /East/ | spec $customer 1 / / next $amount next | console"
```

```text
Ann 120
Cam 200
```

Notice Ben (West) is gone. PICK compared the region field as a string.
`$region` needed the dollar sign; `pick region == /East/` would be
`PICK: unexpected operand region`.

### 5. Add a computed field

`orders.csv` already has an amount. This example builds a total from quantity
and price, the same shape as the tour in `examples\json_rows.txt`. Two
`literal` stages write a header and a data row (chained `literal` writes in
reverse order, which is why the header line is listed second). `csv` makes an
object, `json add` inserts `total`, and PICK keeps the large ones:

```text
pipe "literal Ann,3,40| literal customer,qty,price| csv | json add total = $qty * $price | pick $total > /100/ | spec $customer 1 / spent / next $total next | console"
```

```text
Ann spent 120
```

Notice `3 * 40` became `120`, and that value is what PICK compared. In the
expression, `$qty` and `$price` need the dollar sign. The field you are
writing (`total`) may be written with or without one; see
[Optional `$` on JSON subcommands](#optional--on-json-subcommands).

### 6. Open an array

`examples\order.json` is one order whose `items` field is an array. `json split`
writes one record per element:

```text
pipe "< examples\order.json | json split items | spec $sku 1 | console"
```

```text
NUT
BOLT
```

Notice the array became two ordinary object records, and `$sku` then worked
the same way `$customer` did on the CSV rows. [JSON SPLIT](#split) and
[JSON ARRAY](#array) cover the details.

---

## The row shape

The record is still bytes. When those bytes are a JSON **object** (optional
surrounding blanks), a `$field` reference and the JSON / CSV stages treat them
as named fields.

**Numbers stay numbers. Strings stay strings.** That difference matters for
`+`. After `csv`, amount is a number, so `$amount + $amount` adds. After
`csv strings`, amount is a string (`"120"`), so the same `+` concatenates.
See [CSV](#csv).

**The record must be an object** for `$field`, JSON ADD / SET / DELETE / KEEP /
GET / SPLIT / COLLECT, and CSV WRITE. A JSON array, a JSON primitive, plain
text, or invalid JSON is an error. Plain text looks like this:

```text
pipe "literal hello | json get customer | hole"
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get customer"
```

SPEC and PICK say the same thing with their own stage name:

```text
pipe "literal hello | spec $customer 1 | hole"
```

```text
PIPE: SPEC: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "spec $customer 1"
```

A top-level JSON array is not an object record. Use `json array` to turn it
into objects first. `json get` on an array fails the same way:

```text
pipe "literal [1,2,3] | json get value | hole"
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get value"
```

Invalid JSON is that same object error, not a parser dump. `{nope}` has no
double quotes, so it is safe to type in Command Prompt:

```text
pipe "literal {nope} | json get a | hole"
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

## Field paths

`$bleh` is a placeholder for the field named `bleh`. It is not the literal
characters dollar, b, l, e, h. Write `$qty` for the field `qty`, `$customer`
for the field `customer`, `$items.0.sku` for a nested path.

```text
pipe "< examples\order.json | spec $items.0.sku 1 | console"
```

```text
NUT
```

Notice `NUT` is the `sku` of the first item (index 0).

Do not put a second dollar sign after the field word. `$qty$` would look like
a `$...$` delimited string. `$` plus a letter or `_` starts a field word, so
`$qty * $price` is three tokens.

### Nested objects and array indexes

| Path | Meaning |
|---|---|
| `$user.city` | key `city` inside object `user` |
| `$items.0.sku` | `sku` of the first element of array `items` (0-based) |
| `$items[1].sku` | the same path with brackets; no spaces |
| `$items.-1.sku` / `$items[-1].sku` | `sku` of the last element |

```text
pipe "< examples\order.json | spec $items[1].sku 1 | console"
```

```text
BOLT
```

Notice index `1` is the second item.

```text
pipe "< examples\order.json | spec $items.-1.sku 1 | console"
```

```text
BOLT
```

Notice `-1` is the last item, which is also BOLT in this sample.

A whole object or array prints as compact JSON:

```text
pipe "< examples\order.json | spec $items 1 | console"
```

```text
[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]
```

Notice SPEC did not walk the array; it wrote the array as one piece of text.

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

This pipeline builds a one-field object from CSV, then asks SPEC for a field
that is not there:

```text
pipe "literal Ann| literal customer| csv | spec $customer 1 /:/ next $missing next | console"
```

```text
Ann:
```

Notice the missing field became empty, and the colon still printed. SPEC does
not fail the pipeline for a missing field.

PICK is stricter. A missing field never matches, even not-equal:

```text
pipe "literal East| literal region| csv | pick $amount > /0/ | console"
```

(no records)

```text
pipe "literal East| literal region| csv | pick $amount ^= /0/ | console"
```

(no records: a missing field never matches, even not-equal)

JSON ADD needs every field in the expression:

```text
pipe "literal 3| literal qty| csv | json add total = $qty * $price | hole"
```

```text
PIPE: JSON: field $price is missing
... Issued from stage 4 of pipeline 1
... Running "json add total = $qty * $price"
```

Notice the error names `$price`. We built the object from a one-column CSV so
Command Prompt did not have to quote JSON.

In a pipeline specification, `/` starts a delimited string, so the PICK
synonym `/=` is a poor choice next to `/0/`. Use `^=` or `¬=`.

---

## Optional `$` on JSON subcommands

JSON **field-name operands** accept a name with or without a leading `$`.
`json split items keep` and `json split $items keep` both work. So do
`json get $items` and `json delete $order`.

**Inside SPEC, PICK, and JSON ADD / SET expressions, the `$` is required.**
There `qty` is not a field. `$qty` is.

| Place | `$` | Verified |
|---|---|---|
| `json add total = …` / `json add $total = …` (the name being written) | optional | both write `total` |
| `json set n = …` / `json set $n = …` | optional | both produce `5` |
| `json delete b` / `json delete $b`; `json delete order` / `json delete $order` | optional | both drop the named field |
| `json keep a c` / `json keep $a $c` | optional | both keep `a` and `c` |
| `json get a` / `json get $a`; `json get items.0.sku` / `json get $items.0.sku` | optional | both write `1` or `NUT` |
| `json split items` / `json split $items` (and the same with `keep`) | optional | both write `NUT` then `BOLT` |
| `json collect items` / `json collect $items` | optional | both collect into `items` |
| `json array` | no field name | |
| `$qty` in an ADD / SET **expression** | required | `json add shout = bleh + /!/` is `JSON: unexpected b in expression` |
| `$customer` in SPEC | required | `spec customer 1` is `SPEC: unexpected operand customer` |
| `$region` in PICK | required | `pick region == /East/` is `PICK: unexpected operand region` |

KEEP and COLLECT take a **top-level** name only (`a` or `$a`, not `$a.b`).

The GET / SPLIT missing-field messages always print a `$` in the path
(`JSON: field $missing is missing`), even if you wrote the name without one.

```text
pipe "< examples\order.json | json split items | spec $sku 1 | console"
pipe "< examples\order.json | json split $items | spec $sku 1 | console"
```

Both write:

```text
NUT
BOLT
```

Notice the `$` on `items` is optional. The `$` on `$sku` in SPEC is not:

```text
pipe "< examples\orders.csv | csv | spec customer 1 | console"
```

```text
PIPE: SPEC: unexpected operand customer
... Issued from stage 3 of pipeline 1
... Running "spec customer 1"
```

---

## JSON subcommands

ADD / SET do not grow arrays. A missing array or an out-of-range index is an
error (`JSON: index 9 is out of range`). Intermediate objects for missing
keys are created when the next step is a key (`json add user.city = /Oslo/`
builds `{"user":{"city":"Oslo"}}` on the way).

Non-object input to every subcommand except ARRAY is
`JSON: record is not a JSON object`.

### ADD

We build a small object from CSV, then insert `total`. ADD fails if that leaf
already exists.

```text
pipe "literal Ann,3,40| literal customer,qty,price| csv | json add total = $qty * $price | console"
```

```text
{"customer":"Ann","qty":3,"price":40,"total":120}
```

Notice `total` is a new numeric field on the same object.

```text
pipe "literal 1| literal n| csv | json add n = 2 | hole"
```

```text
PIPE: JSON: field n already exists; use SET to replace it
... Issued from stage 4 of pipeline 1
... Running "json add n = 2"
```

Notice ADD refused to overwrite `n`. Use SET when you mean to replace.

### SET

SET evaluates the expression and writes the field, creating it if needed.

```text
pipe "literal 1| literal n| csv | json set n = $n + 4 | json get n | console"
```

```text
5
```

Notice `$n` in the expression is the old value, and GET then writes `5` as
plain text.

SET will also create a field that was not there. `/x/` is a string at the
start of the expression, so Command Prompt does not need extra quotes:

```text
pipe "literal 1| literal n| csv | json set extra = /x/ | console"
```

```text
{"n":1,"extra":"x"}
```

Notice `extra` was added beside `n`.

### DELETE

DELETE removes each named leaf. A missing leaf is a no-op.

```text
pipe "literal 1,2,3| literal a,b,c| csv | json delete b | console"
```

```text
{"a":1,"c":3}
```

Notice `b` is gone and `a` and `c` stayed.

On the sample order, `json delete $order` (or `json delete order`) drops the
order id and leaves the items:

```text
pipe "< examples\order.json | json delete $order | console"
```

```text
{"items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]}
```

Deleting an index shifts the array. Here we drop the first item of the sample
order:

```text
pipe "< examples\order.json | json delete items.0 | console"
```

```text
{"order":"A-1","items":[{"sku":"BOLT","qty":10}]}
```

Notice NUT is gone and BOLT moved to index 0.

### KEEP

KEEP keeps only the top-level fields you list.

```text
pipe "literal 1,2,3| literal a,b,c| csv | json keep a c | json get a | console"
```

```text
1
```

The object after `json keep a c` is `{"a":1,"c":3}`. Nested paths are
rejected: `json keep a.b` is `JSON: KEEP takes top-level field names (not $a.b)`.

### GET

GET writes one field as plain text, with the same display rules as SPEC. A
missing field is an error.

```text
pipe "< examples\order.json | json get $order | console"
```

```text
A-1
```

Notice you get the order id only, not the whole object.

```text
pipe "literal 1| literal a| csv | json get missing | hole"
```

```text
PIPE: JSON: field $missing is missing
... Issued from stage 4 of pipeline 1
... Running "json get missing"
```

Notice the message prints `$missing` even though the operand was written
without a dollar sign.

### SPLIT

Without `KEEP`, each array element becomes the new record. Object elements
stay objects. Anything else (`1`, `"a"`, `null`, `true`, a nested array) is
wrapped as `{"value": x}` so the record stays an object.

```text
pipe "< examples\order.json | json split items | spec $sku 1 | console"
```

```text
NUT
BOLT
```

The records themselves:

```text
pipe "< examples\order.json | json split items | console"
```

```text
{"sku":"NUT","qty":2}
{"sku":"BOLT","qty":10}
```

Notice each item is now its own object record.

`KEEP` copies the parent object's other fields into each child, then overlays
the element (element keys win). The array field itself is dropped, so the
child does not carry the whole array.

```text
pipe "< examples\order.json | json split items keep | console"
```

```text
{"order":"A-1","sku":"NUT","qty":2}
{"order":"A-1","sku":"BOLT","qty":10}
```

Notice `order` from the parent sits beside `sku` and `qty` from the child.

```text
pipe "< examples\order.json | json split items keep | spec $order 1 / / next $sku next | console"
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
pipe "< examples\order.json | json split items | json collect items | console"
```

```text
{"items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]}
```

Notice you are back to one object whose `items` field is the array. Later
`$field` stages still see an object.

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
pipe "< examples\catalog.json | json array | spec $customer 1 / / next $city next | console"
```

```text
Ann Oslo
Cam Leeds
```

Notice the file was one JSON array, and you now have one object record per
element.

A compact array of numbers has no double quotes, so it is safe on the command
line. Non-object elements are wrapped as `{"value": x}`:

```text
pipe "literal [1,2,3] | json array | spec $value 1 | console"
```

```text
1
2
3
```

The sample order is an object, not an array, so ARRAY rejects it:

```text
pipe "< examples\order.json | json array | hole"
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
the other IBM pair delimiters `%` `#` `!` `;` `,`. In Command Prompt, a quoted
`"!"` inside the pipeline quotes is written as `""!""`. After `+`, `/!/` is
also a string and is easier to type:

```text
pipe "literal hi| literal item| csv | json add shout = $item + /!/ | json get shout | console"
```

```text
hi!
```

Notice `$item` is the string `hi`, and `/!/` was concatenated.

`/` is division when it sits after a value, so `$qty / $price` stays
arithmetic:

```text
pipe "literal 8,2| literal qty,price| csv | json add n = $qty / $price | json get n | console"
```

```text
4
```

At the start of an expression, or after `+` `-` `*` `(` or concat, `/.../` is
a string, so this also works. Prefer `/!/` or `""!""` when in doubt. If `/`
is treated as division, the `!` looks like an unclosed string
(`JSON: unclosed string in expression`).

`$qty-1` in an expression is the field `$qty` minus 1 (`$items.-1` is still
the last element of `items`).

### SPEC `next` and `nextword`

`nextword` inserts a blank between what is already in the output record and
this piece. `next` does not; it just appends.

```text
pipe "< examples\orders.csv | csv | take 1 | spec $customer 1 $amount next | console"
```

```text
Ann120
```

Notice there is no blank. `next` glued the amount on.

```text
pipe "< examples\orders.csv | csv | take 1 | spec $customer 1 $amount nextword | console"
```

```text
Ann 120
```

Notice `nextword` inserted the blank for you.

The examples that print `Ann spent 120` put the spaces inside the insert
text `/ spent /` and then use `next`:

```text
pipe "< examples\orders.csv | csv | take 1 | spec $customer 1 / spent / next $amount next | console"
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

`csv strings` stores every field as a string. Compare the amounts here with
the walkthrough output above:

```text
pipe "< examples\orders.csv | csv strings | console"
```

```text
{"customer":"Ann","amount":"120","region":"East"}
{"customer":"Ben","amount":"80","region":"West"}
{"customer":"Cam","amount":"200","region":"East"}
```

Notice `"120"` is now a string. That changes `+`. On numbers, `+` adds. On
strings, `+` concatenates:

```text
pipe "< examples\orders.csv | csv | json add doubled = $amount + $amount | spec $customer 1 / / next $doubled next | console"
```

```text
Ann 240
Ben 160
Cam 400
```

```text
pipe "< examples\orders.csv | csv strings | json add doubled = $amount + $amount | json get doubled | console"
```

```text
120120
8080
200200
```

Notice the first pipeline added, and the second glued the digits together.

`*` `/` `-` still try to parse a numeric-looking string, so
`csv strings | json add doubled = $n * 2` on a cell `3` writes `6`. Use
`csv strings` when you want to keep leading zeros (`007` stays `"007"` even
without `STRINGS`, because of the number rule). `STRINGS` is for the reader
only: `csv strings write` is `CSV: STRINGS applies to the reader, not WRITE`.

Quoted commas and doubled quotes are easier to see in a file than on the
command line. `examples\quoted.csv` has a comma inside `note` and a quote
inside `msg`:

```text
pipe "< examples\quoted.csv | csv | spec $note 1 / :: / next $msg next | console"
```

```text
x,y :: say "hi"
```

Notice the comma stayed inside `note`, and the doubled quote in the file
became a single quote in the field.

### CSV WRITE

`csv write` reads JSON objects. The first object's keys (insertion order)
are the header. Later objects use those keys; a missing key is empty; extra
keys on later objects are ignored. Fields that contain a comma, quote, CR,
LF, or a leading or trailing blank are quoted.

```text
pipe "literal Ann,3,40| literal customer,qty,price| csv | csv write | console"
```

```text
customer,qty,price
Ann,3,40
```

(Chained `literal` writes in reverse order, which is why the header line is
listed second in the specification and comes out first.)

The quoted sample round-trips as well:

```text
pipe "< examples\quoted.csv | csv | csv write | console"
```

```text
customer,note,msg
Ann,"x,y","say ""hi"""
```

Notice WRITE put the quotes back around the comma and the inner quote.

Plain text is `CSV: record is not a JSON object`.

---

## Cookbook

Run these from the folder that holds `pipe`. They use the checked-in sample
files.

### 1. Look at CSV as JSON objects

```text
pipe "< examples\orders.csv | csv | console"
```

```text
{"customer":"Ann","amount":120,"region":"East"}
{"customer":"Ben","amount":80,"region":"West"}
{"customer":"Cam","amount":200,"region":"East"}
```

### 2. Filter CSV rows

```text
pipe "< examples\orders.csv | csv | pick $region == /East/ | spec $customer 1 / / next $amount next | console"
```

```text
Ann 120
Cam 200
```

### 3. Compute a line total, then keep the large ones

```text
pipe "literal Ann,3,40| literal customer,qty,price| csv | json add total = $qty * $price | pick $total > /100/ | spec $customer 1 / spent / next $total next | console"
```

```text
Ann spent 120
```

### 4. Flatten order items with SPLIT KEEP

```text
pipe "< examples\order.json | json split items keep | spec $order 1 / / next $sku next / x/ next $qty next | console"
```

```text
A-1 NUT x2
A-1 BOLT x10
```

### 5. Expand a whole-file JSON array

```text
pipe "< examples\catalog.json | json array | spec $customer 1 / / next $city next | console"
```

```text
Ann Oslo
Cam Leeds
```

### 6. Pull one field

```text
pipe "< examples\order.json | json get $order | console"
```

```text
A-1
```

```text
pipe "< examples\order.json | spec $items.0.sku 1 | console"
```

```text
NUT
```

### 7. Round trip CSV to JSON to CSV

```text
pipe "< examples\orders.csv | csv | csv write | console"
```

```text
customer,amount,region
Ann,120,East
Ben,80,West
Cam,200,East
```

### 8. Split items, then collect them back

```text
pipe "< examples\order.json | json split items | json collect items | console"
```

```text
{"items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]}
```

---

## Not yet

A few things are still open, and a few behaviours are good to know.

**SORT, CHANGE, and LOCATE do not take `$field` yet.** The range may already
parse in some stages (SUBSTRING, UNIQUE, LOOKUP, ZONE extract the same
display text). SORT currently rejects `sort $amount`
(`SORT: unrecognized operand $amount`). CHANGE still wants two delimited
strings. `locate $region` on the sample CSV objects selects nothing.

**JSON ARRAY holds the whole document in memory.** JSON COLLECT does too.
There is no size cap. SPLIT does not; it walks one input record at a time.

### Good to know

- A segment that looks like an integer is always an array index. `$obj.0` on
  an object whose key is `"0"` is missing, not that key.
- `csv strings` changes `+` to concatenation. `*` `/` `-` still parse a
  numeric-looking string, so a cell `3` times 2 is `6`.
- In a pipeline specification, `/` starts a delimited string, so PICK `/=`
  next to `/0/` does not parse as not-equal. Use `^=` or `¬=`.
- GET and SPLIT missing-field messages always print a `$` in the path, even
  if you wrote the name without one.
- SET / ADD do not grow arrays. An out-of-range index is an error.
- There is no `$["odd key"]`. Field names in `$` refs are ASCII identifiers.
  CSV headers that are not identifiers need a later syntax.
- REXX and Python still see bytes. There is no sidecar on the record.
- There is no JSON Lines file driver as its own stage (the record already is
  one object; `<` plus `json array` is for one array document).
- There is no stage that writes a top-level JSON array. COLLECT wraps the
  array in an object field so later `$field` stages still see an object.
- SPEC writes plain text, so a JSON object does not survive SPEC unless you
  write JSON back out.

The harvested developer catalog still has a short JSON / CSV section in
[`examples-reference.md`](examples-reference.md). This page is the one to
read and copy from.
