# Pipeline examples

`pipe "…"` lines with a `# →` expected list are harvested into
`tests/pipelines/readme-harvest.pipefix` and run by the fixture runner
(`cargo +stable test -p pipe-cli --test pipeline_fixtures`).

Run from the repo root so paths resolve:

```powershell
cargo run -p pipe-cli -- "literal hello | console"
# or, after a release build:
.\target\release\pipe.exe "literal hello | console"
```

Chained `literal a|literal b` writes in reverse order (`b` then `a`): each LITERAL
emits its argument and then copies its input (Author’s Edition 1.1.11 p.413).
A space before `|` is a trailing blank in LITERAL’s argument (p.412).
For several records, use `examples/emit.rexx`, `disk`, `<`, or `fanin` with parallel pipelines.

File stages take a portable host path (not CMS `fn ft fm`). `<` must be first
and reads; a missing file is an error. `>` replaces or creates; `>>` appends
or creates. Both writers copy records through. `disk` is diskfast: first stage
reads (missing file is empty); not-first appends. Invented `disk output` /
`disk append` are rejected — use `>` / `>>`.

A `delimitedString` may be `/abc/`, a hex literal (`x616263`), or a binary
literal (`b01100001`) (Author’s Edition 1.1.11 p.194).

```text
pipe "< examples/lines.txt | locate /todo/ | console"
pipe "literal hi | > /tmp/out.txt"
pipe "literal more | >> /tmp/out.txt"
pipe "literal abc| change x6162 x5858 | console"
# → XXc
```

---

## Multistream with labels

Selection stages (`locate`, `nlocate`, `take`, `drop`, …) write to **primary** and
**secondary** outputs. Use `(ENDCHAR ?)` and a stage label so a second pipeline can
read the secondary stream of the same stage instance:

```text
(ENDCHAR ?) … | L: locate /ap/ | console ? L: | console
#             ^ define L (primary out → console)
#                                           ^ same L, secondary out → console
```

- First `L:` defines the stage and wires its primary streams.
- Later `L:` (after `?`) attaches that stage’s secondary stream.
- Runnable demos: `powershell -File ./examples/multistream.ps1`

```text
pipe "(ENDCHAR ?) ./examples/emit.rexx apple banana apricot | L: locate /ap/ | console ? L: | ./examples/prefix.rexx miss= | console"
# primary  → apple / apricot
# secondary → miss=banana

pipe "(ENDCHAR ?) ./examples/emit.rexx ax b x | L: nlocate /x/ | console ? L: | ./examples/prefix.rexx hit= | console"
# primary  → b
# secondary → hit=ax / hit=x

pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take 2 | console ? T: | ./examples/prefix.rexx rest= | console"
# primary  → a / b
# secondary → rest=c / rest=d

pipe "(ENDCHAR ?) disk ./examples/lines.txt | L: locate /todo/ | console ? L: | ./examples/prefix.rexx other= | console"
# primary  → todo: one / todo: two
# secondary → other=… (every other line)
```

Avoid trailing `:` on ordinary words in these specs (`miss:` would be a new label).

---

## Phase 1 built-ins (TAKE / DROP / NLOCATE / STRIP)

IBM selection filters registered in the stage registry. Prefer these over the older
REXX prototypes (`examples/take.rexx`). Sample input: [`lines.txt`](lines.txt).
Runnable sweep: `powershell -File examples/phase1.ps1`.

### TAKE — keep from the start or end

Default is `FIRST 1`. Keywords and the count may appear in either order.
`*` selects all records; `0` selects none. Remainder goes to the secondary stream
(discarded when the secondary is not connected).

`TAKE FIRST` stops after *n* records. With no secondary stream the producer is
severed; attach the remainder to `hole` (or another stage) when you want it to
finish cleanly. `TAKE LAST` / `take *` read the whole file, so a secondary is optional.

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take | console ? T: | hole"
# → a

pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 | console ? T: | hole"
# → a / b

pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take first 3 | console ? T: | hole"
# → a / b / c

pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 first | console ? T: | hole"
# → a / b  (keyword after the count)

pipe "examples/emit.rexx a b c d | take last 2 | console"
# → c / d

pipe "examples/emit.rexx a b c d | take last * | console"
# → a / b / c / d

