# JSON rows and CSV

This page teaches you how to work with named fields in Portable Pipelines.
When a record is a JSON object, you can pick a field by name, filter on it,
add a computed field, and walk an array, instead of counting columns.

Records still flow through stages connected by `|`. Stages still do not call
each other. SPEC, PICK, and friends still take an input range. A dollar sign
followed by a field name is one more kind of range: `$amount` is the value of
the amount field.

Sample files live next to the binary in a clone:

- [`examples/orders.csv`](../examples/orders.csv): three sales rows with a customer, the price of one item (amount), a region, and a quantity
- [`examples/library.json`](../examples/library.json): one library branch with a `books` array
- [`examples/catalog.json`](../examples/catalog.json): a pretty-printed array of customers
- [`examples/quoted.csv`](../examples/quoted.csv): a comma and a quote inside fields
- [`examples/contacts.csv`](../examples/contacts.csv): city and zip, with a leading zero in the zip
- [`examples/notes.txt`](../examples/notes.txt): two plain text lines (not JSON)
- [`examples/scores.json`](../examples/scores.json): a JSON array of numbers
- [`examples/broken.json`](../examples/broken.json): invalid JSON
- [`examples/guest.json`](../examples/guest.json): a JSON object whose `note` is null
- [`examples/inbox.json`](../examples/inbox.json): an Inbox object whose `messages` array is empty
- [`examples/aisles.json`](../examples/aisles.json): library aisles keyed `"0"` and `"1"` (object keys, not an array)
- [`examples/libs.jsonl`](../examples/libs.jsonl): three library branches, each with a nested `books` array (one JSON object per line)

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
JSON on the command line. That command above is the first walkthrough step;
the output is shown there.

On Linux and macOS, swap those double quotes for single quotes, and you can
use a slash in paths.

## JSON subcommands at a glance

There is one stage verb, `JSON`. The first operand chooses the subcommand.
There are eight.

| Subcommand | Syntax | What it does |
|---|---|---|
| ADD | `json add $total = expr` | Evaluate `expr` and insert `total`. Error if that leaf already exists. |
| SET | `json set $qty = expr` | Evaluate `expr` and create or replace `qty`. |
| DELETE | `json delete $field...` | Remove each named leaf. A missing leaf is a no-op. An array index is removed and later elements shift down. |
| KEEP | `json keep $field...` | Keep only those top-level fields. |
| GET | `json get $field` | Write the field as plain text (same display rules as SPEC). Error if missing. |
| SPLIT | `json split $field [keep]` | The named field must be a JSON array. Write one object record per element. `KEEP` copies the parent's other fields onto each child. |
| COLLECT | `json collect $field` | Read remaining object records and write one record `{"field":[...]}`. Held in memory. |
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

`examples\orders.csv` is three sales rows with a customer, the price of one
item (amount), a region, and a quantity:

```text
customer,amount,region,qty
Ann,120,East,3
Ben,80,West,1
Cam,200,East,2
```

The first row names the fields. Later rows are the data.

### 2. See the JSON rows

`csv` reads that header, then writes one JSON object per data row. This is the
shape the rest of the pipe works with.

```text
pipe "< examples\orders.csv | csv | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3}
{"customer":"Ben","amount":80,"region":"West","qty":1}
{"customer":"Cam","amount":200,"region":"East","qty":2}
```

Notice that `120` is a JSON number (no quotes around it) and `"Ann"` is a JSON
string. `COPY`, `TAKE`, and `FANOUT` pass those same bytes through. SPEC writes
plain text when it builds a line from fields, so the JSON object is gone then.

SPEC with field references builds a plain text line from the fields you
choose, so you are back to ordinary pipeline rows:

```text
pipe "< examples\orders.csv | csv | spec $customer 1 /,/ next $amount next /,/ next $region next | console"
```

```text
Ann,120,East
Ben,80,West
Cam,200,East
```

SPEC with an input range such as `1-*` copies the record unchanged, so the
JSON stays JSON in that case.

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
`$region` needed the dollar sign because `pick region == /East/` would be
`PICK: unexpected operand region`.

### 5. Add a computed field

`orders.csv` already has quantity and the price of one item. `json add`
inserts a new field on each object. Here `total` is quantity times that
price:

```text
pipe "< examples\orders.csv | csv | json add $total = $qty * $amount | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3,"total":360}
{"customer":"Ben","amount":80,"region":"West","qty":1,"total":80}
{"customer":"Cam","amount":200,"region":"East","qty":2,"total":400}
```

Notice each row now has a `total` field. PICK keeps the large ones, then
SPEC prints a plain text line:

```text
pipe "< examples\orders.csv | csv | json add $total = $qty * $amount | pick $total > /100/ | spec $customer 1 / spent / next $total next | console"
```

