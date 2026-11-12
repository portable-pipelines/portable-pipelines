# JSON rows and CSV

This page teaches you how to work with named fields in Portable Pipelines.
When a record is a JSON object, you can pick a field by name, filter on it,
add a computed field, and walk an array, instead of counting columns.

Records still flow through stages connected by `|`. Stages still do not call
each other. SPEC, PICK, and friends still take an input range. A dollar sign
followed by a field name is one more kind of range: `$amount` is the value of
the amount field.

Here is how the page is laid out, so you can jump to what you need:

1. A short walkthrough that turns a CSV file into JSON objects, picks a
   field, keeps some rows, adds a computed field, and opens an array.
2. How a record holds fields, how to write a field path, and each JSON
   subcommand.
3. Reading and writing CSV.
4. Using a field in SORT, LOCATE, CHANGE, COLLATE, and LOOKUP, then a longer
   walkthrough of nested documents (library branches that each hold a
   `books` array).

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
- [`examples/objects.json`](../examples/objects.json): twelve book titles as consecutive JSON objects, not wrapped in an array

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
There are nine.

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
| OBJECTS | `json objects` | Read consecutive JSON objects (pretty-printed across records, adjacent in one record, or one object per line) and write each complete object as one record as soon as it closes. |

CSV is a separate stage:

```text
csv
csv strings
csv write
```

The rest of this page starts with a short walkthrough, then explains each
idea with a command you can run.

---

## A short walkthrough

If you are new, start here. We will read a tiny CSV file, look at the JSON
objects it becomes, pick a field, keep some rows, add a computed field, and
then open an array.

### 1. A tiny CSV file

`examples\orders.csv` is three sales rows with a customer, the price of one
item (amount), a region, and a quantity. We start with this file because the
header already names the fields, so you can see those names become JSON keys.

```text
customer,amount,region,qty
Ann,120,East,3
Ben,80,West,1
Cam,200,East,2
```

The first row names the fields. Later rows are the data.

### 2. See the JSON rows

`csv` reads that header, then writes one JSON object per data row. This is the
shape the rest of the pipe works with. Run this to see those objects:

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

The next command asks SPEC to build a plain text line from three fields. After
SPEC, you are back to ordinary pipeline rows:

```text
pipe "< examples\orders.csv | csv | spec $customer 1 /,/ next $amount next /,/ next $region next | console"
```

```text
Ann,120,East
Ben,80,West
Cam,200,East
```

SPEC can also copy the whole record (`1-*` means from the first column to
the last). In that case the JSON stays JSON.

### 3. Pick a field

To name a field, write a dollar sign followed by the field name. So
`$customer` is the value of the customer field. Here we ask SPEC to copy that
field onto a new record, so we can see the three customer values on their own:

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

PICK can compare a field. We want the East rows only, then a short line with
customer and amount. That is a common pattern: keep the rows you care about,
then print a few fields.

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
inserts a new field on each object. Here we compute `total` as quantity times
that price, so later stages can filter or print the line total:

```text
pipe "< examples\orders.csv | csv | json add $total = $qty * $amount | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3,"total":360}
{"customer":"Ben","amount":80,"region":"West","qty":1,"total":80}
{"customer":"Cam","amount":200,"region":"East","qty":2,"total":400}
```

Notice each row has a `total` field after ADD. PICK keeps the large ones, then
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
array. We want one record per book, so `$title` works the same way
`$customer` did on the CSV rows. `json split` writes one record per element:

```text
pipe "< examples\library.json | json split $books | spec $title 1 | console"
```

```text
Dune
Emma
```