pipe "examples/emit.rexx a b c d | take * | console"
# → a / b / c / d

pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 0 | console ? T: | hole"
# → (no records)
```

Secondary stream (remainder) printed on the terminal:

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 | console ? T: | console"
# primary → a / b
# secondary → c / d
```

### DROP — discard from the start or end

Converse of TAKE. Dropped records go to the secondary stream; kept records stay
on the primary. Same operand shapes as TAKE.

```text
pipe "examples/emit.rexx a b c d | drop | console"
# → b / c / d  (default FIRST 1)

pipe "examples/emit.rexx a b c d | drop 2 | console"
# → c / d

pipe "examples/emit.rexx a b c d | drop first 1 | console"
# → b / c / d

pipe "examples/emit.rexx a b c d | drop last 2 | console"
# → a / b

pipe "examples/emit.rexx a b c d | drop * | console"
# → (no records)

pipe "examples/emit.rexx a b c d | drop 0 | console"
# → a / b / c / d
```

Secondary stream (discarded records):

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | D: drop 2 | console ? D: | console"
# primary → c / d
# secondary → a / b
```

### NLOCATE / NOTLOCATE — keep records that do *not* contain a string

Hits go to the secondary stream. No operand (a null string) selects only empty
records. `NOTLOCATE` is the documented synonym.

```text
pipe "examples/emit.rexx ax b x | nlocate /x/ | console"
# → b

pipe "disk examples/lines.txt | nlocate | console"
# → empty records only (null-string select)

pipe "examples/emit.rexx apple banana apricot | notlocate /ap/ | console"
# → banana

pipe "disk examples/lines.txt | nlocate /todo/ | console"
# → lines that do not contain "todo"
```

### STRIP — trim leading and/or trailing characters

Default is both ends, blank. Accepts `BOTH` / `LEADING` / `TRAILING`, `BLANK`,
a single character or delimited set, `ANYOF`, `NOT` / `TO`, `ANYCASE`, and an
optional per-side count.

```text
pipe "literal   aa   | strip | console"
# → aa

pipe "literal   aa   | strip leading | console"
# → aa␠␠

pipe "literal   aa   | strip trailing | console"
# → ␠␠aa

pipe "literal   aa   | strip both | console"
# → aa

pipe "literal   aa   | strip blank | console"
# → aa  (explicit blank target)

pipe "literal   aa   | strip 1 | console"
# → ␠aa␠  (at most one character per side)

pipe "literal xxyzz| strip anyof /xy/ | console"
# → zz

pipe "literal aaa| strip leading anyof /a/ 2 | console"
# → a

pipe "literal AaA| strip anycase anyof /a/ | console"
# → (empty)

pipe "literal a b| strip not | console"
# → ␠  (NOT blank: strip non-blanks from both ends)

pipe "literal a.b.| strip trailing . | console"
# → a.b
```

### SPLIT — break records into pieces at a target

Default is blank (space). Operand order is fixed (Author's Edition 1.1.11 p.505):
`ANYCASE`, `MINIMUM`, placement (`AT` / `BEFORE` / `AFTER`, with an optional signed
offset on the last two), `NOT`, then `BLANK` or a target and optional count.
A target is an xrange (`,` or hex `11` or `a-z`), `STRING` dstring, or `ANYOF`
dstring. Null input records are copied; generated empty pieces are dropped.

```text
pipe "literal a b c | split | console"
# → a / b / c

pipe "literal a,b,c| split , | console"
# → a / b / c

pipe "literal x::y::z| split string /::/ | console"
# → x / y / z

pipe "literal a,b;c:d| split anyof /,:;/ | console"
# → a / b / c / d

pipe "literal a b c| split before | console"
# → a /  b /  c

pipe "literal a,b,c| split after , | console"
# → a, / b, / c

pipe "literal a b c d e| split blank 2 | console"
# → a / b / c d e  (at most 2 splits)

pipe "literal AaBbCc| split anycase anyof /b/ | console"
# → Aa / Cc  (case-insensitive; empty piece dropped)