```text
Ann spent 360
Cam spent 400
```

Notice Ben's total is 80, so PICK dropped that row. Write `$qty` and
`$amount` in the expression, and `$total` for the field you are writing.
On JSON subcommands you may leave the `$` off that written name; see
[Optional `$` on JSON subcommands](#optional--on-json-subcommands).

### 6. Open an array

`examples\library.json` is one library branch whose `books` field is an
array. `json split` writes one record per element:

```text
pipe "< examples\library.json | json split $books | spec $title 1 | console"
```

```text
Dune
Emma
```

Notice the array became two ordinary object records, and `$title` then worked
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
text, or invalid JSON is an error. `examples\notes.txt` is two ordinary lines:

```text
pipe "< examples\notes.txt | json get $city | hole"
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get $city"
```

SPEC and PICK say the same thing with their own stage name:

```text
pipe "< examples\notes.txt | spec $city 1 | hole"
```

```text
PIPE: SPEC: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "spec $city 1"
```

A top-level JSON array is not an object record. Use `json array` to turn it
into objects first. `examples\scores.json` is `[90, 85, 72]`. `json get` on
that array fails the same way:

```text
pipe "< examples\scores.json | json get $value | hole"
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get $value"
```

Invalid JSON is that same object error, not a parser dump.
`examples\broken.json` looks like an object but the city value has no quotes:

```text
pipe "< examples\broken.json | json get $city | hole"
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get $city"
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

`$city` is a placeholder for the field named `city`. It is not the characters
dollar, c, i, t, y. Write `$qty` for the field `qty`, `$customer` for the
field `customer`, `$books.0.title` for a nested path.

```text
pipe "< examples\library.json | spec $books.0.title 1 | console"
```

```text
Dune
```

Notice `Dune` is the `title` of the first book (index 0).

Do not put a second dollar sign after the field word. `$qty$` would look like
a `$...$` delimited string. `$` plus a letter or `_` starts a field word, so
`$qty * $amount` is three tokens.

### Nested objects and array indexes

| Path | Meaning |
|---|---|
| `$user.city` | key `city` inside object `user` |
| `$books.0.title` | `title` of the first element of array `books` (0-based) |
| `$books[1].title` | the same path with brackets; no spaces |
| `$books.-1.title` / `$books[-1].title` | `title` of the last element |

```text
pipe "< examples\library.json | spec $books[1].title 1 | console"
```

```text
Emma
```

Notice index `1` is the second book.

```text
pipe "< examples\library.json | spec $books.-1.title 1 | console"
```

```text
Emma
```

Notice `-1` is the last book, which is also Emma in this sample.

A whole object or array prints as compact JSON:

```text
pipe "< examples\library.json | spec $books 1 | console"
```

```text
[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]
```

Notice SPEC did not walk the array; it wrote the array as one piece of text.

A numeric segment is an array index when that step is an array, and an object
key when that step is an object. `$books.0` is the first book. `$aisles.0` is
the aisle whose key is `"0"`. The same rule applies to the bracket form
(`$aisles[0]`). If that step is neither an array nor an object, the path is
missing (same as an out-of-range index).

```text
pipe "< examples\aisles.json | spec $aisles.0 1 | console"
```

```text
Fiction
```

```text
pipe "< examples\aisles.json | spec $aisles[1] 1 | console"
```

```text
History
```

Notice `"0"` and `"1"` are keys on the `aisles` object, not array indexes.

### Missing fields

| Situation | SPEC | PICK | JSON ADD / SET expression | JSON GET |
|---|---|---|---|---|
| Field is missing, out of range, or a numeric segment on a value that is not an array or an object | empty (the record is still written) | the comparison does not hold, including `!=` / `!==` / `^=` / `^==` / `¬=` / `¬==`; the record is not selected | error `JSON: field $price is missing` | error `JSON: field $price is missing` (the message echoes the operand you wrote) |
| Field is JSON `null` | empty | the comparison does not hold | (the null value is used) | empty |
| Record is not a JSON object | `SPEC: record is not a JSON object` | `PICK: record is not a JSON object` | `JSON: record is not a JSON object` | same |

This pipeline reads the sample CSV, then asks SPEC for a field that is not
there:

```text
pipe "< examples\orders.csv | csv | spec $customer 1 /:/ next $price next | console"
```

```text
Ann:
Ben:
Cam:
```

Notice the missing field became empty, and the colon still printed. SPEC does
not fail the pipeline for a missing field.

JSON `null` looks the same as missing in SPEC and GET. `examples\guest.json`
has `city` and a null `note`:

```text
pipe "< examples\guest.json | spec $city 1 /:/ next $note next | console"
```

```text
Oslo:
```

GET of that null field writes a blank line:

```text
pipe "< examples\guest.json | json get $note | console"
```

(one blank line)

PICK is stricter. A missing field never matches, even not-equal:

```text
pipe "< examples\orders.csv | csv | pick $price > /0/ | console"
```

(no records)

```text
pipe "< examples\orders.csv | csv | pick $price ^= /0/ | console"
```

(no records: a missing field never matches, even not-equal)

JSON ADD needs every field in the expression:

```text
pipe "< examples\orders.csv | csv | json add $total = $qty * $price | hole"
```

```text
PIPE: JSON: field $price is missing
... Issued from stage 3 of pipeline 1
... Running "json add $total = $qty * $price"
```

Notice the error names `$price`. The sample file has no `price` field.

In a pipeline specification, `/` starts a delimited string, so the IBM
synonym `/=` is a poor choice next to `/0/`. Write `!=` for numeric not-equal
and `!==` for string not-equal. `^=` and `¬=` still work.

```text
pipe "< examples\orders.csv | csv | pick $region !== /West/ | spec $customer 1 | console"
```

```text
Ann
Cam
```

```text
pipe "< examples\orders.csv | csv | pick $amount != /80/ | spec $customer 1 | console"
```

```text
Ann
Cam
```

Notice Ben (West, amount 80) is gone in both. `!=` is the numeric operator
(the same job as `^=`). `!==` is the string operator (the same job as `^==`).

---

## Optional `$` on JSON subcommands

Write `$field` everywhere. That is the same spelling SPEC and PICK use.

On JSON subcommands, the `$` on a **field-name operand** may be left off.
`json split $books keep` and `json split books keep` both work. So do
`json get $books` and `json delete $branch`.

Inside SPEC, PICK, and JSON ADD / SET **expressions**, the `$` is required.
There `qty` is not a field. `$qty` is.

| Place | `$` | Verified |
|---|---|---|
| `json add $total = …` (the name being written) | may be left off | `json add total = …` also writes `total` |
| `json set $qty = …` | may be left off | `json set qty = …` also works |
| `json delete $branch` | may be left off | `json delete branch` also drops the field |
| `json keep $customer $qty` | may be left off | `json keep customer qty` also works |
| `json get $branch`; `json get $books.0.title` | may be left off | `json get branch` also writes `Downtown` |
| `json split $books` (and the same with `keep`) | may be left off | `json split books` also writes `Dune` then `Emma` |
| `json collect $books` | may be left off | `json collect books` also collects into `books` |
| `json array` | no field name | |
| `$qty` in an ADD / SET **expression** | required | `json add $shout = city + /!/` is `JSON: unexpected c in expression` |
| `$customer` in SPEC | required | `spec customer 1` is `SPEC: unexpected operand customer` |
| `$region` in PICK | required | `pick region == /East/` is `PICK: unexpected operand region` |

KEEP and COLLECT take a **top-level** name only (`$customer`, not `$customer.qty`).

The GET / SPLIT missing-field messages echo the operand you wrote.
`json get $price` says `JSON: field $price is missing`. `json get price`
says `JSON: field price is missing`.

```text
pipe "< examples\library.json | json split $books | spec $title 1 | console"
pipe "< examples\library.json | json split books | spec $title 1 | console"
```

Both write:

```text
Dune
Emma
```

Write `$books` on the JSON subcommand. The bare name `books` also works.
The `$` on `$title` in SPEC is required:

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
keys are created when the next step is a key. On the sample guest record,
`json add $user.city = /Oslo/` builds a nested `user` object:

```text
pipe "< examples\guest.json | json add $user.city = /Oslo/ | console"
```

```text
{"city":"Oslo","note":null,"user":{"city":"Oslo"}}
```

Non-object input to every subcommand except ARRAY is
`JSON: record is not a JSON object`.

### ADD

We read one sales row from CSV, then insert `total`. ADD fails if that leaf
already exists.

```text
pipe "< examples\orders.csv | csv | take 1 | json add $total = $qty * $amount | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3,"total":360}
```

Notice `total` is a new numeric field on the same object.

```text
pipe "< examples\orders.csv | csv | json add $qty = 1 | hole"
```

```text
PIPE: JSON: field qty already exists; use SET to replace it
... Issued from stage 3 of pipeline 1
... Running "json add $qty = 1"
```

Notice ADD refused to overwrite `qty`. Use SET when you mean to replace.

### SET

SET evaluates the expression and writes the field, creating it if needed.

```text
pipe "< examples\orders.csv | csv | json set $qty = $qty + 4 | json get $qty | console"
```

```text
7
5
6
```

Notice `$qty` in the expression is the old value, and GET then writes the new
quantity as plain text. Ann's 3 became 7, Ben's 1 became 5, Cam's 2 became 6.

SET will also create a field that was not there. `/x/` is a string at the
start of the expression, so Command Prompt does not need extra quotes:

```text
pipe "< examples\orders.csv | csv | take 1 | json set $note = /x/ | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3,"note":"x"}
```

Notice `note` was added beside the sales fields.

### DELETE

DELETE removes each named leaf. A missing leaf is a no-op.

```text
pipe "< examples\orders.csv | csv | take 1 | json delete $region | console"
```

```text
{"customer":"Ann","amount":120,"qty":3}
```

Notice `region` is gone and the other fields stayed.

On the sample library, `json delete $branch` drops the branch and leaves the
books:

```text
pipe "< examples\library.json | json delete $branch | console"
```

```text
{"books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
```

Deleting an index shifts the array. Here we drop the first book of the sample
library:

```text
pipe "< examples\library.json | json delete $books.0 | console"
```

```text
{"branch":"Downtown","books":[{"title":"Emma","copies":1}]}
```

Notice Dune is gone and Emma moved to index 0.

### KEEP

KEEP keeps only the top-level fields you list.

```text
pipe "< examples\orders.csv | csv | take 1 | json keep $customer $qty | console"
```

```text
{"customer":"Ann","qty":3}
```

Notice `amount` and `region` are gone. Nested paths are rejected:
`json keep $customer.qty` is `JSON: KEEP takes top-level field names (not $a.b)`.

### GET

GET writes one field as plain text, with the same display rules as SPEC. A
missing field is an error.

```text
pipe "< examples\library.json | json get $branch | console"
```

```text
Downtown
```

Notice you get the branch only, not the whole object.

```text
pipe "< examples\orders.csv | csv | json get $price | hole"
```

```text
PIPE: JSON: field $price is missing
... Issued from stage 3 of pipeline 1
... Running "json get $price"
```

Notice the message names `$price`, matching the operand. The sample file has
no `price` field. Without the dollar sign, the message has no dollar sign
either:

```text
pipe "< examples\orders.csv | csv | json get price | hole"
```

```text
PIPE: JSON: field price is missing
... Issued from stage 3 of pipeline 1
... Running "json get price"
```

### SPLIT

Without `KEEP`, each array element becomes the new record. Object elements
stay objects. Anything else (`1`, `"a"`, `null`, `true`, a nested array) is
wrapped as `{"value": x}` so the record stays an object.

```text
pipe "< examples\library.json | json split $books | spec $title 1 | console"
```

```text
Dune
Emma
```

The records themselves:

```text
pipe "< examples\library.json | json split $books | console"
```

```text
{"title":"Dune","copies":3}
{"title":"Emma","copies":1}
```

Notice each book is now its own object record.

`KEEP` copies the parent object's other fields into each child, then overlays
the element (element keys win). The array field itself is dropped, so the
child does not carry the whole array.

```text
pipe "< examples\library.json | json split $books keep | console"
```

```text
{"branch":"Downtown","title":"Dune","copies":3}
{"branch":"Downtown","title":"Emma","copies":1}
```

Notice `branch` from the parent sits beside `title` and `copies` from the
child.

```text
pipe "< examples\library.json | json split $books keep | spec $branch 1 / / next $title next | console"
```

```text
Downtown Dune
Downtown Emma
```

An empty array writes nothing for that input record. `examples\inbox.json`
has `messages: []`:

```text
pipe "< examples\inbox.json | json split $messages | console"
```

(no records)

A missing field is `JSON: field $books is missing` when you wrote `$books`,
or `JSON: field books is missing` when you wrote `books`. A non-array field
(including `null`) is `JSON: $books is not a JSON array` (again echoing the
operand). SPLIT does not hold the rest of the stream; it writes the elements
of one record, then reads the next.

### COLLECT

The small inverse of SPLIT for object elements. It reads every remaining
object record, holds them, and writes one record. Empty input writes
`{"books":[]}`. There is no count operand and no undo of KEEP: scalar elements
that SPLIT wrapped as `{"value": x}` stay wrapped inside the collected array.
COLLECT does not write a top-level JSON array.

```text
pipe "< examples\library.json | json split $books | json collect $books | console"
```

```text
{"books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
```

Notice you are back to one object whose `books` field is the array. Later
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

`examples\scores.json` is a compact array of numbers. Non-object elements are
wrapped as `{"value": x}`:

```text
pipe "< examples\scores.json | json array | spec $value 1 | console"
```

```text
90
85
72
```

The sample library is an object, not an array, so ARRAY rejects it:

```text
pipe "< examples\library.json | json array | hole"
```

```text
PIPE: JSON: input is not a JSON array
... Issued from stage 2 of pipeline 1
... Running "json array"
```

---

## Expressions, strings, `/`, and SPEC separators

JSON ADD / SET expressions take JSON fields (`$qty`, `$books.0.title`), numbers
(`3`, `1.5`), and quoted strings. Operators: `+` `-` `*` `/` on numbers.
`+` concatenates when either side is a string. `|` or `||` also concatenates.
In a pipeline specification you must write `||` for a bar, because a single
`|` is the stage separator.

The reliable string form is a quoted string. `"..."` and `'...'` work, as do
the other IBM pair delimiters `%` `#` `!` `;` `,`. In Command Prompt, a quoted
`"!"` inside the pipeline quotes is written as `""!""`. After `+`, `/!/` is
also a string and is easier to type:

```text
pipe "< examples\catalog.json | json array | take 1 | json add $shout = $city + /!/ | json get $shout | console"
```

```text
Oslo!
```

Notice `$city` is the string `Oslo`, and `/!/` was concatenated.

`/` is division when it sits after a value, so `$amount / $qty` stays
arithmetic:

```text
pipe "< examples\orders.csv | csv | take 1 | json add $unit = $amount / $qty | json get $unit | console"
```

```text
40
```

At the start of an expression, or after `+` `-` `*` `(` or concat, `/.../` is
a string, so this also works. Prefer `/!/` or `""!""` when in doubt. If `/`
is treated as division, the `!` looks like an unclosed string
(`JSON: unclosed string in expression`).

`$qty-1` in an expression is the field `$qty` minus 1 (`$books.-1` is still
the last element of `books`).

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

The walkthrough that prints `Ann spent 360` puts the spaces inside the insert
text `/ spent /` and then uses `next`:

```text
pipe "< examples\orders.csv | csv | take 1 | json add $total = $qty * $amount | spec $customer 1 / spent / next $total next | console"
```

```text
Ann spent 360
```

`nextword` after `/spent/` would add a blank before `spent`, then `next`
would glue the total on with no blank (`Ann spent360`).

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
{"customer":"Ann","amount":"120","region":"East","qty":"3"}
{"customer":"Ben","amount":"80","region":"West","qty":"1"}
{"customer":"Cam","amount":"200","region":"East","qty":"2"}
```

Notice `"120"` is now a string. That changes `+`. On numbers, `+` adds. On
strings, `+` concatenates:

```text
pipe "< examples\orders.csv | csv | json add $doubled = $amount + $amount | spec $customer 1 / / next $doubled next | console"
```

```text
Ann 240
Ben 160
Cam 400
```

```text
pipe "< examples\orders.csv | csv strings | json add $doubled = $amount + $amount | json get $doubled | console"
```

```text
120120
8080
200200
```

Notice the first pipeline added, and the second glued the digits together.

`*` `/` `-` do not turn a numeric-looking string back into a number. After
`csv strings`, Ann's quantity is the string `"3"`, so times 2 is an error
that tells you to read without `STRINGS` or use a number:

```text
pipe "< examples\orders.csv | csv strings | take 1 | json add $doubled = $qty * 2 | hole"
```

```text
PIPE: JSON: * cannot use the string "3"; read without STRINGS, or use a number
... Issued from stage 4 of pipeline 1
... Running "json add $doubled = $qty * 2"
```

That is the same rule as `+`: a string stays a string. Use `csv` (without
`STRINGS`) when you want arithmetic on those cells.

Use `csv strings` when you want every field as a string. Leading zeros stay
strings even without `STRINGS`, because of the number rule.
`examples\contacts.csv` has zip `007` and `012`:

```text
pipe "< examples\contacts.csv | csv | json get $zip | console"
```

```text
007
012
```

Notice `007` did not become the number 7. `STRINGS` is for the reader only:
`csv strings write` is `CSV: STRINGS applies to the reader, not WRITE`.

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
pipe "< examples\orders.csv | csv | csv write | console"
```

```text
customer,amount,region,qty
Ann,120,East,3
Ben,80,West,1
Cam,200,East,2
```

The quoted sample round-trips as well:

```text
pipe "< examples\quoted.csv | csv | csv write | console"
```

```text
customer,note,msg
Ann,"x,y","say ""hi"""
```

Notice WRITE put the quotes back around the comma and the inner quote.

Plain text is not a JSON object. `csv write` on the notes file fails:

```text
pipe "< examples\notes.txt | csv write | hole"
```

```text
PIPE: CSV: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "csv write"
```

---

## Cookbook

Run these from the folder that holds `pipe`. They use the checked-in sample
files.

### 1. Look at CSV as JSON objects

```text
pipe "< examples\orders.csv | csv | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3}
{"customer":"Ben","amount":80,"region":"West","qty":1}
{"customer":"Cam","amount":200,"region":"East","qty":2}
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
pipe "< examples\orders.csv | csv | json add $total = $qty * $amount | pick $total > /100/ | spec $customer 1 / spent / next $total next | console"
```

```text
Ann spent 360
Cam spent 400
```

### 4. Flatten library books with SPLIT KEEP

```text
pipe "< examples\library.json | json split $books keep | spec $branch 1 / / next $title next / x/ next $copies next | console"
```

```text
Downtown Dune x3
Downtown Emma x1
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
pipe "< examples\library.json | json get $branch | console"
```

```text
Downtown
```

```text
pipe "< examples\library.json | spec $books.0.title 1 | console"
```

```text
Dune
```

### 7. Round trip CSV to JSON to CSV

```text
pipe "< examples\orders.csv | csv | csv write | console"
```

```text
customer,amount,region,qty
Ann,120,East,3
Ben,80,West,1
Cam,200,East,2
```

### 8. Split books, then collect them back

```text
pipe "< examples\library.json | json split $books | json collect $books | console"
```

```text
{"books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
```

### 9. Join children back to parents on `$branch`

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json delete $books | sort $branch | c: collate $branch | spec $branch 1 / / next $city next | console ? f: | json split $books keep | sort $branch | c:"
```

```text
Campus Trondheim
Campus Trondheim
Campus Trondheim
Downtown Oslo
Downtown Oslo
Downtown Oslo
Harbor Bergen
Harbor Bergen
```

[Parents, children, and putting a nest back](#parents-children-and-putting-a-nest-back)
walks through the same wiring and the LOOKUP / COLLECT variants.

---

## `$field` in SORT, LOCATE, CHANGE, COLLATE, and LOOKUP

`$amount` is one more kind of input range. A leading `$` is never IBM syntax,
so these stages can take a field path without clashing with column ranges.
A missing field is empty. A record that is not a JSON object is an error
with the stage name in front, the same wording SPEC and PICK use. The key
is the field's SPEC display text (a number `3` and a string `"3"` both
compare as the bytes `3`), then host byte order, the same way COLLATE
already compares a column range.

### SORT

`sort $region` orders records by the SPEC display text of that field
(binary, like IBM SORT on a column). Several fields are compared left to
right. IBM column ranges are still not implemented.

```text
pipe "< examples\orders.csv | csv | sort $region $customer | spec $customer 1 / / next $region next | console"
```

```text
Ann East
Cam East
Ben West
```

Notice the two East rows stay in customer order, then West.

### LOCATE and NLOCATE

`locate $region /West/` searches for that string in the region field, not in
the whole JSON text. With no search string, `locate $region` keeps records
whose field is not empty. NLOCATE is the inverse. IBM column ranges, MIXED,
ONES, and ZEROS are still not implemented.

```text
pipe "< examples\orders.csv | csv | locate $region /West/ | spec $customer 1 | console"
```

```text
Ben
```

```text
pipe "< examples\orders.csv | csv | nlocate $region /West/ | spec $customer 1 | console"
```

```text
Ann
Cam
```

### CHANGE

`change $region /East/North/` substitutes in that field, then writes the
object back. After a replacement, the field is stored as a JSON string.
If nothing matched, the record is unchanged (including its JSON types).
IBM column ranges are still not implemented.

```text
pipe "< examples\orders.csv | csv | take 1 | change $region /East/North/ | console"
```

```text
{"customer":"Ann","amount":120,"region":"North","qty":3}
```

A number becomes a string if CHANGE rewrites it. Ann's quantity is 3:

```text
pipe "< examples\orders.csv | csv | take 1 | change $qty /3/9/ | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":"9"}
```

Notice `"9"` has quotes. Use `json set` when you want a JSON number.

### COLLATE and LOOKUP

`collate $branch` matches a master stream to a detail stream on that field.
Write two ranges when the sides use different names: `collate $branch $id`
is the master key then the detail key. Both streams must already be sorted
on the key. IBM column ranges still work the same way.

`lookup $branch` does the same job without needing sorted input. Details
arrive on the primary; the reference is the secondary and is read first.
`lookup $branch $id` is the detail key then the reference key.

A missing field is empty, so two records that both lack `$branch` match
each other. Plain text is the same object error as SPEC:

```text
pipe "< examples\notes.txt | sort $city | hole"
```

```text
PIPE: SORT: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "sort $city"
```

```text
pipe "(ENDCHAR ?) < examples\notes.txt | c: collate $city | hole ? < examples\libs.jsonl | take 1 | c:"
```

```text
PIPE: COLLATE: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "collate $city"
```

The next section walks through a real nest: take library branches apart,
send parents and children down different streams, join them again on
`$branch`, and pack books back into an array.

---

## Parents, children, and putting a nest back

`examples\library.json` is one branch. `examples\libs.jsonl` is three
branches, one JSON object per line. Each row has a `books` array:

```text
{"branch":"Downtown","city":"Oslo","books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
{"branch":"Harbor","city":"Bergen","books":[{"title":"Sula","copies":2}]}
{"branch":"Campus","city":"Trondheim","books":[{"title":"Dune","copies":1},{"title":"Kindred","copies":4}]}
```

You can open that nest, work on the books as their own rows, and later
join them back to the branch they came from. COLLATE and LOOKUP now take
`$branch` as the match key, so you do not have to count columns.

### See the file

```text
pipe "< examples\libs.jsonl | console"
```

```text
{"branch":"Downtown","city":"Oslo","books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
{"branch":"Harbor","city":"Bergen","books":[{"title":"Sula","copies":2}]}
{"branch":"Campus","city":"Trondheim","books":[{"title":"Dune","copies":1},{"title":"Kindred","copies":4}]}
```

### Take the nest apart

`json delete $books` keeps the parent and drops the array:

```text
pipe "< examples\libs.jsonl | json delete $books | console"
```

```text
{"branch":"Downtown","city":"Oslo"}
{"branch":"Harbor","city":"Bergen"}
{"branch":"Campus","city":"Trondheim"}
```

`json split $books keep` writes one child per book and copies the other
parent fields onto it. The array itself is dropped, so the child does not
carry every sibling:

```text
pipe "< examples\libs.jsonl | json split $books keep | console"
```

```text
{"branch":"Downtown","city":"Oslo","title":"Dune","copies":3}
{"branch":"Downtown","city":"Oslo","title":"Emma","copies":1}
{"branch":"Harbor","city":"Bergen","title":"Sula","copies":2}
{"branch":"Campus","city":"Trondheim","title":"Dune","copies":1}
{"branch":"Campus","city":"Trondheim","title":"Kindred","copies":4}
```

Notice each book still knows its `branch`. That is the key you will match
on. Without `KEEP` you get only `title` and `copies`, and there is nothing
left to join with.

### Send parents and children down different streams

FANOUT copies each incoming row to every connected output. One output
deletes the array (parents). The other splits the array (children). The
`?` starts the next pipeline in the set, and `f:` is the FANOUT label.
`(ENDCHAR ?)` (or `(end ?)`) names `?` as that separator. In Command
Prompt the whole specification stays inside one pair of double quotes.

COLLATE needs both sides sorted on the key. `sort $branch` does that.

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json delete $books | sort $branch | c: collate $branch | console ? f: | json split $books keep | sort $branch | c:"
```

```text
{"branch":"Campus","city":"Trondheim"}
{"branch":"Campus","city":"Trondheim","title":"Dune","copies":1}
{"branch":"Campus","city":"Trondheim","title":"Kindred","copies":4}
{"branch":"Downtown","city":"Oslo"}
{"branch":"Downtown","city":"Oslo","title":"Dune","copies":3}
{"branch":"Downtown","city":"Oslo","title":"Emma","copies":1}
{"branch":"Harbor","city":"Bergen"}
{"branch":"Harbor","city":"Bergen","title":"Sula","copies":2}
```

Notice the default write is the parent, then each matching child. Campus
sorts first. The children sit next to their parent as separate records.
They are not stuffed back into a `books` array yet.

If only Emma remains on the child side, Campus and Harbor have no match.
Matched rows still leave on the primary. Unmatched parents leave on
COLLATE's secondary output:

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json delete $books | sort $branch | c: collate $branch | console ? f: | json split $books keep | pick $title == /Emma/ | sort $branch | c:"
```

```text
{"branch":"Downtown","city":"Oslo"}
{"branch":"Downtown","city":"Oslo","title":"Emma","copies":1}
```

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json delete $books | sort $branch | c: collate $branch | hole ? f: | json split $books keep | pick $title == /Emma/ | sort $branch | c: | console ? c: | hole"
```

```text
{"branch":"Campus","city":"Trondheim"}
{"branch":"Harbor","city":"Bergen"}
```

The first of those two commands prints the match. The second prints the
unmatched parents. The last `c: | hole` is COLLATE's tertiary (unmatched
children); it is empty here.

### LOOKUP when the streams are not sorted

LOOKUP reads the whole reference first, then matches each detail as it
arrives. You do not sort. The usual wiring is two reads of the same file:
children on the primary, parents on the secondary. Default write is each
child followed by the parent it matched.

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | json split $books keep | L: lookup $branch | console ? < examples\libs.jsonl | json delete $books | L:"
```

```text
{"branch":"Downtown","city":"Oslo","title":"Dune","copies":3}
{"branch":"Downtown","city":"Oslo"}
{"branch":"Downtown","city":"Oslo","title":"Emma","copies":1}
{"branch":"Downtown","city":"Oslo"}
{"branch":"Harbor","city":"Bergen","title":"Sula","copies":2}
{"branch":"Harbor","city":"Bergen"}
{"branch":"Campus","city":"Trondheim","title":"Dune","copies":1}
{"branch":"Campus","city":"Trondheim"}
{"branch":"Campus","city":"Trondheim","title":"Kindred","copies":4}
{"branch":"Campus","city":"Trondheim"}
```

Notice Downtown's parent is written twice, once per child. `lookup
$branch detail` writes only the matching children.

FANOUT plus LOOKUP is easy to stall. LOOKUP will not read children until
the parent stream has ended, and FANOUT wants to write both sides of the
same row together. Two reads of the file avoid that. If you do use
FANOUT, put `elastic` on the child path before LOOKUP so the children can
wait:

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json split $books keep | elastic | L: lookup $branch | console ? f: | json delete $books | L:"
```

That writes the same ten records as the two-read pipeline above.

### Pack children back into an array

`json collect $books` reads every remaining object and writes one record
`{"books":[...]}`. It does not group by branch. That is the clumsy part,
and it is the stage we have.

To rebuild one branch, pick that branch first, split, then collect.
Harbor has one book:

```text
pipe "< examples\libs.jsonl | pick $branch == /Harbor/ | json split $books | json collect $books | console"
```

```text
{"books":[{"title":"Sula","copies":2}]}
```

That is a nested record again. It is not the original Harbor row: `branch`
and `city` are gone. There is no stage that pastes this array back onto
`{"branch":"Harbor","city":"Bergen"}`. Those two objects can sit next to
each other (two records). They do not become one object.

A useful whole-file collect is: keep the books you still care about, then
pack them. Here we drop copies of 1 (Downtown Emma and Campus Dune), look
the rest up against the parent stream so only real branches remain, keep
title and copies, and collect:

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | json split $books keep | pick $copies > /1/ | L: lookup $branch detail | json keep $title $copies | json collect $books | console ? < examples\libs.jsonl | json delete $books | L:"
```

```text
{"books":[{"title":"Dune","copies":3},{"title":"Sula","copies":2},{"title":"Kindred","copies":4}]}
```

Notice that is one array for the whole stream, in child order, not three
nested branch records. If you collect the COLLATE mix (parents and
children together), the parents land inside the array too. Prefer
`collate $branch detail` or `lookup $branch detail` when the next stage is
COLLECT.

So the round trip that really works is: take the nest apart, join on
`$branch`, and collect the children you kept. You get a books array back.
You do not get the original three nested branch objects back from COLLECT
alone.

---

## Not yet

A few things are still open, and a few behaviours are good to know.

**JSON ARRAY holds the whole document in memory.** JSON COLLECT does too.
There is no size cap. SPLIT does not; it walks one input record at a time.

SUBSTRING, UNIQUE, and ZONE already extract `$field` as the same
display text SPEC writes. A non-object record is empty there, not an
error. COLLATE and LOOKUP take `$field` with the same missing (empty) and
non-object (`record is not a JSON object`) rules as SORT. IBM column
ranges on SORT, LOCATE, and CHANGE are still not implemented.

### Good to know

- A numeric path segment is an array index on an array and an object key on
  an object. `$aisles.0` selects the key `"0"`. `$books.0` still indexes the
  `books` array.
- `csv strings` keeps every field as a string. `+` concatenates. `*` `/` `-`
  on a string are an error (`read without STRINGS, or use a number`).
- PICK `/=` next to `/0/` does not parse as not-equal, because `/` starts a
  delimited string. Write `!=` (numeric) or `!==` (string), or `^=` / `¬=`.
- GET and SPLIT missing-field messages echo the operand you wrote (`price`
  or `$price`).
- SET / ADD do not grow arrays. An out-of-range index is an error.
- CHANGE `$field` stores a JSON string after a replacement.
- There is no `$["odd key"]`. Field names in `$` refs are ASCII identifiers.
  CSV headers that are not identifiers need a later syntax.
- REXX and Python still see bytes. There is no sidecar on the record.
- There is no JSON Lines file driver as its own stage (the record already is
  one object; `<` plus `json array` is for one array document).
- There is no stage that writes a top-level JSON array. COLLECT wraps the
  array in an object field so later `$field` stages still see an object.
- SPEC writes plain text when it builds a line from fields, so the JSON
  object is gone then. An input range such as `1-*` copies the record
  unchanged, so the JSON stays JSON.

The harvested developer catalog still has a short JSON / CSV section in
[`examples-reference.md`](examples-reference.md). This page is the one to
read and copy from.