Notice the array became two ordinary object records. [JSON SPLIT](#split),
[JSON ARRAY](#array), and [JSON OBJECTS](#objects) cover the details.

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
text, or invalid JSON is an error. The next command reads
`examples\notes.txt`, which is two ordinary lines, and asks JSON GET for a
field. That fails because the lines are not objects:

```text
pipe "< examples\notes.txt | json get $city | hole"
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get $city"
```

SPEC and PICK say the same thing with their own stage name. This command
asks SPEC for `$city` on those same plain text lines:

```text
pipe "< examples\notes.txt | spec $city 1 | hole"
```

```text
PIPE: SPEC: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "spec $city 1"
```

A top-level JSON array is not an object record. Use `json array` to turn it
into objects first. `examples\scores.json` is `[90, 85, 72]`. The next
command asks `json get` for `$value` on that array, and it fails the same way:

```text
pipe "< examples\scores.json | json get $value | hole"
```

```text
PIPE: JSON: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "json get $value"
```

Invalid JSON is that same object error, not a parser dump.
`examples\broken.json` looks like an object but the city value has no quotes.
This command shows that the stage still reports the object error:

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

The next command reads the sample library and asks SPEC for the title of the
first book. That path is `$books.0.title`:

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

The next command uses the bracket form to ask for the second book (index 1):

```text
pipe "< examples\library.json | spec $books[1].title 1 | console"
```

```text
Emma
```

Notice index `1` is the second book.

This command uses `-1` to ask for the last book, which is also Emma in this
sample:

```text
pipe "< examples\library.json | spec $books.-1.title 1 | console"
```

```text
Emma
```

Notice `-1` is the last book.

A whole object or array prints as compact JSON. Here we ask SPEC for `$books`
so you can see the array as one piece of text:

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

`examples\aisles.json` stores aisles as an object with keys `"0"` and `"1"`,
not as an array. The next two commands show that both spellings select those
keys:

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
there. We do this so you can see that SPEC still writes the record and leaves
the missing field empty:

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
has `city` and a null `note`. This command prints both, so you can see the
null become empty after the colon:

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

PICK is stricter. A missing field never matches, even not-equal. The sample
CSV has no `price` field, so both of these PICK commands write nothing:

```text
pipe "< examples\orders.csv | csv | pick $price > /0/ | console"
```

(no records)

```text
pipe "< examples\orders.csv | csv | pick $price ^= /0/ | console"
```

(no records: a missing field never matches, even not-equal)

JSON ADD needs every field in the expression. This command asks for `$price`,
which the sample file does not have:

```text
pipe "< examples\orders.csv | csv | json add $total = $qty * $price | hole"
```

```text
PIPE: JSON: field $price is missing
... Issued from stage 3 of pipeline 1
... Running "json add $total = $qty * $price"
```

Notice the error names `$price`. The sample file has no `price` field.

In a pipeline specification, `/` starts a delimited string, so `/=` is a
poor choice next to `/0/`. Write `!=` for numeric not-equal and `!==` for
string not-equal. `^=` and `¬=` still work.

These two commands keep everyone except Ben, once by region and once by
amount:

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

If you write `!=` on a text field, PICK stops and names the string operator.
The sample CSV has `East` in `$region`, so this command shows that message:

```text
pipe "< examples\orders.csv | csv | pick $region != /East/ | hole"
```

```text
PIPE: PICK: != compares numbers, but $region holds the text East. Use !== to compare text.
... Issued from stage 3 of pipeline 1
... Running "pick $region != /East/"
```

Notice it tells you to use `!==`. The same kind of message appears for `=`,
`<`, `>`, `^=`, and the other numeric operators, each naming its string pair
(`==`, `<<`, `>>`, `^==`, and so on).

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
| `json objects` | no field name | |
| `$qty` in an ADD / SET **expression** | required | `json add $shout = city + /!/` is `JSON: unexpected c in expression` |
| `$customer` in SPEC | required | `spec customer 1` is `SPEC: unexpected operand customer` |
| `$region` in PICK | required | `pick region == /East/` is `PICK: unexpected operand region` |

KEEP and COLLECT take a **top-level** name only (`$customer`, not `$customer.qty`).

The GET / SPLIT missing-field messages echo the operand you wrote.
`json get $price` says `JSON: field $price is missing`. `json get price`
says `JSON: field price is missing`.

These two commands do the same work. The first writes `$books` on the JSON
subcommand. The second leaves the `$` off. Both split the sample library:

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
The `$` on `$title` in SPEC is required. This command leaves it off, so SPEC
rejects `customer`:

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
`json add $user.city = /Oslo/` builds a nested `user` object. Run it to see
the new object sit beside the original fields:

```text
pipe "< examples\guest.json | json add $user.city = /Oslo/ | console"
```

```text
{"city":"Oslo","note":null,"user":{"city":"Oslo"}}
```

Non-object input to every subcommand except ARRAY and OBJECTS is
`JSON: record is not a JSON object`.

### ADD

We read one sales row from CSV, then insert `total`. ADD fails if that leaf
already exists. `take 1` keeps Ann's row so the output is one object:

```text
pipe "< examples\orders.csv | csv | take 1 | json add $total = $qty * $amount | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3,"total":360}
```

Notice `total` is a new numeric field on the same object.

The next command tries to ADD `$qty` even though `qty` is already there. ADD
refuses to overwrite it:

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
Here we add 4 to each quantity, then GET writes the new value as plain text:

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
start of the expression, so Command Prompt does not need extra quotes. This
command adds `note` on Ann's row:

```text
pipe "< examples\orders.csv | csv | take 1 | json set $note = /x/ | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3,"note":"x"}
```

Notice `note` was added beside the sales fields.

### DELETE

DELETE removes each named leaf. A missing leaf is a no-op. This command drops
`region` from Ann's row so you can see the other fields stay:

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

KEEP keeps only the top-level fields you list. This command keeps customer
and quantity on Ann's row:

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
missing field is an error. This command writes the library branch:

```text
pipe "< examples\library.json | json get $branch | console"
```

```text
Downtown
```

Notice you get the branch only, not the whole object.

The next command asks for `$price` on the sales rows. The sample file has no
`price` field, so GET reports it:

```text
pipe "< examples\orders.csv | csv | json get $price | hole"
```

```text
PIPE: JSON: field $price is missing
... Issued from stage 3 of pipeline 1
... Running "json get $price"
```

Notice the message names `$price`, matching the operand. Without the dollar
sign, the message has no dollar sign either:

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

This command splits the sample library and prints each book title:

```text
pipe "< examples\library.json | json split $books | spec $title 1 | console"
```

```text
Dune
Emma
```

The records themselves look like this. Each book is now its own object:

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
child does not carry the whole array. Run this to see `branch` sit beside
each book:

```text
pipe "< examples\library.json | json split $books keep | console"
```

```text
{"branch":"Downtown","title":"Dune","copies":3}
{"branch":"Downtown","title":"Emma","copies":1}
```

Notice `branch` from the parent sits beside `title` and `copies` from the
child.

The same records, printed as a short line:

```text
pipe "< examples\library.json | json split $books keep | spec $branch 1 / / next $title next | console"
```

```text
Downtown Dune
Downtown Emma
```

An empty array writes nothing for that input record. `examples\inbox.json`
has `messages: []`. This command splits that empty array:

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

One thing to know: COLLECT packs every remaining object into one array. It
does not make a separate array for each parent. If you need one array per
parent, pick that parent first, then collect. The longer walkthrough in
[Parents, children, and nested documents](#parents-children-and-nested-documents)
shows that pattern, and a way to work around it.

This command splits the sample library and collects the books back into one
array:

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
them and expands the objects. This command prints customer and city:

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
wrapped as `{"value": x}`. This command prints those numbers:

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

### OBJECTS

A file of consecutive JSON objects is several objects written one after
another. They are not wrapped in a top-level array, and they need not be
one object per line. One object may be pretty-printed across several
records from `<`. Two objects may sit in the same record with only
optional blanks between them, as in `{"title":"A"}{"title":"B"}`. JSON
Lines of objects (one object per line) is the same shape, one object
already complete in each record.

`JSON OBJECTS` reads that stream and writes each complete object as one
pipeline record as soon as the matching `}` is seen. Downstream `take 10`
can start work after ten objects. It does not wait for the rest of the
file. When the output closes, `json objects` stops and does not parse
what is left.

Only a JSON object is valid input. Surrounding blanks are ignored, as is
blank space between objects. A top-level array is
`JSON: input is not a JSON object. json objects reads consecutive JSON objects, not a top-level array. Use json array for an array document.`
A JSON number, string, `true`, `false`, `null`, or other non-object text
is `JSON: input is not a JSON object. json objects reads only JSON objects.`
Invalid JSON and an object that is still open at end of file are errors.
Trailing blanks after the last object are ignored. Each output record is
compact JSON, the same spelling the other JSON subcommands write.

`examples\objects.json` is twelve book titles in that consecutive-object
shape. This command prints every title:

```text
pipe "< examples\objects.json | json objects | spec $title 1 | console"
```

```text
Dune
Emma
Sula
Kindred
Beloved
Gilead
Neuromancer
Neverwhere
Circe
Hamnet
Piranesi
Lonesome Dove
```

Notice the file was not a JSON array, and each object became its own
record. The next command keeps the first ten titles, so later stages need
not wait for the last two objects:

```text
pipe "< examples\objects.json | json objects | take 10 | spec $title 1 | console"
```

```text
Dune
Emma
Sula
Kindred
Beloved
Gilead
Neuromancer
Neverwhere
Circe
Hamnet
```

MongoDB shell export is not JSON. Constructors such as `ObjectId(...)`,
`ISODate(...)`, `NumberLong(...)`, and `NumberInt(...)` are not converted.
If the input contains them, the stage stops and tells you to export JSON
or Extended JSON first. Extended JSON objects such as
`{"$oid":"507f1f77bcf86cd799439011"}` are ordinary JSON and pass through.

The sample catalog is a JSON array, so OBJECTS rejects it and points you
at ARRAY:

```text
pipe "< examples\catalog.json | json objects | hole"
```

```text
PIPE: JSON: input is not a JSON object. json objects reads consecutive JSON objects, not a top-level array. Use json array for an array document.
... Issued from stage 2 of pipeline 1
... Running "json objects"
```

---

## Expressions, strings, `/`, and SPEC separators

JSON ADD / SET expressions take JSON fields (`$qty`, `$books.0.title`), numbers
(`3`, `1.5`), and quoted strings. Operators: `+` `-` `*` `/` on numbers.
`+` concatenates when either side is a string. `|` or `||` also concatenates.
In a pipeline specification you must write `||` for a bar, because a single
`|` is the stage separator.

The reliable string form is a quoted string. `"..."` and `'...'` work, as do
the other pair delimiters `%` `#` `!` `;` `,`. In Command Prompt, a quoted
`"!"` inside the pipeline quotes is written as `""!""`. After `+`, `/!/` is
also a string and is kinder to type. This command adds an exclamation mark
to the first catalog city:

```text
pipe "< examples\catalog.json | json array | take 1 | json add $shout = $city + /!/ | json get $shout | console"
```

```text
Oslo!
```

Notice `$city` is the string `Oslo`, and `/!/` was concatenated.

`/` is division when it sits after a value, so `$amount / $qty` stays
arithmetic. This command computes Ann's unit price:

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
this piece. `next` does not; it appends.

This command uses `next`, so customer and amount sit together with no blank:

```text
pipe "< examples\orders.csv | csv | take 1 | spec $customer 1 $amount next | console"
```

```text
Ann120
```

Notice there is no blank. `next` glued the amount on.

This command uses `nextword`, so SPEC inserts the blank for you:

```text
pipe "< examples\orders.csv | csv | take 1 | spec $customer 1 $amount nextword | console"
```

```text
Ann 120
```

Notice `nextword` inserted the blank.

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
the walkthrough output above. This command reads the same sales file with
`STRINGS`:

```text
pipe "< examples\orders.csv | csv strings | console"
```

```text
{"customer":"Ann","amount":"120","region":"East","qty":"3"}
{"customer":"Ben","amount":"80","region":"West","qty":"1"}
{"customer":"Cam","amount":"200","region":"East","qty":"2"}
```

Notice `"120"` is a string after `STRINGS`. That changes `+`. On numbers, `+`
adds. On strings, `+` concatenates.

This command reads with `csv` (numbers) and adds each amount to itself:

```text
pipe "< examples\orders.csv | csv | json add $doubled = $amount + $amount | spec $customer 1 / / next $doubled next | console"
```

```text
Ann 240
Ben 160
Cam 400
```

This command reads with `csv strings` and uses the same `+`. The digits are
glued together:

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
`examples\contacts.csv` has zip `007` and `012`. This command prints those
zips:

```text
pipe "< examples\contacts.csv | csv | json get $zip | console"
```

```text
007
012
```

Notice `007` did not become the number 7. `STRINGS` is for the reader only:
`csv strings write` is `CSV: STRINGS applies to the reader, not WRITE`.

Quoted commas and doubled quotes are clearer in a file than on the
command line. `examples\quoted.csv` has a comma inside `note` and a quote
inside `msg`. This command prints both fields:

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

This command reads the sales file as objects, then writes CSV again:

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
files. Each item is a short job you might want to copy: what we are doing,
the command, the output, and what to notice.

### 1. Look at CSV as JSON objects

We want to see the sales file as the JSON objects later stages will use.

```text
pipe "< examples\orders.csv | csv | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":3}
{"customer":"Ben","amount":80,"region":"West","qty":1}
{"customer":"Cam","amount":200,"region":"East","qty":2}
```

Notice each data row became one object, and `amount` and `qty` are numbers.

### 2. Filter CSV rows

We want the East sales, printed as a short line.

```text
pipe "< examples\orders.csv | csv | pick $region == /East/ | spec $customer 1 / / next $amount next | console"
```

```text
Ann 120
Cam 200
```

Notice Ben (West) is gone. PICK compared `$region` as a string.

### 3. Compute a line total, then keep the large ones

We want quantity times price, then only the totals above 100.

```text
pipe "< examples\orders.csv | csv | json add $total = $qty * $amount | pick $total > /100/ | spec $customer 1 / spent / next $total next | console"
```

```text
Ann spent 360
Cam spent 400
```

Notice Ben's total is 80, so PICK dropped that row.

### 4. Flatten library books with SPLIT KEEP

We want one line per book, with the branch still on the line. `KEEP` copies
the parent fields onto each child.

```text
pipe "< examples\library.json | json split $books keep | spec $branch 1 / / next $title next / x/ next $copies next | console"
```

```text
Downtown Dune x3
Downtown Emma x1
```

Notice `branch` from the parent sits beside each book.

### 5. Expand a whole-file JSON array

We want one object per element of the pretty-printed catalog.

```text
pipe "< examples\catalog.json | json array | spec $customer 1 / / next $city next | console"
```

```text
Ann Oslo
Cam Leeds
```

Notice ARRAY joined the pretty-printed file and wrote two object records.

### 6. Split consecutive JSON objects

We want each object from a file that is not a JSON array. The objects are
written one after another, some of them pretty-printed across lines.

```text
pipe "< examples\objects.json | json objects | spec $title 1 | console"
```

```text
Dune
Emma
Sula
Kindred
Beloved
Gilead
Neuromancer
Neverwhere
Circe
Hamnet
Piranesi
Lonesome Dove
```

Notice each object became its own record. `take 10` keeps the first ten
titles without waiting for the rest of the file:

```text
pipe "< examples\objects.json | json objects | take 10 | spec $title 1 | console"
```

```text
Dune
Emma
Sula
Kindred
Beloved
Gilead
Neuromancer
Neverwhere
Circe
Hamnet
```

### 7. Pull one field

We want the branch name, then the title of the first book.

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

Notice GET writes plain text. SPEC with `$books.0.title` walks into the array.

### 8. Round trip CSV to JSON to CSV

We want to read CSV, become objects, and write CSV again.

```text
pipe "< examples\orders.csv | csv | csv write | console"
```

```text
customer,amount,region,qty
Ann,120,East,3
Ben,80,West,1
Cam,200,East,2
```

Notice the header and the three data rows come back.

### 9. Split books, then collect them back

We want to open the `books` array and pack those objects into one field
again.

```text
pipe "< examples\library.json | json split $books | json collect $books | console"
```

```text
{"books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
```

Notice COLLECT writes one object whose `books` field is the array. It does
not restore fields that SPLIT dropped, such as `branch`.

### 10. Join children back to parents on `$branch`

We want each book next to the branch it came from. FANOUT sends a copy of
each row down two paths: one path keeps the branches, the other opens the
books. Sort both paths on `$branch`, match them with COLLATE, then print
branch and city. The walkthrough later on this page explains FANOUT,
`(ENDCHAR ?)`, and the `f:` and `c:` names in this command.

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

Notice Campus has three lines (the parent plus two books), Downtown the same,
and Harbor two lines (the parent plus one book).
[Parents, children, and nested documents](#parents-children-and-nested-documents)
walks through the same wiring and the LOOKUP / COLLECT variants.

---

## `$field` in SORT, LOCATE, CHANGE, COLLATE, and LOOKUP

These stages can look at a named field instead of a slice of the record
by column numbers. `$amount` is the value of the amount field. If that
field is missing, the value is still empty, and the stage **warns**. The
warning names the stage, its position, the field you wrote, and the
fields the record does have, for example
`LOOKUP (stage 3): field $owner_i not found in input record; it has owner_id, name, ...`.
The pipe does not fail for that reason: a missing field has always been
empty, and pipes that match on an optional field still succeed. JSON GET
is the exception; a missing field there is still an error. If the record
is not a JSON object, the stage stops with `record is not a JSON object`
and its own name in front, the same wording SPEC and PICK use.

The value they compare is the text SPEC would write for that field. A
JSON number `3` and a JSON string `"3"` both become the characters `3`.
The comparison then uses ordinary byte order, the same way a column
range is compared.

### SORT

`sort $region` orders records by the region field. The comparison uses
the text SPEC would write for that field, character by character.
Several fields are compared left to right.

This command sorts the sales rows by region, then by customer, and prints
both fields:

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
whose field is not empty. NLOCATE keeps the rows LOCATE would drop.

This command keeps the West row:

```text
pipe "< examples\orders.csv | csv | locate $region /West/ | spec $customer 1 | console"
```

```text
Ben
```

This command keeps the rows that are not West:

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
If nothing matched, the record is unchanged, including whether a field is
a number or a string.

This command rewrites Ann's region:

```text
pipe "< examples\orders.csv | csv | take 1 | change $region /East/North/ | console"
```

```text
{"customer":"Ann","amount":120,"region":"North","qty":3}
```

A number becomes a string if CHANGE rewrites it. Ann's quantity is 3. This
command replaces that `3` with `9`:

```text
pipe "< examples\orders.csv | csv | take 1 | change $qty /3/9/ | console"
```

```text
{"customer":"Ann","amount":120,"region":"East","qty":"9"}
```

Notice `"9"` has quotes. Use `json set` when you want a JSON number.

### COLLATE and LOOKUP

The next section joins library branches to their books. COLLATE and LOOKUP
are the two stages that do that join. Both match rows that share a key
field. In our example that field is `$branch`.

`collate $branch` walks two streams that are already in branch order. One
stream is the parents (the branches). The other is the children (the
books). When a branch and a book share the same `$branch`, COLLATE writes
the branch, then each matching book. Because it walks the two streams
together, both sides need to be lined up on `$branch` first. `sort
$branch` on each path does that lining up.

`lookup $branch` does the same matching without that lining-up step.
LOOKUP first reads every parent into memory. Then, as each book arrives,
it finds the branch with the same `$branch` and writes the book followed
by that branch. The books do not need to be in order, and neither do the
branches.

When both sides use the same field name, write that name once. When the
names differ, write two. Imagine the books kept the branch they came from
in `$home`, while the parent rows still used `$branch`. `collate $branch
$home` reads `$branch` on the parent side and `$home` on the child side.
`lookup $home $branch` is the other way around: `$home` on the books
(LOOKUP matches each book as it arrives), then `$branch` on the parent
list (the list LOOKUP reads first).

If you are matching plain text lines by column position instead of a JSON
field, you can still write a column range as the key. For JSON objects, a
field such as `$branch` is the usual spelling.

A missing field is an empty key, the same way SPEC writes nothing for a
field that is not there. The stage also warns, so a typo such as
`$owner_i` for `$owner_id` is visible on LOOKUP instead of only later.
Suppose a book row had no `$branch`. That book would not match Downtown
or Harbor. Two rows that both lack `$branch` would match each other, as
if they shared a blank key — and LOOKUP or COLLATE would warn that
`$branch` was not on the record.

These stages need each record to be a JSON object, so they can read the
field. A plain text line is not an object. `examples\notes.txt` is two
ordinary lines, not JSON. If you ask SORT for `$city` on that file, SORT
stops with `record is not a JSON object`. That is the same wording SPEC
uses, with SORT's name in front. COLLATE and LOOKUP stop the same way.
Turn CSV into objects with `csv` first, or start from a JSON sample file.

This command shows the SORT error:

```text
pipe "< examples\notes.txt | sort $city | hole"
```

```text
PIPE: SORT: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "sort $city"
```

This command shows the same error from COLLATE. The notes enter on one
side and one library row on the other. `(ENDCHAR ?)` puts two pipelines
in one command. `c:` is a name for the COLLATE stage, so both sides can
feed it:

```text
pipe "(ENDCHAR ?) < examples\notes.txt | c: collate $city | hole ? < examples\libs.jsonl | take 1 | c:"
```

```text
PIPE: COLLATE: record is not a JSON object
... Issued from stage 2 of pipeline 1
... Running "collate $city"
```

The next section walks through nested documents: open the library
branches, send parents and children down different streams, join them
again on `$branch`, and pack books back into an array.

---

## Parents, children, and nested documents

`examples\library.json` is one branch. `examples\libs.jsonl` is three
branches, one JSON object per line. Each row has a `books` array:

```text
{"branch":"Downtown","city":"Oslo","books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
{"branch":"Harbor","city":"Bergen","books":[{"title":"Sula","copies":2}]}
{"branch":"Campus","city":"Trondheim","books":[{"title":"Dune","copies":1},{"title":"Kindred","copies":4}]}
```

You can open that nested data, work on the books as their own rows, and later
join them back to the branch they came from. COLLATE and LOOKUP both match
records on `$branch`. The rest of this section uses these library branches
as the parents and the books as the children.

Here are the stages you will meet in this walkthrough, in order. Each one
is described with our library file: the branches are the parents, and the
books are the children.

- **FANOUT** takes each library row and makes a copy of it. One copy goes
  down the path that keeps the branches. The other copy goes down the path
  that opens the books. That way we read the file once and still get both
  the parents and the children.
- **`json delete $books`** starts with a full branch row, such as Downtown
  with its books array still inside. It removes that array and leaves the
  rest of the row. What you get is the parent: `branch` and `city`, with
  no books attached.
- **`json split $books keep`** looks at the books array on a branch and
  writes one row per book. It also copies the branch's other fields onto
  each of those rows, so Downtown Dune still knows it is Downtown. The
  array itself is dropped, so Emma's row does not also carry Dune.
- **`sort $branch`** lines the rows up by branch name. Campus comes first,
  then Downtown, then Harbor. COLLATE needs both the parents and the
  children in that same order before it can match them.
- **COLLATE** walks the sorted branches and the sorted books together.
  When they share the same `$branch`, it writes the branch, then each
  matching book. A branch with no matching book can leave on a different
  path, so you can see the ones that did not pair up.
- **LOOKUP** holds the list of branches in memory, then looks at each book
  as it arrives. For each book it finds the branch with the same `$branch`
  and writes the book followed by that branch. The books do not need to
  be in order, and neither do the branches.
- **`json collect $books`** reads the book rows that are still in the pipe
  and packs them into one array on one record. That array is the whole
  remaining stream, not one array per branch. If you want only Harbor's
  books, pick Harbor first, then collect.
- **ELASTIC** holds onto rows until the next stage is ready for them.
  Picture FANOUT sending two copies of Downtown at once: one copy is the
  parent, and the other will become the books. LOOKUP will not look at any
  books until every parent has arrived, and FANOUT will not finish
  Downtown until both copies have somewhere to go, so the two paths wait
  for each other. Put ELASTIC on the book path and those children have a
  waiting room, so nobody gets stuck.

### See the file

We start by printing `examples\libs.jsonl` as it sits on disk, so the later
commands have a picture to compare against:

```text
pipe "< examples\libs.jsonl | console"
```

```text
{"branch":"Downtown","city":"Oslo","books":[{"title":"Dune","copies":3},{"title":"Emma","copies":1}]}
{"branch":"Harbor","city":"Bergen","books":[{"title":"Sula","copies":2}]}
{"branch":"Campus","city":"Trondheim","books":[{"title":"Dune","copies":1},{"title":"Kindred","copies":4}]}
```

### Separate parents and children

`json delete $books` keeps the parent and drops the array. Run it on the
three branches to see the parent rows:

```text
pipe "< examples\libs.jsonl | json delete $books | console"
```

```text
{"branch":"Downtown","city":"Oslo"}
{"branch":"Harbor","city":"Bergen"}
{"branch":"Campus","city":"Trondheim"}
```

`json split $books keep` looks at the books array on a branch and writes
one row per book. It also copies the branch's other fields onto each of
those rows. Run it to see five book rows:

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

So far we have pulled the parents and the children apart in two separate
pipelines, which meant reading the file twice. Now we will do both from a
single read.

A stage called FANOUT makes a copy of every row it receives and sends one
copy down each of two paths. On the first path, `json delete $books`
strips out the array, which leaves the parent (the branch). On the second
path, `json split $books keep` turns each book into its own row and keeps
the branch fields on it, which gives us the children.

Two bits of punctuation make this work. The label `f:` in front of FANOUT
is a name for that stage, so we can refer to it again. The `?` marks the
end of the first path. After it we start the second path, and writing
`f:` at its start says continue from that same FANOUT.

A pipeline specification normally uses `|` to separate stages. To put more
than one path in the same command, we also need a mark that ends a path.
`(ENDCHAR ?)` (you can also write `(end ?)`) tells the command that `?`
is that mark. In Command Prompt the whole specification stays inside one
pair of double quotes.

COLLATE walks two sorted streams together and matches rows that share a
key. In this example the key is `$branch`. One stream is the parents (the
branches). The other is the children (the books). Those two roles are
often called the master stream and the detail stream: master means the
parent side, and detail means the child side. COLLATE reads the parents
on its first input (the primary) and the books on its second input (the
secondary). Both streams must already be in key order, so each path runs
`sort $branch` first.

The first `c:` is a name for the COLLATE stage, the same idea as `f:` for
FANOUT. That first `c:` is its parent input and the output we print. The
second `c:` (after `?`) is the same stage's child input. When a branch and
a book have the same `$branch`, COLLATE writes the parent, then each
matching child:

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

If only Emma remains on the child side, Campus and Harbor have no matching
book. Matching pairs still leave on COLLATE's first output (the primary
output). Parents with no matching book leave on COLLATE's second output
(the secondary output). This command prints the match (Downtown and Emma):

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json delete $books | sort $branch | c: collate $branch | console ? f: | json split $books keep | pick $title == /Emma/ | sort $branch | c:"
```

```text
{"branch":"Downtown","city":"Oslo"}
{"branch":"Downtown","city":"Oslo","title":"Emma","copies":1}
```

This command prints the unmatched parents. `hole` discards a stream. The
first `hole` throws away the match, so CONSOLE can show the second output.
The last `c: | hole` is COLLATE's third output, often called the tertiary.
Unmatched children would leave there. It is empty here, because Emma did
match Downtown:

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json delete $books | sort $branch | c: collate $branch | hole ? f: | json split $books keep | pick $title == /Emma/ | sort $branch | c: | console ? c: | hole"
```

```text
{"branch":"Campus","city":"Trondheim"}
{"branch":"Harbor","city":"Bergen"}
```

The first of those two commands prints the match. The second prints the
unmatched parents.

### LOOKUP when the streams are not sorted

LOOKUP matches each record on one stream against a list of records on
another stream. In our example, the list is the parent records (the
branches) and the records we are matching are the children (the books).
LOOKUP first reads every parent into memory. Then, as each book arrives,
it finds the branch with the same `$branch` and writes the book followed
by that branch. The books do not need to be sorted, and neither do the
branches.

The command below reads the file twice: once for the books, which go on
the first input (the primary), and once for the branches, which go on the
second input (the secondary). The second input is the list LOOKUP reads
first. That is the other way around from COLLATE, which wanted parents on
the first input and books on the second.

The label `L:` is a name for the LOOKUP stage, the same idea as `c:` for
COLLATE. The first `L:` is the book input and the output we print. After
`?`, a second read of the file feeds the branches into the same LOOKUP
through the second `L:`:

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

Notice Downtown's parent is written twice, once per child. That is the
default: each book, then the branch it matched. `lookup $branch detail`
writes only the matching books, without repeating the branch.

FANOUT plus LOOKUP can leave both paths waiting. LOOKUP will not start
reading books until every parent has arrived (the second input must end
first). FANOUT wants to write both copies of the same row together, so it
will not finish the parent copy until the book copy has somewhere to go.
The two wait on each other. Reading the file twice avoids that, because
the two paths are not tied to the same row. If you do use FANOUT, put
`elastic` on the child path before LOOKUP so the books have a waiting
room. The `f:` and `L:` names work the same way as before. FANOUT sends
books down the first path and branches down the second:

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | f: fanout | json split $books keep | elastic | L: lookup $branch | console ? f: | json delete $books | L:"
```

That writes the same ten records as the two-read pipeline above.

### Pack children back into an array

`json collect $books` reads every remaining object and writes one record
`{"books":[...]}`. It packs the whole stream into one array. It does not
group by branch. If you need one array for one branch, pick that branch
first, then split, then collect.

Harbor has one book. This command keeps Harbor, opens its array, and packs
that one book:

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
pack them. Here we drop copies of 1 (Downtown Emma and Campus Dune). We
match the rest against the parent stream with LOOKUP, so only books from
real branches remain. Then we keep title and copies, and collect. This is
the same two-read LOOKUP wiring as above, with `lookup $branch detail` so
only the books go on to COLLECT:

```text
pipe "(ENDCHAR ?) < examples\libs.jsonl | json split $books keep | pick $copies > /1/ | L: lookup $branch detail | json keep $title $copies | json collect $books | console ? < examples\libs.jsonl | json delete $books | L:"
```

```text
{"books":[{"title":"Dune","copies":3},{"title":"Sula","copies":2},{"title":"Kindred","copies":4}]}
```

Notice that is one array for the whole stream, in child order, not three
nested branch records. If you collect the mixed stream from COLLATE
(parents and children together), the parents land inside the array too.
When the next stage is COLLECT, write `collate $branch detail` or
`lookup $branch detail` so only the books go through.

So the round trip that works is: open the nested documents, join on
`$branch`, and collect the children you kept. You get a books array back.
You do not get the original three nested branch objects back from COLLECT
alone. To rebuild one branch's array, pick that branch first, then collect.

---

## Not yet

A few things are still open, and a few behaviours are good to know.

**JSON ARRAY holds the whole document in memory.** JSON COLLECT does too.
There is no size cap. SPLIT does not; it walks one input record at a time.
`JSON OBJECTS` keeps the object that is still open, not the rest of the
file, and it stops when the output closes.

SUBSTRING, UNIQUE, and ZONE also read `$field` as the text SPEC would
write. If the record is not a JSON object, those stages treat the field
as empty instead of stopping. COLLATE, LOOKUP, and SORT still stop with
`record is not a JSON object`. A missing `$field` on an object record
is empty and produces a warning that names the stage, its position, the
field, and the keys the record does have. JSON GET still errors.

### Good to know

- A numeric path segment is an array index on an array and an object key on
  an object. `$aisles.0` selects the key `"0"`. `$books.0` still indexes the
  `books` array.
- `csv strings` keeps every field as a string. `+` concatenates. `*` `/` `-`
  on a string are an error (`read without STRINGS, or use a number`).
- PICK `/=` next to `/0/` does not parse as not-equal, because `/` starts a
  delimited string. Write `!=` (numeric) or `!==` (string), or `^=` / `¬=`.
- A missing `$field` on LOOKUP, SORT, COLLATE, SPEC, and the other
  `$field` stages is still empty. The stage warns once:
  `LOOKUP (stage 3): field $owner_i not found in input record; it has owner_id, name, ...`.
  JSON GET still errors when the field is missing.
- GET and SPLIT missing-field messages echo the operand you wrote (`price`
  or `$price`).
- SET / ADD do not grow arrays. An out-of-range index is an error.
- CHANGE `$field` stores a JSON string after a replacement.
- There is no `$["odd key"]`. Field names in `$` refs are ASCII identifiers.
  CSV headers that are not identifiers need a later syntax.
- REXX and Python still see bytes. There is no sidecar on the record.
- There is no JSON Lines file driver as its own stage. One object per line
  is already a record after `<`. Consecutive objects that span lines, or
  sit adjacent in one record, are split with `json objects`. A top-level
  array document still uses `<` plus `json array`.
- There is no stage that writes a top-level JSON array. COLLECT wraps the
  array in an object field so later `$field` stages still see an object.
- SPEC writes plain text when it builds a line from fields, so the JSON
  object is gone then. Copying the whole record (`1-*`) leaves the JSON
  as JSON.

The harvested developer catalog still has a short JSON / CSV section in
[`examples-reference.md`](examples-reference.md). This page is the one to
read and copy from.