pipe "literal ,,| split , | console"
# → (no output; records that are only the target are discarded)
```

### JOIN — put records together

Default is to append one record to the first (pairs) with no insert string.
The first number is how many records are appended to the first of a set
(`join 2` builds groups of three). `*` joins everything. A delimited string
is inserted between joined records; `TERMINATE` also appends it after the
last record of a set. A second number is a maximum output length (it cannot
be the first word). `COUNT` prefixes a 10-character contributor count.
`KEYLENGTH n` joins while the leading *n* bytes match.

```text
pipe "literal a few words in a sentence | split | join | console"
# → afew / wordsin / asentence

pipe "literal a few words in a sentence | split | join 2 | console"
# → afewwords / inasentence

pipe "literal a few words in a sentence | split | join 2 /**/ | console"
# → a**few**words / in**a**sentence

pipe "literal a few words in a sentence | split | join * / / 20 | console"
# → a few words in a / sentence

pipe "literal a b c d e | split | join /*/ | console"
# → a*b / c*d / e

pipe "literal This is a short sentence. | split | join * / / 10 | console"
# → This is a / short / sentence.

pipe "literal a b c | split | join 0 | console"
# → a / b / c  (append none; each record is a set)

pipe "literal a b c | split | join /*/ terminate | console"
# → a*b* / c*

pipe "literal a b c | split | join count | console"
# → "         2ab" / "         1c"
```

### JOINCONT — join records marked with a continuation string

Default is TRAILING: a record is continued when it *ends* with the string (the
string is deleted unless `NOT` or `KEEP`). `LEADING` looks at the start of the
*next* record. `RANGE` inspects an `inputRange` and implies KEEP. The record
that does not continue is the last of the set. A second delimited string is
inserted between joined pieces. Operand order is fixed (Author's Edition 1.1.11
pp.401–403).

```text
pipe "literal one two\ three four | split | joincont /\/ | console"
# → one / twothree / four

pipe "literal This, is, the, first, sentence. | split | joincont /,/ / / | console"
# → This is the first sentence.

pipe "literal This, is, the, first, sentence. | split | joincont not /./ | console"
# → This,is,the,first,sentence.

pipe "examples/emit.rexx a1 a2 x3 b1 x2 | joincont not range 1 /x/ | console"
# → a1a2x3 / b1x2   (Note 3: join until column 1 is x, inclusive)

pipe "examples/emit.rexx a, b, c. d | joincont anyof /,/ | console"
# → abc. / d  (ANYOF: last character is comma → continue; "c." ends the set)
```

`LEADING / / KEEP` folds a following record that begins with a blank into the
previous one and keeps that blank (p.403 RFC 822 header sample). Build those
records from a file rather than `emit.rexx` (`PARSE ARG` strips leading blanks).

### CHOP — truncate a record after a column or at a target

Default is after column 80 (records are not padded). A lone number is that
column (`chop 8`, `chop 0` → a null record). Target form (Author's Edition
1.1.11 pp.289–290) scans for an xrange, `STRING` dstring, or `ANYOF` dstring
and truncates **before** the match unless `AFTER` is given. The prefix goes to
the primary output; the remainder goes to the secondary (discarded if that
stream is not connected). `TRUNCATE` is the documented synonym.

`examples/chop.rexx` is a REXX `SUBSTR`/`LENGTH` sample (`6:bcd`); it is **not**
the IBM CHOP built-in and is kept as a REXX exercise.

```text
pipe "literal abcdefghij | chop 4 | console"
# → abcd

pipe "literal abcaabccabcaa | chop str /ccab/ | console"
# → abcaab

pipe "literal abcaabccabcaa | chop any /c / | console"
# → ab

pipe "literal one two three | chop blank | console"
# → one

pipe "literal hello | chop after string /ll/ | console"
# → hell

pipe "(ENDCHAR ?) literal abcdef| C: chop 3 | console ? C: | console"
# primary → abc
# secondary → def
```

### UNIQUE — discard or retain adjacent duplicate records

Default is `LAST` of each adjacent run (the whole record is the key).
Records that are not selected go to the secondary stream. Operand order is
fixed (Author's Edition 1.1.11 pp.572–574): `COUNT`, `NOPAD` / `PAD xorc`,
`ANYCASE`, an `inputRange` or a parenthesized list, then `LAST` / `FIRST` /
`SINGLES` / `MULTIPLE` / `PAIRWISE`. COUNT and the selection keyword may
also appear before the range. UNIQUE compares only adjacent records; sort
first when duplicates may be separated.

```text
pipe "literal a a a b b b a a a c c c | split | unique | console"
# → a / b / a / c   (Figure 116 p.56; default LAST)

pipe "literal a a a b b b a a a c c c | split | unique first | console"
# → a / b / a / c

pipe "examples/emit.rexx aa1 aa2 bb | unique 1.2 last | console"
# → aa2 / bb

pipe "examples/emit.rexx a a b c c | unique singles | console"
# → b

pipe "examples/emit.rexx a a b c c | unique multiple | console"
# → a / a / c / c

pipe "examples/emit.rexx a b a a c d | unique pairwise | console"
# → a / b / c / d   (equal pair a,a discarded)

pipe "examples/emit.rexx a a a b | unique count | console"
# → "         3a" / "         1b"

pipe "examples/emit.rexx Foo foo BAR | unique anycase | console"
# → foo / BAR

pipe "(ENDCHAR ?) examples/emit.rexx aa1 aa2 bb | U: unique 1.2 first | console ? U: | console"
# primary → aa1 / bb
# secondary → aa2
```

### DUPLICATE — write extra copies of each record

Default additional count is 1 (each record is written twice). The number is
how many *extra* copies to write and must be −1 or larger. `*` copies only
the first input record until the consumer stops (Author's Edition 1.1.11
p.334). Always bound `dup *` with `take` (or another stage that severs the
output). Verb abbreviation `dup` (min 3).

```text
pipe "literal a| duplicate | console"
# → a / a

pipe "examples/emit.rexx a b | duplicate 2 | console"
# → a / a / a / b / b / b

pipe "examples/emit.rexx a b c | duplicate 0 | console"
# → a / b / c

pipe "examples/emit.rexx a b c | duplicate -1 | console"
# → (no records; input is consumed)

pipe "literal +60| duplicate * | take 3 | console"
# → +60 / +60 / +60   (Figure 245 p.148; take stops the supply)

pipe "literal +60| dup * | take 2 | console"
# → +60 / +60   (printed `dup *` spelling)
```

### PICK — select records that satisfy a relation

Compare a field (or a literal) with another field or a delimited string.
String operators are the REXX strict set (`==` `<<` `>>` …); numeric operators
(`=` `<` `>` …) parse the FPnumber railroad. A numeric *constant* must be a
dstring (`/37.5/`); a token such as `37.5` is columns 37–41. Records that
match go to the primary output; others go to the secondary. Operand order is
fixed (Author's Edition 1.1.11 pp.454–456): `NOPAD` / `PAD xorc`, `ANYCASE`,
then left side, operator, right side. Interval selection is a cascade of
`pick` stages (no `AND`/`OR`). Verb is spelled in full (`pic` is not an
abbreviation).

```text
pipe "examples/emit.rexx 0759 0800 1600 | pick 1.4 << /0800/ | console"
# → 0759

pipe "examples/emit.rexx 0759 0800 1200 1600 | pick 1.4 >>= /0800/ | pick 1.4 <<= /1559/ | console"
# → 0800 / 1200

pipe "literal a b c b| pick word 2 == word 4 | console"
# → a b c b

pipe "literal only| pick word 2 == word 4 | console"
# → only   (missing words are empty and compare equal)

pipe "literal n 40| pick word 2 > /37.5/ | console"
# → n 40

pipe "examples/emit.rexx Foo foo x | pick anycase 1-* == /FOO/ | console"
# → Foo / foo

pipe "(ENDCHAR ?) examples/emit.rexx aa ab | P: pick 1-* == /aa/ | console ? P: | console"
# primary → aa
# secondary → ab
```

Quoted dstrings work as in the book (`pick 1.4 << "0800"`). Hex/binary
dstrings are raw bytes: use `x30383030` for ASCII `0800` (the book’s
`xf1f2f3` example is EBCDIC).

### ZONE — run a selection stage on a column subset

Extract an `inputRange` from each record, optionally uppercase (`CASEI`)
and/or reverse those bytes, and run a stage on the extract. The original
record is written to the same output stream that stage used (Author's
Edition 1.1.11 pp.610–611). The book assumes a selection stage. Operand
order is fixed: `inputRange`, `CASEI`, `REVERSE`, then the program name
and its operands. The verb is spelled in full (`zon` is not an
abbreviation). Unregistered verbs (`find`, `all`, …) are unknown stages.

```text
pipe "examples/emit.rexx xxon middle xxoff after | zone -3;-1 reverse between /no/ /ffo/ | console"
# → xxon / middle / xxoff
# (p.610: records between those that end in “on” and “off”)

pipe "literal xxxxxxxx PIPERM rest| zone 9.8 locate / PIPERM / | console"
# → xxxxxxxx PIPERM rest
# (Figure 140 p.67; FIND is not registered, LOCATE on the zone is the portable equivalent)

pipe "literal abCxxx| zone 1-3 casei locate /abc/ | console"
# → abCxxx

pipe "literal aa target zz| zone word 2 locate /target/ | console"
# → aa target zz

pipe "(ENDCHAR ?) examples/emit.rexx ABCzzz XYZzzz | Z: zone 1-3 locate /ABC/ | console ? Z: | console"
# primary → ABCzzz
# secondary → XYZzzz
```

Hex/binary dstrings in the inner stage are raw bytes: use `x4142` for
ASCII `AB` (the book’s `xf1f2f3` example is EBCDIC).

### SPEC — rearrange fields, literals, and conversions

Phase 3 subset of IBM SPECS (Author's Edition 1.1.11 pp.496–502; tutorial
pp.153–162). Each item is an input range or literal, then an output column
(`NEXT` / `NEXTWORD` / `NEXTFIELD`, or a number). `WORD`/`FIELD` may be
written `w1`. Hex literals in the book that are EBCDIC (`x5c` asterisk,
C2X of "First") are shown here in portable ASCII (`x2a`, `c2x`).

```text
pipe "literal abc| spec 1-* 1 /*/ next | console"
# → abc*

pipe "literal abc| spec /Input: / 1 1-* next | console"
# → Input: abc

pipe "literal red green blue | spec word 2 1 | console"
# → green

pipe "literal Here is the first record | spec w2 1 w3 nextword w4 nextword | console"
# → is the first

pipe "literal a=b| spec fieldseparator = field 1 1 field 2 6 | console"
# → a    b

pipe "literal What a rum fellow. | spec substr 2 of word 3 1 | console"
# → u

pipe "literal First record | spec w1 1 x2a next w2 next | console"
# → First*record   (book Figure 266 uses EBCDIC x5c)

pipe "literal First record | spec pad 0 number 1.5 right w1 nextword | console"
# → 00001 First

pipe "literal AB| spec 1-* c2x 1 | console"
# → 4142
```

`READ` / `READSTOP` consume another input record mid-list; `WRITE` emits
the buffer so far and starts a new output record. Counters, `IF`/`THEN`,
`SELECT`, and `EOF` are not in this increment.

### PREFACE / APPEND — nested output before or after the input

Control stages (Author's Edition 1.1.11 pp.461–462 / pp.263–264). The
argument is a nested pipeline. Self-escape `|` as `||` so it is parsed
again inside the nested spec; or change the main separator with `(sep ?)`.
The nested beginning is unconnected; do not write `*:`.

```text
pipe "literal c d e| preface literal a b || split | console"
# → a / b / c d e

pipe "(sep ?) literal c d e? preface literal a b | split ? console"
# → a / b / c d e

pipe "literal c d e| append literal a b || split | console"
# → c d e / a / b

pipe "(sep ?) literal c d e? append literal a b | split ? console"
# → c d e / a / b

pipe "literal c d e| append literal *** End of data *** | console"
# → c d e / *** End of data ***
```

The verbs are spelled in full (`pref` / `app` are not abbreviations).

### LOOKUP — find records in a reference using a key field

Sorter (Author's Edition 1.1.11 pp.416–423; tutorial Figure 178 pp.87–88).
Details arrive on the primary; the secondary is the master file and is read
first. Default write is the detail followed by the matching master. Unmatched
details leave on the secondary; leftover masters leave on the tertiary at
end of file, sorted by key. Portable ASCII keys (book examples that print
EBCDIC hex are not used here). The verb is spelled in full (`looku` is not
an abbreviation). `AFTER` is not a LOOKUP keyword.

```text
pipe "(ENDCHAR ?) literal apple apricot banana| split | L: lookup | console ? literal apple banana cherry| split | L: | hole"
# → apple / apple / banana / banana

pipe "(ENDCHAR ?) literal apple apricot banana| split | L: lookup | hole ? literal apple banana cherry| split | L: | console"
# → apricot

pipe "(ENDCHAR ?) literal apple| L: lookup | hole ? literal apple cherry| split | L: | hole ? L: | console"
# → cherry

pipe "(ENDCHAR ?) literal a a b| split | L: lookup count | hole ? literal a c| split | L: | hole ? L: | console"
# → "         2a" / "         0c"

pipe "(ENDCHAR ?) literal aaaaa bbbbb aaaaa ccccc aaaaa| split | L: lookup 1.5 autoadd keyonly | hole ? L: | console"
# → aaaaa / bbbbb / ccccc

pipe "(ENDCHAR ?) literal the cat and the hat| split | L: lookup | hole ? literal the and of| split | L: | console"
# → cat / hat

pipe "(ENDCHAR ?) literal Foo| L: lookup anycase | console ? literal foo| L: | hole"
# → Foo / foo
```

### ELASTIC — buffer enough records to avoid a stall

Gateway (Author’s Edition 1.1.11 p.335; tutorial Ch.5 pp.88–89). No operands
(the diagram is all-caps; `elast` is not an abbreviation). With only a primary
input, ELASTIC copies records through and buffers just enough that a write
wait does not stop a further read. With a secondary input (feedback from the
primary-output path), it writes the primary file first, then the buffered
to-do list. Put ELASTIC on every FANIN input except the primary (p.338 note 2)
when two paths meet.

```text
pipe "literal a|literal b| elastic | console"
# → b / a

pipe "examples/emit.rexx a b c | elastic | console"
# → a / b / c

pipe "(ENDCHAR ?) examples/emit.rexx a b | f: fanout | i: fanin | console ? f: | elastic | i:"
# → a / b / a / b  (without elastic this mesh stalls)

pipe "(ENDCHAR ?) literal MASTER| e: elastic | console ? literal CHILD| e:"
# → MASTER / CHILD  (primary file, then the secondary to-do list)
```

The verb is spelled in full. A connected secondary output is an error.

### GATE — pass records until stopped

Gateway (Author’s Edition 1.1.11 pp.365–366). Optional operand `STRICT` (a
whole word; the diagram is all-caps, so `gat` is not an abbreviation).
Records on secondary and higher stream pairs are copied through until a
record arrives on the primary input. GATE then shorts primary in to primary
out and stops, so the other streams see end-of-file. The classic use is a
selection stage whose primary output is the stop signal:

```text
pipe "(ENDCHAR ?) literal a b abc c | split | L: locate /abc/ | G: gate | hole ? L: | G: | console"
# → a / b

pipe "(ENDCHAR ?) literal a b c | split | L: locate /zzz/ | G: gate | hole ? L: | G: | console"
# → a / b / c  (no primary record; copy until EOF)

pipe "literal a|literal b| gate | console"
# → b / a  (primary-only: short the primary file through)

pipe "literal a|literal b| gate strict | console"
# → b / a
```

Invented operands (`forever`, `stop`, `stric`) are rejected.

### IF — if-then / if-then-else around a selection stage

Gateway (Author’s Edition 1.1.11 pp.389–390). Syntax is `IF word [string]`
(the diagram is all-caps, so `i` is not an abbreviation). `word` is a
program (the book assumes a selection stage and does not enforce one);
further tokens are that program’s operands. IF adds that stage plus
FANINANY to the running set.

With one label reference (if-then), selected records go through the stages
between IF and the label; rejected records are merged with that path’s
output. With a second label reference (if-then-else), rejected records go
through the stages between the two labels, and both paths merge on the
tertiary stream. Two streams must be defined; a third is the else path.
The book’s `strfind` / `insert` / `xlate` examples use stages that are not
in this port (`STRFIND` is Phase 6); `locate` and `change` (null old
string prefixes, p.287) are the portable stand-ins.

```text
pipe "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | copy | if1: | console"
# → a / abc / b

pipe "(ENDCHAR ?) literal upA keep upB skip | split | if1: if locate /up/ | spec 3-* 1 | if1: | console"
# → A / keep / B / skip

pipe "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | change // /-> / | if1: | change // /   / | if1: | console"
# → "   a" / "-> abc" / "   b"

pipe "(ENDCHAR ?) literal a b c | split | if1: if locate /b/ | change // /X/ | if1: | console"
# → a / Xb / c
```

A missing nested verb, an unregistered verb (`then`, `strfind`, `nosuch`),
a primary-only pipe, and a fourth stream are rejected.

### DAM — pass records once primed

Gateway (Author’s Edition 1.1.11 pp.310–311). Converse of GATE: wait for a
record on the primary input, then short every stream pair so the held file
can flow. The diagram is a bare `DAM` (all-caps; `da` is not an abbreviation,
and GATE’s `STRICT` is not an operand). The book’s example uses FANOUT plus
ELASTIC to hold records on the secondary while LOCATE decides:

```text
pipe "(ENDCHAR ?) literal a b abc c | split | o: fanout | locate /abc/ | d: dam | hole ? o: | elastic | d: | console"
# → a / b / abc / c

pipe "(ENDCHAR ?) literal a b c | split | o: fanout | locate /zzz/ | d: dam | hole ? o: | elastic | d: | console"
# → (no records)

pipe "literal a|literal b| dam | console"
# → b / a

pipe "literal a | drop | dam | console"
# → (empty)

pipe "(ENDCHAR ?) literal a b c | split | f: fanout | d: dam | hole ? f: | d: | console"
# → a / b / c  (FANOUT feeds both pairs; no ELASTIC)
```

Invented operands (`forever`, `strict`, `stop`) are rejected.

### JUXTAPOSE — preface the last primary record with each secondary

Gateway (Author’s Edition 1.1.11 pp.403–405; tutorial pp.83–84). Optional
operand `COUNT` (a whole word; the diagram is all-caps, so `juxtapos` is
not an abbreviation). Each primary record is stored in a one-record buffer
and consumed. Each secondary record is appended to that buffer and the
combined record is written to the primary output. A primary that no
secondary matched is written to the secondary output. `COUNT` prefixes a
10-character right-aligned append-count (starting at 1) on the combined
primary record only. Use a common source (FANOUT) so the prefix is stored
before the matching secondary arrives (p.84).

```text
pipe "(ENDCHAR ?) literal a b | split | f: fanout | j: juxtapose | console ? f: | j:"
# → aa / bb

pipe "(ENDCHAR ?) literal P| f: fanout | j: juxtapose count | console ? f: | dup | j:"
# → "         1PP" / "         2PP"

pipe "(ENDCHAR ?) literal a b | split | j: juxtapose | hole ? j: | console"
# → a / b  (no secondary input: every primary is unused)
```

Invented operands (`strict`, `pad`, `coun`) are rejected.

### SYNCHRONISE — wait for a record on every input, then copy one from each

Arcane gateway (Author’s Edition 1.1.11 pp.536–539). No operands. The diagram
is `SYNChronise` (minimum abbreviation `sync`); `SYNChronize` is the American
spelling synonym. It peeks every connected input in numerical order, writes
one record to each corresponding output, then consumes the inputs. End-of-file
on any stream stops the cycle, so a short file paces a longer one:

```text
pipe "literal a|literal b| synchronise | console"
# → b / a

pipe "literal a|literal b| synchronize | console"
# → b / a

pipe "literal hi| sync | console"
# → hi

pipe "(ENDCHAR ?) literal a b c | split | S: synchronise | console ? literal x y z | split | S: | console"
# primary → a / b / c
# secondary → x / y / z

pipe "(ENDCHAR ?) literal a b c | split | S: synchronise | console ? literal x| S: | console"
# primary → a
# secondary → x  (secondary EOF stops; b and c are not copied)
```

Invented operands (`forever`, `strict`, `stop`) are rejected.

### PREDSELECT — route the original after a destructive test on copies

Gateway (Author’s Edition 1.1.11 pp.459–460; tutorial p.85 Figure 175;
FANOUT example p.341). No operands. The diagram is `PREDSELect` (minimum
abbreviation `predsel`). FANOUT writes one copy into PREDSELECT’s primary
input (stored in a one-record buffer) and another copy through a
transform and a selection stage. A record on PREDSELECT’s secondary input
writes the original to the primary output; a record on the tertiary input
writes it to the secondary output. The trigger records are discarded.
Specify `STOP ANYEOF` on FANOUT so end-of-file can propagate backwards
(p.85). The book’s `xlate` / `find` examples use stages that are not in
this port; `change` and `locate` are the portable stand-ins. A following
word is not a nested stage (unlike IF/ZONE) and is rejected.

```text
pipe "(ENDCHAR ?) literal a abc b | split | o: fanout stop anyeof | p: predselect | console ? o: | l: locate /abc/ | p: | hole ? l: | p:"
# → abc

pipe "(ENDCHAR ?) literal keep-secret|literal skip-me| o: fanout stop anyeof | p: predselect | console ? o: | change /secret/ /XXXXX/ | l: locate /XXXXX/ | p: | hole ? l: | p:"
# → keep-secret

pipe "(ENDCHAR ?) literal a abc b | split | o: fanout stop anyeof | p: predselect | console ? o: | l: locate /abc/ | p: | console ? l: | p:"
# primary → abc
# secondary → a / b

pipe "(ENDCHAR ?) literal xxapple|literal xxpear| o: fanout stop anyeof | p: predsel | console ? o: | spec 3-* 1 | l: locate /apple/ | p: | hole ? l: | p:"
# → xxapple
```

Invented operands (`strict`, `stop`, `locate`) are rejected.

### BETWEEN — select records between two targets

Selects records from the first match up to and including the second match
(or a count). Records outside the range go to the secondary stream. Supports
`ANYCASE` (before the operands) for case-insensitive matching. Requires two operands.

```text
pipe "(ENDCHAR ?) examples/emit.rexx a start b c end d | B: between /start/ /end/ | console ? B: | console"
# primary → start / b / c / end
# secondary → a / d

pipe "examples/emit.rexx a x 1 x b x 2 x c | between /x/ /x/ | console"
# → x / 1 / x / x / 2 / x  (repeated ranges with same pattern)

pipe "examples/emit.rexx a start b c d | between /start/ /end/ | console"
# → start / b / c / d  (range never closes, runs to EOF)

pipe "examples/emit.rexx a start b c d e | between /start/ 3 | console"
# → start / b / c  (numeric count: 3 records from the start match)

pipe "(ENDCHAR ?) examples/emit.rexx a START b END c | B: between anycase /start/ /end/ | console ? B: | console"
# primary → START / b / END
# secondary → a / c
```

### Cascade (Phase 1 exit criteria)

```text
pipe "disk examples/lines.txt | take 20 | nlocate /x/ | strip | console"
```

---

## REXX stage examples

These `.rexx` filters use IBM pipeline subcommands (`READTO`, `OUTPUT`, `CALLPIPE`, …)
and run through **Regina** when discoverable, otherwise the embedded **subset**
interpreter. Examples use IBM quoting (`'READTO LINE'`, `'OUTPUT' expr`) so
Regina does not substitute variables into the command string. The engine also
accepts `.rex`; examples prefer `.rexx`.

```powershell
cargo run -p pipe-cli -- "literal hello | examples/upper.rexx | console"
```

| Script | What it exercises |
|---|---|
| `emit.rexx` | Producer stage, `ARG.*`, no `READTO` |
| `upper.rexx` | `DO FOREVER`, `READTO` / `RC=12`, `TRANSLATE` |
| `prefix.rexx` | Stage args (`ARG.1`), `\|\|` concatenation |
| `filter.rexx` | `POS`, conditional pass-through |
| `fields.rexx` | `PARSE VAR`, rebuild with `\|\|` |
| `chop.rexx` | `SUBSTR`, `LENGTH`, `STRIP` (not IBM `chop`; use the built-in) |
| `take.rexx` | REXX prototype; prefer built-in `take` |
| `wrap.rexx` | Multi-step assign, `TRANSLATE`, `\|\|` |
| `tee_say.rexx` | `SAY` to stderr + pass-through |
| `callpipe_demo.rexx` | `CALLPIPE` with `*:` connectors |

### Stage arguments

Arguments after the script path become `ARG.1`, `ARG.2`, … (`ARG.0` is the count):

```text
pipe "literal hello| examples/prefix.rexx >>> | console"
# → >>>hello

pipe "examples/emit.rexx apple banana apricot | examples/filter.rexx ap | console"
# → apple / apricot
```
