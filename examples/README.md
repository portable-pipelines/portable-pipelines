# Pipeline examples

`pipe "…"` lines are harvested into
`tests/pipelines/readme-harvest.pipefix` and run by the fixture runner
(`cargo +stable test -p pipe-cli --test pipeline_fixtures`).
Expected console output is a `text` block under each command, one record
per line (a blank line is an empty record).

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
```

Output:
```text
todo: one
todo: two
```

```text
pipe "literal hi | > /tmp/out.txt"
```

Output: *(no records)*

```text
pipe "literal more | >> /tmp/out.txt"
```

Output: *(no records)*

```text
pipe "literal abc| change x6162 x5858 | console"
```

Output:
```text
XXc
```

---

## Multistream with labels

Selection stages (`locate`, `nlocate`, `find`, `nfind`, `take`, `drop`, …) write to **primary** and
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
```

Output:
```text
apple
apricot
miss=banana
```
Both streams write to the terminal.

```text
pipe "(ENDCHAR ?) ./examples/emit.rexx ax b x | L: nlocate /x/ | console ? L: | ./examples/prefix.rexx hit= | console"
```

Output:
```text
b
hit=ax
hit=x
```
Both streams write to the terminal.

```text
pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take 2 | console ? T: | ./examples/prefix.rexx rest= | console"
```

Output:
```text
a
b
rest=c
rest=d
```
Both streams write to the terminal.

```text
pipe "(ENDCHAR ?) disk ./examples/lines.txt | L: locate /todo/ | console ? L: | ./examples/prefix.rexx other= | console"
```

Output:
```text
other=apple
other=banana
other=apricot
other=  spaced  
other=xx
other=Xy
todo: one
other=a.b.
todo: two
other=done: three
other=
other=blank-above
```
Both streams write to the terminal.
Record 4 ends with 2 trailing blanks.

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
```

Output:
```text
a
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 | console ? T: | hole"
```

Output:
```text
a
b
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take first 3 | console ? T: | hole"
```

Output:
```text
a
b
c
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 first | console ? T: | hole"
```

Output:
```text
a
b
```
Keyword after the count.

```text
pipe "examples/emit.rexx a b c d | take last 2 | console"
```

Output:
```text
c
d
```

```text
pipe "examples/emit.rexx a b c d | take last * | console"
```

Output:
```text
a
b
c
d
```

```text
pipe "examples/emit.rexx a b c d | take * | console"
```

Output:
```text
a
b
c
d
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 0 | console ? T: | hole"
```

Output: *(no records)*

Secondary stream (remainder) printed on the terminal:

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 | console ? T: | console"
```

Output:
```text
a
b
c
d
```
Both streams write to the terminal.

### DROP — discard from the start or end

Converse of TAKE. Dropped records go to the secondary stream; kept records stay
on the primary. Same operand shapes as TAKE.

```text
pipe "examples/emit.rexx a b c d | drop | console"
```

Output:
```text
b
c
d
```
Default FIRST 1.

```text
pipe "examples/emit.rexx a b c d | drop 2 | console"
```

Output:
```text
c
d
```

```text
pipe "examples/emit.rexx a b c d | drop first 1 | console"
```

Output:
```text
b
c
d
```

```text
pipe "examples/emit.rexx a b c d | drop last 2 | console"
```

Output:
```text
a
b
```

```text
pipe "examples/emit.rexx a b c d | drop * | console"
```

Output: *(no records)*

```text
pipe "examples/emit.rexx a b c d | drop 0 | console"
```

Output:
```text
a
b
c
d
```

Secondary stream (discarded records):

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b c d | D: drop 2 | console ? D: | console"
```

Output:
```text
a
b
c
d
```
Both streams write to the terminal.

### NLOCATE / NOTLOCATE — keep records that do *not* contain a string

Hits go to the secondary stream. No operand (a null string) selects only empty
records. `NOTLOCATE` is the documented synonym.

```text
pipe "examples/emit.rexx ax b x | nlocate /x/ | console"
```

Output:
```text
b
```

```text
pipe "disk examples/lines.txt | nlocate | console"
```

Output:
```text

```
Null-string select.
The record is empty (a blank line).

```text
pipe "examples/emit.rexx apple banana apricot | notlocate /ap/ | console"
```

Output:
```text
banana
```

```text
pipe "disk examples/lines.txt | nlocate /todo/ | console"
```

Output:
```text
apple
banana
apricot
  spaced  
xx
Xy
a.b.
done: three

blank-above
```
Record 4 ends with 2 trailing blanks.
Record 9 is empty (a blank line).

### STRIP — trim leading and/or trailing characters

Default is both ends, blank. Accepts `BOTH` / `LEADING` / `TRAILING`, `BLANK`,
a single character or delimited set, `ANYOF`, `NOT` / `TO`, `ANYCASE`, and an
optional per-side count.

```text
pipe "literal   aa   | strip | console"
```

Output:
```text
aa
```

```text
pipe "literal   aa   | strip leading | console"
```

Output:
```text
aa   
```
The record ends with 3 trailing blanks.

```text
pipe "literal   aa   | strip trailing | console"
```

Output:
```text
  aa
```

```text
pipe "literal   aa   | strip both | console"
```

Output:
```text
aa
```

```text
pipe "literal   aa   | strip blank | console"
```

Output:
```text
aa
```
Explicit blank target.

```text
pipe "literal   aa   | strip 1 | console"
```

Output:
```text
 aa  
```
At most one character per side.
The record ends with 2 trailing blanks.

```text
pipe "literal xxyzz| strip anyof /xy/ | console"
```

Output:
```text
zz
```

```text
pipe "literal aaa| strip leading anyof /a/ 2 | console"
```

Output:
```text
a
```

```text
pipe "literal AaA| strip anycase anyof /a/ | console"
```

Output:
```text

```
The record is empty (a blank line).

```text
pipe "literal a b| strip not | console"
```

Output:
```text
 
```
NOT blank: strip non-blanks from both ends.
The record is one blank.

```text
pipe "literal a.b.| strip trailing . | console"
```

Output:
```text
a.b
```

### SPLIT — break records into pieces at a target

Default is blank (space). Operand order is fixed (Author's Edition 1.1.11 p.505):
`ANYCASE`, `MINIMUM`, placement (`AT` / `BEFORE` / `AFTER`, with an optional signed
offset on the last two), `NOT`, then `BLANK` or a target and optional count.
A target is an xrange (`,` or hex `11` or `a-z`), `STRING` dstring, or `ANYOF`
dstring. Null input records are copied; generated empty pieces are dropped.

```text
pipe "literal a b c | split | console"
```

Output:
```text
a
b
c
```

```text
pipe "literal a,b,c| split , | console"
```

Output:
```text
a
b
c
```

```text
pipe "literal x::y::z| split string /::/ | console"
```

Output:
```text
x
y
z
```

```text
pipe "literal a,b;c:d| split anyof /,:;/ | console"
```

Output:
```text
a
b
c
d
```

```text
pipe "literal a b c| split before | console"
```

Output:
```text
a
 b
 c
```

```text
pipe "literal a,b,c| split after , | console"
```

Output:
```text
a,
b,
c
```

```text
pipe "literal a b c d e| split blank 2 | console"
```

Output:
```text
a
b
c d e
```
At most 2 splits.

```text
pipe "literal AaBbCc| split anycase anyof /b/ | console"
```

Output:
```text
Aa
Cc
```
Case-insensitive; empty piece dropped.

```text
pipe "literal ,,| split , | console"
```

Output: *(no records)*
No output; records that are only the target are discarded.

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
```

Output:
```text
afew
wordsin
asentence
```

```text
pipe "literal a few words in a sentence | split | join 2 | console"
```

Output:
```text
afewwords
inasentence
```

```text
pipe "literal a few words in a sentence | split | join 2 /**/ | console"
```

Output:
```text
a**few**words
in**a**sentence
```

```text
pipe "literal a few words in a sentence | split | join * / / 20 | console"
```

Output:
```text
a few words in a
sentence
```

```text
pipe "literal a b c d e | split | join /*/ | console"
```

Output:
```text
a*b
c*d
e
```

```text
pipe "literal This is a short sentence. | split | join * / / 10 | console"
```

Output:
```text
This is a
short
sentence.
```

```text
pipe "literal a b c | split | join 0 | console"
```

Output:
```text
a
b
c
```
Append none; each record is a set.

```text
pipe "literal a b c | split | join /*/ terminate | console"
```

Output:
```text
a*b*
c*
```

```text
pipe "literal a b c | split | join count | console"
```

Output:
```text
         2ab
         1c
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
```

Output:
```text
one
twothree
four
```

```text
pipe "literal This, is, the, first, sentence. | split | joincont /,/ / / | console"
```

Output:
```text
This is the first sentence.
```

```text
pipe "literal This, is, the, first, sentence. | split | joincont not /./ | console"
```

Output:
```text
This,is,the,first,sentence.
```

```text
pipe "examples/emit.rexx a1 a2 x3 b1 x2 | joincont not range 1 /x/ | console"
```

Output:
```text
a1a2x3
b1x2
```
Note 3: join until column 1 is x, inclusive.

```text
pipe "examples/emit.rexx a, b, c. d | joincont anyof /,/ | console"
```

Output:
```text
abc.
d
```
ANYOF: last character is comma → continue; "c." ends the set.

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
```

Output:
```text
abcd
```

```text
pipe "literal abcaabccabcaa | chop str /ccab/ | console"
```

Output:
```text
abcaab
```

```text
pipe "literal abcaabccabcaa | chop any /c / | console"
```

Output:
```text
ab
```

```text
pipe "literal one two three | chop blank | console"
```

Output:
```text
one
```

```text
pipe "literal hello | chop after string /ll/ | console"
```

Output:
```text
hell
```

```text
pipe "(ENDCHAR ?) literal abcdef| C: chop 3 | console ? C: | console"
```

Output:
```text
def
abc
```
Both streams write to the terminal.

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
```

Output:
```text
a
b
a
c
```
Figure 116 p.56; default LAST.

```text
pipe "literal a a a b b b a a a c c c | split | unique first | console"
```

Output:
```text
a
b
a
c
```

```text
pipe "examples/emit.rexx aa1 aa2 bb | unique 1.2 last | console"
```

Output:
```text
aa2
bb
```

```text
pipe "examples/emit.rexx a a b c c | unique singles | console"
```

Output:
```text
b
```

```text
pipe "examples/emit.rexx a a b c c | unique multiple | console"
```

Output:
```text
a
a
c
c
```

```text
pipe "examples/emit.rexx a b a a c d | unique pairwise | console"
```

Output:
```text
a
b
c
d
```
Equal pair a,a discarded.

```text
pipe "examples/emit.rexx a a a b | unique count | console"
```

Output:
```text
         3a
         1b
```

```text
pipe "examples/emit.rexx Foo foo BAR | unique anycase | console"
```

Output:
```text
foo
BAR
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx aa1 aa2 bb | U: unique 1.2 first | console ? U: | console"
```

Output:
```text
aa2
aa1
bb
```
Both streams write to the terminal.

### DUPLICATE — write extra copies of each record

Default additional count is 1 (each record is written twice). The number is
how many *extra* copies to write and must be −1 or larger. `*` copies only
the first input record until the consumer stops (Author's Edition 1.1.11
p.334). Always bound `dup *` with `take` (or another stage that severs the
output). Verb abbreviation `dup` (min 3).

```text
pipe "literal a| duplicate | console"
```

Output:
```text
a
a
```

```text
pipe "examples/emit.rexx a b | duplicate 2 | console"
```

Output:
```text
a
a
a
b
b
b
```

```text
pipe "examples/emit.rexx a b c | duplicate 0 | console"
```

Output:
```text
a
b
c
```

```text
pipe "examples/emit.rexx a b c | duplicate -1 | console"
```

Output: *(no records)*
Input is consumed.

```text
pipe "literal +60| duplicate * | take 3 | console"
```

Output:
```text
+60
+60
+60
```
Figure 245 p.148; take stops the supply.

```text
pipe "literal +60| dup * | take 2 | console"
```

Output:
```text
+60
+60
```
Printed `dup *` spelling.

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
```

Output:
```text
0759
```

```text
pipe "examples/emit.rexx 0759 0800 1200 1600 | pick 1.4 >>= /0800/ | pick 1.4 <<= /1559/ | console"
```

Output:
```text
0800
1200
```

```text
pipe "literal a b c b| pick word 2 == word 4 | console"
```

Output:
```text
a b c b
```

```text
pipe "literal only| pick word 2 == word 4 | console"
```

Output:
```text
only
```
Missing words are empty and compare equal.

```text
pipe "literal n 40| pick word 2 > /37.5/ | console"
```

Output:
```text
n 40
```

```text
pipe "examples/emit.rexx Foo foo x | pick anycase 1-* == /FOO/ | console"
```

Output:
```text
Foo
foo
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx aa ab | P: pick 1-* == /aa/ | console ? P: | console"
```

Output:
```text
ab
aa
```
Both streams write to the terminal.

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
abbreviation). Unregistered verbs (`all`, `strliteral`, …) are unknown stages.

```text
pipe "examples/emit.rexx xxon middle xxoff after | zone -3;-1 reverse between /no/ /ffo/ | console"
```

Output:
```text
xxon
middle
xxoff
```
p.610: records between those that end in “on” and “off”.

```text
pipe "literal xxxxxxxx PIPERM rest| zone 9.8 find _PIPERM_ | console"
```

Output:
```text
xxxxxxxx PIPERM rest
```
Figure 140 p.67; FIND underscore is a required blank.

```text
pipe "literal abCxxx| zone 1-3 casei locate /abc/ | console"
```

Output:
```text
abCxxx
```

```text
pipe "literal aa target zz| zone word 2 locate /target/ | console"
```

Output:
```text
aa target zz
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx ABCzzz XYZzzz | Z: zone 1-3 locate /ABC/ | console ? Z: | console"
```

Output:
```text
XYZzzz
ABCzzz
```
Both streams write to the terminal.

Hex/binary dstrings in the inner stage are raw bytes: use `x4142` for
ASCII `AB` (the book’s `xf1f2f3` example is EBCDIC).

### CASEI — run a selection stage case-insensitively

Control (Author’s Edition 1.1.11 pp.285–286; tutorial p.122). Verb `CASEI`
is all-caps (`case` is not an abbreviation). Operand order is fixed:
`[ZONE inputRange] [REVERSE] word [string]`. CASEI uppercases the nested
stage’s argument and the data passed to it (portable ASCII A–Z), then
writes the **original** record to the stream that stage used. The book
assumes a selection stage; any registered `word` is accepted. Note 1:
built-in `ANYCASE` / `STRFIND` exist; CASEI still wraps stages that have
no case option. The book example `casei find :figref` is ASCII here.

```text
pipe "literal xyz|literal Abc|literal keep| casei locate /abc/ | console"
```

Output:
```text
Abc
```
Literal cascade is reverse order.

```text
pipe "literal :FigRef id| casei find :figref | console"
```

Output:
```text
:FigRef id
```
p.285; portable ASCII.

```text
pipe "literal abcZZ|literal xabc| casei find abc | console"
```

Output:
```text
abcZZ
```

```text
pipe "literal abCxxx| casei zone 1-3 locate /abc/ | console"
```

Output:
```text
abCxxx
```
Only columns 1–3 are searched.

```text
pipe "literal xxon| casei zone -3;-1 reverse locate /nox/ | console"
```

Output:
```text
xxon
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx Abc xyz | C: casei locate /abc/ | console ? C: | console"
```

Output:
```text
Abc
xyz
```
Both streams write to the terminal.

```text
pipe "literal a | casei | hole"
```

Error:
```text
PIPE: CASEI: a word (the name of the program to run) is required
```

```text
pipe "literal a | casei anycase locate /a/ | hole"
```

Error:
```text
PIPE: CASEI: unknown stage: anycase
```

Invented operands (`anycase`, `caseless`, `forever`) are the nested verb
if they appear first and are rejected when unregistered.

### SPEC — rearrange fields, literals, and conversions

Phase 3 subset of IBM SPECS (Author's Edition 1.1.11 pp.496–502; tutorial
pp.153–162). Each item is an input range or literal, then an output column
(`NEXT` / `NEXTWORD` / `NEXTFIELD`, or a number). `WORD`/`FIELD` may be
written `w1`. Hex literals in the book that are EBCDIC (`x5c` asterisk,
C2X of "First") are shown here in portable ASCII (`x2a`, `c2x`).

```text
pipe "literal abc| spec 1-* 1 /*/ next | console"
```

Output:
```text
abc*
```

```text
pipe "literal abc| spec /Input: / 1 1-* next | console"
```

Output:
```text
Input: abc
```

```text
pipe "literal red green blue | spec word 2 1 | console"
```

Output:
```text
green
```

```text
pipe "literal Here is the first record | spec w2 1 w3 nextword w4 nextword | console"
```

Output:
```text
is the first
```

```text
pipe "literal a=b| spec fieldseparator = field 1 1 field 2 6 | console"
```

Output:
```text
a    b
```

```text
pipe "literal What a rum fellow. | spec substr 2 of word 3 1 | console"
```

Output:
```text
u
```

```text
pipe "literal First record | spec w1 1 x2a next w2 next | console"
```

Output:
```text
First*record
```
Book Figure 266 uses EBCDIC x5c.

```text
pipe "literal First record | spec pad 0 number 1.5 right w1 nextword | console"
```

Output:
```text
00001 First
```

```text
pipe "literal AB| spec 1-* c2x 1 | console"
```

Output:
```text
4142
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
```

Output:
```text
a
b
c d e
```

```text
pipe "(sep ?) literal c d e? preface literal a b | split ? console"
```

Output:
```text
a
b
c d e
```

```text
pipe "literal c d e| append literal a b || split | console"
```

Output:
```text
c d e
a
b
```

```text
pipe "(sep ?) literal c d e? append literal a b | split ? console"
```

Output:
```text
c d e
a
b
```

```text
pipe "literal c d e| append literal *** End of data *** | console"
```

Output:
```text
c d e
*** End of data ***
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
```

Output:
```text
apple
apple
banana
banana
```

```text
pipe "(ENDCHAR ?) literal apple apricot banana| split | L: lookup | hole ? literal apple banana cherry| split | L: | console"
```

Output:
```text
apricot
```

```text
pipe "(ENDCHAR ?) literal apple| L: lookup | hole ? literal apple cherry| split | L: | hole ? L: | console"
```

Output:
```text
cherry
```

```text
pipe "(ENDCHAR ?) literal a a b| split | L: lookup count | hole ? literal a c| split | L: | hole ? L: | console"
```

Output:
```text
         2a
         0c
```

```text
pipe "(ENDCHAR ?) literal aaaaa bbbbb aaaaa ccccc aaaaa| split | L: lookup 1.5 autoadd keyonly | hole ? L: | console"
```

Output:
```text
aaaaa
bbbbb
ccccc
```

```text
pipe "(ENDCHAR ?) literal the cat and the hat| split | L: lookup | hole ? literal the and of| split | L: | console"
```

Output:
```text
cat
hat
```

```text
pipe "(ENDCHAR ?) literal Foo| L: lookup anycase | console ? literal foo| L: | hole"
```

Output:
```text
Foo
foo
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
```

Output:
```text
b
a
```

```text
pipe "examples/emit.rexx a b c | elastic | console"
```

Output:
```text
a
b
c
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b | f: fanout | i: fanin | console ? f: | elastic | i:"
```

Output:
```text
a
b
a
b
```
Without elastic this mesh stalls.

```text
pipe "(ENDCHAR ?) literal MASTER| e: elastic | console ? literal CHILD| e:"
```

Output:
```text
MASTER
CHILD
```
Primary file, then the secondary to-do list.

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
```

Output:
```text
a
b
```

```text
pipe "(ENDCHAR ?) literal a b c | split | L: locate /zzz/ | G: gate | hole ? L: | G: | console"
```

Output:
```text
a
b
c
```
No primary record; copy until EOF.

```text
pipe "literal a|literal b| gate | console"
```

Output:
```text
b
a
```
Primary-only: short the primary file through.

```text
pipe "literal a|literal b| gate strict | console"
```

Output:
```text
b
a
```

Invented operands (`forever`, `stop`, `stric`) are rejected.

### NOEOFBACK — pass records and ignore end-of-file on output

Filter (Author’s Edition 1.1.11 pp.440–441). Syntax is a bare `NOEOFBACK`
(the diagram is all-caps, so `noeof` is not an abbreviation). No operands.
Passes records while the output is connected; when the output is at
end-of-file it consumes the remaining input, so EOF travels forwards but
not backwards. `NOTEOFBACK` is the documented synonym. The book’s example
puts NOEOFBACK after GATE so a caller that stops reading cannot shut GATE
before the target arrives:

```text
pipe "literal a|literal b| noeofback | console"
```

Output:
```text
b
a
```

```text
pipe "examples/emit.rexx a b c | noeofback | console"
```

Output:
```text
a
b
c
```

```text
pipe "(ENDCHAR ?) literal a b abc c | split | L: locate /abc/ | G: gate ? L: | G: | noeofback | console"
```

Output:
```text
a
b
c
```
GATE primary unconnected; the match is not passed.

```text
pipe "(ENDCHAR ?) literal a b abc c | split | L: locate /abc/ | G: gate ? L: | G: | noeofback | take 1 | console"
```

Output:
```text
a
```
TAKE severs; NOEOFBACK still consumes the rest so GATE stays open.

```text
pipe "literal hello| noteofback | console"
```

Output:
```text
hello
```

Invented operands (`forever`, `strict`, `1`) are rejected.

### EOFBACK — copy through, then run an output driver so EOF can go backwards

Control (Author’s Edition 1.1.11 p.337; file-driver example p.665). Syntax is
`EOFBACK word [string]` (the diagram is all-caps, so `eof` is not an
abbreviation). `word` is the output device driver to run after each record
has been consumed on the primary output; further tokens are that driver’s
arguments. The nested driver’s own output is not connected.

Without EOFBACK, a device driver such as CONSOLE or `>` consumes the whole
input. With it, the driver sees only records the following stages consume,
so a partitioning stage (the book’s `tolabel`; here `take`) can stop the
producer as well as the driver.

```text
pipe "literal a|literal b| eofback hole | console"
```

Output:
```text
b
a
```

```text
pipe "examples/emit.rexx a b c | eofback hole | console"
```

Output:
```text
a
b
c
```

```text
pipe "literal a b c | split | eofback hole | take 1 | console"
```

Output:
```text
a
```
TAKE severs; remaining input is not consumed — EOF goes backwards.

```text
pipe "literal a b c | split | eofback > /tmp/eofback-out.txt | take 1 | console"
```

Output:
```text
a
```
Nested `>` writes only the consumed record; p.665.

```text
pipe "literal a | eofback | hole"
```

Error:
```text
PIPE: EOFBACK: a word (the name of the program to run) is required
```

Invented nested verbs (`forever`, `nosuch`, `1`) are rejected. `eofbac` is
not an abbreviation.

### AGGRC — aggregate numeric return codes

Filter (Author’s Edition 1.1.11 pp.258–259). Syntax is a bare `AGGRC`
(the diagram is all-caps, so `aggr` is not an abbreviation). No operands.
Reads records that each contain one signed 32-bit number, waits until
end-of-file, then writes a single record: the most negative number if any
input is negative, otherwise the largest. A leading plus is ignored; a
nonnegative result is unsigned (no plus sign). No output if there is no
input. When AGGRC is last, the aggregate becomes the PIPE return code
instead of a record (the p.259 `(listerr)` message is not implemented).

```text
pipe "literal 0 99 3 6 | split | aggrc | console"
```

Output:
```text
99
```

```text
pipe "literal 0 99 -3 6 | split | aggrc | console"
```

Output:
```text
-3
```

```text
pipe "literal +6 +0 | split | aggrc | console"
```

Output:
```text
6
```

```text
pipe "literal a | drop | aggrc | console"
```

Output: *(no records)*

```text
pipe "literal 6 | aggrc | console"
```

Output:
```text
6
```

```text
pipe "literal 6 | aggrc"
```

Output: *(no records)*
PIPE return code 6.

```text
pipe "literal 6 | aggrc strict | hole"
```

Error:
```text
PIPE: AGGRC: unrecognized operand strict
```

Invented operands (`strict`, `max`, `1`) are rejected. `aggr` is not an abbreviation.

### DELAY — wait until a time of day or an interval

Device driver (Author’s Edition 1.1.11 pp.321–322; tutorial pp.148–150).
Syntax is a bare `DELAY` (the diagram is all-caps, so `dela` is not an
abbreviation). No operands: each input record’s first blank-delimited word
is the time, and the whole record is copied when that time is reached.
A leading plus is an interval from the moment the record is read (`+60` is
sixty seconds, not sixty hours). Without a plus the word is local time
since midnight (`0` never waits; `24` is the next midnight). DELAY must not
be a first stage. The book’s once-a-minute example is
`literal +60 | dup * | delay`; tests use a zero or fractional-second
interval so the suite stays fast.

```text
pipe "literal +0| delay | console"
```

Output:
```text
+0
```

```text
pipe "literal 0| delay | console"
```

Output:
```text
0
```

```text
pipe "literal +0 hello| delay | console"
```

Output:
```text
+0 hello
```

```text
pipe "literal +0| delay forever | hole"
```

Error:
```text
PIPE: DELAY: unrecognized operand forever
```

```text
pipe "delay | hole"
```

Error:
```text
PIPE: DELAY must not be a first stage
```

```text
pipe "literal +0| dela | hole"
```

Error:
```text
PIPE: unknown stage: DELA
```

Invented operands (`forever`, `strict`, `+60` as a stage argument) are
rejected. The time belongs on the record.

### BEAT — heartbeat when records do not arrive

Device driver (Author’s Edition 1.1.11 pp.271–272). Syntax is
`BEAT [ONCE] number[.number] [delimitedString]` (the diagram is all-caps,
so `bea` is not an abbreviation). Copies primary input to primary output.
If no record arrives within the interval (seconds; up to six fractional
digits), writes the marker to the secondary output. The default marker is
a null record. Without `ONCE`, another timeout starts after that marker is
consumed; with `ONCE`, at most one marker is written per input record.
`ONCE` is a whole word and must precede the number. The book’s example
`… | b: beat 1 | … ? b:` defines the secondary stream without connecting
it, so the first timeout stops the stage. Tests use a short interval so
the suite stays fast; the README pipes below copy records that are already
waiting (no wait).

```text
pipe "(ENDCHAR ?) literal a|literal b| B: beat 10 | console ? B: | hole"
```

Output:
```text
b
a
```

```text
pipe "(ENDCHAR ?) literal hi| B: beat once 10 /tick/ | console ? B: | hole"
```

Output:
```text
hi
```

```text
pipe "(ENDCHAR ?) literal a|literal b| B: beat 10 | console ? B:"
```

Output:
```text
b
a
```

```text
pipe "literal a | beat | hole"
```

Error:
```text
PIPE: BEAT: an interval in seconds is required
```

```text
pipe "literal a | beat forever | hole"
```

Error:
```text
PIPE: BEAT: invalid interval forever
```

```text
pipe "literal a | beat 1 once | hole"
```

Error:
```text
PIPE: BEAT: unrecognized operand once
```

```text
pipe "literal a | beat 1 tick | hole"
```

Error:
```text
PIPE: BEAT: unrecognized operand tick
```

```text
pipe "literal a | bea 1 | hole"
```

Error:
```text
PIPE: unknown stage: BEA
```

Invented operands (`forever`, `strict`, a DELAY-style `+1` / `1:30`,
`ONCE` after the number, an undelimited marker) are rejected.

### UPDATE — apply a CMS-format UPDATE file

Gateway (Author’s Edition 1.1.11 pp.576–577; tutorial Figure 176 p.86).
Syntax is `UPDATE [range] [FIRST] [LAST] [* string]` (the diagram is
all-caps, so `updat` is not an abbreviation). The master file is on the
primary input; a CMS UPDATE stream (SEQ8 control cards `./ I`, `./ D`,
`./ R`, `./ S`, `./ *`) is on the secondary input. The updated master
goes to the primary output; the update log to the secondary output (pipe
it to `hole` to discard it). Both outputs must be connected.

`range` is the sequence-field columns (default 73-80; maximum width 15).
The samples below put the sequence in columns 1-8 so the records stay
short. `FIRST` checks that the input master is in sequence; `LAST` skips
output-sequence checking so the last update in a cascade can add lines
without numbers. A trailing `* string` names the update in log messages.

Portable tests use ASCII digits `0`–`9` in the sequence field. CMS card
images are EBCDIC with the sequence in columns 73-80; that encoding is
not reproduced here.

```text
pipe "(ENDCHAR ?) < examples/update-master.txt | u: update 1-8 first | console ? < examples/update-file.txt | u: | hole"
```

Output:
```text
00010000alpha
00015000inserted
00020000beta
00040000delta-new
```

```text
pipe "(ENDCHAR ?) < examples/update-master.txt | u: update 1-8 first | hole ? < examples/update-file.txt | u: | console"
```

Output:
```text
./ * portable SEQ8-style update (sequence in columns 1-8)
./ I 00010000
00015000inserted
./ D 00030000
00030000gamma
./ R 00040000
00040000delta
00040000delta-new
```

```text
pipe "(ENDCHAR ?) < examples/update-master.txt | u1: update 1-8 first | u2: update 1-8 last | console ? < examples/update-file.txt | u1: | hole ? < examples/update-second.txt | u2: | hole"
```

Output:
```text
00010000alpha
00015000inserted
00040000delta-new
```

```text
pipe "literal a | update seq8 | hole"
```

Error:
```text
PIPE: UPDATE: unrecognized operand seq8
```

```text
pipe "literal a | update first 1-8 | hole"
```

Error:
```text
PIPE: UPDATE: unrecognized operand 1-8
```

```text
pipe "literal a | updat | hole"
```

Error:
```text
PIPE: unknown stage: UPDAT
```

Invented operands (`seq8`, `inc`, `ctl`, `noseq8`, `strict`, `firs`, `las`)
and the reverse keyword order `last first` / `first 1-8` are rejected.

### PIPCMD — issue pipeline commands from input records

Control (Author’s Edition 1.1.11 pp.457–458; commands Ch.21 pp.638–660).
Syntax is `PIPCMD [ STOP [ ALLEOF | ANYEOF | number ] ]` (the diagram is
all-caps, so `pipcm` is not an abbreviation). Each primary-input record is
a pipeline command. PIPCMD itself writes nothing; `OUTPUT` and `CALLPIPE`
write on the selected stream. A blank before `|` is part of LITERAL’s
argument (p.412), so write `literal output hello| pipcmd` with no space
after the command text.

`STOP` (default `ALLEOF`) inspects output EOF after each command. The
p.458 example `literal resolve insert | pipcmd` now sets a nonzero PIPE RC
because INSERT is a built-in (RC 0 still means the name is not registered).

```text
pipe "literal output hello| pipcmd | console"
```

Output:
```text
hello
```

```text
pipe "literal output two|literal output one| pipcmd | console"
```

Output:
```text
one
two
```

```text
pipe "literal callpipe literal hello||*:| pipcmd | console"
```

Output:
```text
hello
```

```text
pipe "literal output c|literal output b|literal output a| pipcmd stop anyeof | take 1 | console"
```

Output:
```text
a
```

```text
pipe "literal a | pipcmd forever | hole"
```

Error:
```text
PIPE: PIPCMD: unrecognized operand forever
```

```text
pipe "literal readto line | pipcmd | hole"
```

Error:
```text
PIPE: PIPCMD: READTO is not available to PIPCMD
```

```text
pipe "literal addstream | pipcmd | hole"
```

Error:
```text
PIPE: PIPCMD: ADDSTREAM is not implemented
```

```text
pipe "literal a | pipcm | hole"
```

Error:
```text
PIPE: unknown stage: PIPCM
```

Invented operands (`forever`, `anyeof` without `stop`, `stop forever`)
are rejected. REXX-only commands (`readto`, `peekto`, …) and allowlist
members this engine cannot honor yet (`addstream`, `eofreport`,
`issuemsg`, `setrc`, the `rexx` subcommand) are rejected.

### PIPESTOP — stop stages waiting on an external event

Arcane control (Author’s Edition 1.1.11 pp.458–459; tutorial p.150; DELAY
p.322). Syntax is a bare `PIPESTOP` (the diagram is all-caps, so `pipesto`
is not an abbreviation). No operands. When an input record arrives, CMS
posts every ECB in the pipeline set so stages waiting on an external event
terminate; this engine has no CMS ECBs, so PIPESTOP wakes stages parked in
`wait_until` / `wait_until_or_input` (DELAY, BEAT) and forbids further
time waits. It then copies the record to the primary output if that stream
is connected. It does not abort a pipeline that is only copying records.
The book’s timer is `literal +60 | delay | pipestop`; tests use a short
interval so the suite stays fast.

```text
pipe "literal hello| pipestop | console"
```

Output:
```text
hello
```

```text
pipe "(ENDCHAR ?) literal +5 | delay | hole ? literal go| pipestop | console"
```

Output:
```text
go
```

```text
pipe "(ENDCHAR ?) literal a| copy | console ? literal go| pipestop | hole"
```

Output:
```text
a
```

```text
pipe "(ENDCHAR ?) literal +0 | delay | pipestop"
```

Output: *(no records)*

```text
pipe "literal a | pipestop forever | hole"
```

Error:
```text
PIPE: PIPESTOP: unrecognized operand forever
```

```text
pipe "literal a | pipesto | hole"
```

Error:
```text
PIPE: unknown stage: PIPESTO
```

Invented operands (`forever`, `strict`, `stop`, numbers) are rejected.

### RUNPIPE — issue nested PIPE specifications, intercepting messages

Control (Author’s Edition 1.1.11 pp.481–483). Syntax is
`RUNPIPE [ MSGLevel number | Xhex ] [ TRACE | EVENTS [ MASK hex ] ]`
(the diagram is all-caps, so `runpip` is not an abbreviation). Must not be
a first stage. Each primary-input record is a PIPE specification issued as
a new pipeline set (the same syntax as the PIPE command, including a
leading `(…)` options group). Nested pipeline **messages** are written to
RUNPIPE’s primary output; nested data records are not. Null and blank
input records are discarded. `||` self-escapes `|` in a LITERAL that
feeds RUNPIPE.

On CMS, interception goes to the pipeline message repository. This port
writes a 10-character identifier (`PIPES0001E` errors, `PIPES0002I` TRACE,
`PIPES0003I` MESSAGE) plus the text. `TRACE` forces nested TRACE into that
stream. `MSGLEVEL` is accepted and ignored (no PIPMOD message-level bits).
`EVENTS` / `MASK` are parsed in diagram order and rejected: Appendix G is
not implemented.

The PIPE return code is the aggregate of the nested pipeline RCs (most
negative, else largest). Last-stage RUNPIPE ignores write errors, matching
the p.483 availability test.

```text
pipe "literal hole | runpipe | console"
```

Output: *(no records)*

```text
pipe "literal nosuch | runpipe | console"
```

Output:
```text
PIPES0001E unknown stage: NOSUCH
```

```text
pipe "literal literal hello||hole | runpipe | console"
```

Output: *(no records)*

```text
pipe "literal hole | runpipe trace | take 1 | hole"
```

Output: *(no records)*
TRACE records; take 1 keeps the first.

```text
pipe "literal a | runpipe forever | hole"
```

Error:
```text
PIPE: RUNPIPE: unrecognized operand forever
```

```text
pipe "literal a | runpipe events | hole"
```

Error:
```text
PIPE: RUNPIPE: EVENTS is not implemented
```

```text
pipe "literal a | runpip | hole"
```

Error:
```text
PIPE: unknown stage: RUNPIP
```

Invented operands (`forever`, `anyeof`, `mask` without `events`,
`trace msglevel 1`) are rejected.

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
The book’s `strfind` example is now in this port; `locate` / `find` /
`strfind` all work as the nested `word`. The p.390 `insert /-> /` /
`insert /   /` cascade is the INSERT built-in.

```text
pipe "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | copy | if1: | console"
```

Output:
```text
a
abc
b
```

```text
pipe "(ENDCHAR ?) literal upA keep upB skip | split | if1: if locate /up/ | spec 3-* 1 | if1: | console"
```

Output:
```text
A
keep
B
skip
```

```text
pipe "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | change // /-> / | if1: | change // /   / | if1: | console"
```

Output:
```text
   a
-> abc
   b
```

```text
pipe "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | insert /-> / | if1: | insert /   / | if1: | console"
```

Output:
```text
   a
-> abc
   b
```
p.390 book cascade with INSERT.

```text
pipe "(ENDCHAR ?) literal a b c | split | if1: if locate /b/ | change // /X/ | if1: | console"
```

Output:
```text
a
Xb
c
```

```text
pipe "(ENDCHAR ?) literal a abc b | split | if1: if strfind /abc/ | copy | if1: | console"
```

Output:
```text
a
abc
b
```
p.390 railroad word; STRFIND is the book’s nested example.

A missing nested verb, an unregistered verb (`then`, `strliteral`, `nosuch`),
a primary-only pipe, and a fourth stream are rejected.

### NOT — run a stage with output streams inverted

Control (Author’s Edition 1.1.11 pp.441–442). Verb `NOT` is all-caps
(`no` is not an abbreviation). Syntax: `NOT word [string]`. The nested
primary output is connected to NOT’s secondary if that stream is defined;
the nested secondary output is connected to NOT’s primary. The book
assumes a selection stage that supports two outputs; any registered
`word` is accepted. `not locate /x/` inverts LOCATE (same primary records
as NLOCATE). The book example `not chop .` uses CHOP’s default BEFORE the
first period: the remainder including the period is the nested secondary,
so it leaves on NOT’s primary.

```text
pipe "literal ax b x | split | not locate /x/ | console"
```

Output:
```text
b
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx ax b x | N: not locate /x/ | console ? N: | console"
```

Output:
```text
ax
x
b
```
Both streams write to the terminal.

```text
pipe "literal a.b.c| not chop . | console"
```

Output:
```text
.b.c
```
p.441: part of the record from the first period.

```text
pipe "(ENDCHAR ?) literal left.right| N: not chop . | console ? N: | console"
```

Output:
```text
left
.right
```
Both streams write to the terminal.

```text
pipe "literal a | not | hole"
```

Error:
```text
PIPE: NOT: a word (the name of the program to run) is required
```

```text
pipe "literal a | not nosuch /a/ | hole"
```

Error:
```text
PIPE: NOT: unknown stage: nosuch
```

```text
pipe "literal a | no locate /a/ | hole"
```

Error:
```text
PIPE: unknown stage: NO
```

Invented nested verbs (`forever`, `nosuch`) are rejected.

### DAM — pass records once primed

Gateway (Author’s Edition 1.1.11 pp.310–311). Converse of GATE: wait for a
record on the primary input, then short every stream pair so the held file
can flow. The diagram is a bare `DAM` (all-caps; `da` is not an abbreviation,
and GATE’s `STRICT` is not an operand). The book’s example uses FANOUT plus
ELASTIC to hold records on the secondary while LOCATE decides:

```text
pipe "(ENDCHAR ?) literal a b abc c | split | o: fanout | locate /abc/ | d: dam | hole ? o: | elastic | d: | console"
```

Output:
```text
a
b
abc
c
```

```text
pipe "(ENDCHAR ?) literal a b c | split | o: fanout | locate /zzz/ | d: dam | hole ? o: | elastic | d: | console"
```

Output: *(no records)*

```text
pipe "literal a|literal b| dam | console"
```

Output:
```text
b
a
```

```text
pipe "literal a | drop | dam | console"
```

Output: *(no records)*

```text
pipe "(ENDCHAR ?) literal a b c | split | f: fanout | d: dam | hole ? f: | d: | console"
```

Output:
```text
a
b
c
```
FANOUT feeds both pairs; no ELASTIC.

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
```

Output:
```text
aa
bb
```

```text
pipe "(ENDCHAR ?) literal P| f: fanout | j: juxtapose count | console ? f: | dup | j:"
```

Output:
```text
         1PP
         2PP
```

```text
pipe "(ENDCHAR ?) literal a b | split | j: juxtapose | hole ? j: | console"
```

Output:
```text
a
b
```
No secondary input: every primary is unused.

Invented operands (`strict`, `pad`, `coun`) are rejected.

### SYNCHRONISE — wait for a record on every input, then copy one from each

Arcane gateway (Author’s Edition 1.1.11 pp.536–539). No operands. The diagram
is `SYNChronise` (minimum abbreviation `sync`); `SYNChronize` is the American
spelling synonym. It peeks every connected input in numerical order, writes
one record to each corresponding output, then consumes the inputs. End-of-file
on any stream stops the cycle, so a short file paces a longer one:

```text
pipe "literal a|literal b| synchronise | console"
```

Output:
```text
b
a
```

```text
pipe "literal a|literal b| synchronize | console"
```

Output:
```text
b
a
```

```text
pipe "literal hi| sync | console"
```

Output:
```text
hi
```

```text
pipe "(ENDCHAR ?) literal a b c | split | S: synchronise | console ? literal x y z | split | S: | console"
```

Output:
```text
a
x
b
y
c
z
```
Both streams write to the terminal.

```text
pipe "(ENDCHAR ?) literal a b c | split | S: synchronise | console ? literal x| S: | console"
```

Output:
```text
a
x
```
Both streams write to the terminal.

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
(p.85). FIND is registered; `change` and `locate` are also used as
stand-ins. A following word is not a nested stage (unlike IF/ZONE) and
is rejected.

```text
pipe "(ENDCHAR ?) literal a abc b | split | o: fanout stop anyeof | p: predselect | console ? o: | l: locate /abc/ | p: | hole ? l: | p:"
```

Output:
```text
abc
```

```text
pipe "(ENDCHAR ?) literal keep-secret|literal skip-me| o: fanout stop anyeof | p: predselect | console ? o: | change /secret/ /XXXXX/ | l: locate /XXXXX/ | p: | hole ? l: | p:"
```

Output:
```text
keep-secret
```

```text
pipe "(ENDCHAR ?) literal a abc b | split | o: fanout stop anyeof | p: predselect | console ? o: | l: locate /abc/ | p: | console ? l: | p:"
```

Output:
```text
a
abc
b
```
Both streams write to the terminal.

```text
pipe "(ENDCHAR ?) literal xxapple|literal xxpear| o: fanout stop anyeof | p: predsel | console ? o: | spec 3-* 1 | l: locate /apple/ | p: | hole ? l: | p:"
```

Output:
```text
xxapple
```

Invented operands (`strict`, `stop`, `locate`) are rejected.

### XLATE — transliterate contents of records

Filter (Author’s Edition 1.1.11 pp.604–607; tutorial pp.44–46). Verb `XLATE`
(all-caps) or `TRANSlate` (minimum `trans`). Operand order is fixed: an
optional `inputRange` or a parenthesized list, then named tables
(`UPPER` / `LOWER` / `INPUT` / `OUTPUT`), then from/to xrange pairs.
No arguments (and no secondary table) defaults to `UPPER`. Pairs without a
table keyword use the identity table; use `*-*` when the first from-range
would also parse as a column range (p.607 Note 2).

Portable ASCII: `UPPER` maps `a`–`z` to `A`–`Z`; `LOWER` maps `A`–`Z` to
`a`–`z`. Book examples that use EBCDIC code points (X'40' blank, X'F1'
digit) are shown here with ASCII bytes. Code-page tables (`TO` / `FROM` /
`A2E` / `E2A`) are rejected.

```text
pipe "literal Hello, world|xlate upper|console"
```

Output:
```text
HELLO, WORLD
```

```text
pipe "literal Hello, WORLD|xlate lower|console"
```

Output:
```text
hello, world
```

```text
pipe "literal abcABC|xlate upper a z|console"
```

Output:
```text
zBCABC
```
Book p.606; ASCII letters.

```text
pipe "literal This|xlate upper A-Z a-z|console"
```

Output:
```text
tHIS
```
Reverse case; ASCII A-Z is contiguous.

```text
pipe "literal hello|xlate a-y b-z i j r s z a|console"
```

Output:
```text
ifmmp
```
Figure 89 Caesar; extra i j r s are EBCDIC-hole fillers.

```text
pipe "literal Hello, world!|xlate *-* 21-2f blank|console"
```

Output:
```text
Hello  world 
```
ASCII punctuation → blank; book 40-7f / X'40' is EBCDIC.
The record ends with one trailing blank.

```text
pipe "literal abc|xlate 01-80 40|hole"
```

Error:
```text
PIPE: XLATE: Odd number of translate pairs
```
Figure 88: 01-80 is a column range.

`INPUT` / `OUTPUT` are identity (no CMS SET). Invented operands (`anycase`,
`strict`) and code-page tables are rejected.

### INSERT — insert a string in records

Filter (Author’s Edition 1.1.11 p.392). Verb `INSERT` is all-caps (`ins` is
not an abbreviation). Operand order is fixed: a required `delimitedString`,
optional `BEFORE` (default) or `AFTER`, optional `inputRange`. The default
range is the entire record, so `insert /*/ after` appends. When the range
is not in the record, INSERT uses the first column (range relative to the
beginning) or the last column (relative to the end).

```text
pipe "literal abc def | split | insert /*/ after | console"
```

Output:
```text
abc*
def*
```
p.392 book example.

```text
pipe "literal abc def | split | insert /*/ | console"
```

Output:
```text
*abc
*def
```
Default BEFORE, entire record.

```text
pipe "literal abcdef| insert /X/ after 3 | console"
```

Output:
```text
abcXdef
```

```text
pipe "literal one two three| insert /-/ after word 1 | console"
```

Output:
```text
one- two three
```

```text
pipe "literal abc| insert x2a after | console"
```

Output:
```text
abc*
```
ASCII X'2A'; book hex examples are often EBCDIC.

```text
pipe "literal a| insert /*/ in | hole"
```

Error:
```text
PIPE: INSERT: unexpected operand in
```

Invented operands (`in`, `first`, `anycase`) and `AFTER` before the string
are rejected.

### PAD — expand short records

Filter (Author’s Edition 1.1.11 p.451). Verb `PAD` is all-caps (`pa` is not
an abbreviation). Operand order is fixed: optional `Right` (default) or
`Left` (min 1), optional `[number] MODULO`, a required number, optional
`BLANK` (default) or xorc. Without `MODULO` the number is the minimum
length; records that are already that long (or longer) pass unmodified.
With `MODULO` the output length is the smallest multiple of the modulo
plus an optional offset (default 0). The first number must be smaller
than the second. Portable ASCII: `BLANK` is X'20' (the book cites EBCDIC
X'40'). The book’s `console | pad 80 | chop 80 | > input file a fixed`
example is a CMS fixed file; the lines below use `literal` / `chop`.

```text
pipe "literal hi| pad 5 | console"
```

Output:
```text
hi   
```
The record ends with 3 trailing blanks.

```text
pipe "literal hello| pad 5 | console"
```

Output:
```text
hello
```

```text
pipe "literal hi| pad left 5 | console"
```

Output:
```text
   hi
```

```text
pipe "literal hi| pad 5 * | console"
```

Output:
```text
hi***
```

```text
pipe "literal abcde| pad modulo 4 | console"
```

Output:
```text
abcde   
```
The record ends with 3 trailing blanks.

```text
pipe "literal ab| pad 1 modulo 4 | console"
```

Output:
```text
ab   
```
The record ends with 3 trailing blanks.

```text
pipe "literal hi| pad 8 | chop 8 | console"
```

Output:
```text
hi      
```
p.451 pad 80 | chop 80, shortened.
The record ends with 6 trailing blanks.

```text
pipe "literal a| pad first 80 | hole"
```

Error:
```text
PIPE: PAD: unexpected operand first
```

Invented operands (`first`, `both`, `center`) and `LEFT` after the number
are rejected.

### COMBINE — combine a run of records into one

Filter (Author’s Edition 1.1.11 pp.298–300). Verb `COMBINE` is all-caps
(`comb` is not an abbreviation). The function keyword is required:
`Or` (min 1), `N` or exact `AND`, `X` or `EXCLUSIVEOR` (min 3), `FIRST`,
`LAST`. `FIRST` / `LAST` take the first or last record that **contains
that column** (a zero byte still counts; a shorter record does not).
Default run size is a pair. `*` combines the whole file. `KEYLENGTH n`
keeps the first *n* columns unchanged and combines only the rest.
Two input streams pair one record from each; `STOP ALLEOF` (default) /
`ANYEOF` / `number` say when to stop. Different lengths are not padded
(Note 2 p.300). Portable ASCII: bitwise OR/AND/XOR are raw bytes. The
book’s p.299 XOR map uses X'00' as equal (same byte on ASCII); `DEBLOCK 1`
is omitted.

```text
pipe "literal AAA BBB | split | combine or | console"
```

Output:
```text
CCC
```
'A'|'B' = 'C'.

```text
pipe "literal AAA BBB | split | combine n | console"
```

Output:
```text
@@@
```
'A'&'B' = X'40'.

```text
pipe "literal ABCD EF | split | combine first | console"
```

Output:
```text
ABCD
```

```text
pipe "literal AB CD EF | split | combine 2 last | console"
```

Output:
```text
EF
```

```text
pipe "literal A B C | split | combine * or | console"
```

Output:
```text
C
```

```text
pipe "literal AA111 AA222 BB333 | split | combine keylength 2 or | console"
```

Output:
```text
AA333
BB333
```

```text
pipe "literal AAA| append literal BBB | combine x | xlate *-* 01-ff 1 00 0 | console"
```

Output:
```text
111
```
p.299 XOR inequality map; portable ASCII.

```text
pipe "(ENDCHAR ?) literal AAA| C: combine or | console ? literal BBB| C:"
```

Output:
```text
CCC
```

```text
pipe "literal a | combine xor | hole"
```

Error:
```text
PIPE: COMBINE: unexpected operand xor
```

Invented operands (`xor`, `a`, `an`, `pad`) and a missing function are
rejected. `STOP` is valid only with two input streams; a number /
`KEYLENGTH` is rejected when two inputs are defined.

### OVERLAY — overlay data from input streams

Gateway (Author’s Edition 1.1.11 p.447). Verb `OVERlay` (minimum
abbreviation `over`). Optional pad character: `BLANK` (default) or xorc
(one character, two hex digits, `SPACE`, or `TABULATE`). Each output
column is the character from the **highest-numbered** input stream where
that column is present and is not the pad. A shorter record does not
fill missing columns with pad. Output length is the longest input record
of the cycle. All connected inputs are read, beginning with stream 0;
only the primary output is written. End-of-file on one stream drops that
stream from later cycles; overlay stops when every input is at
end-of-file. Portable ASCII: `BLANK` is X'20' (the book cites EBCDIC
X'40').

```text
pipe "(ENDCHAR ?) literal the left/the right| c: chop before / | o: overlay | console ? c: | spec 2-* 4 | o:"
```

Output:
```text
thethefright
```
p.447; no space before `|` so LITERAL has no trailing blank.

```text
pipe "(ENDCHAR ?) literal the left/the right| c: chop before / | o: overlay | console ? c: | spec 2-* 1.50 right | o:"
```

Output:
```text
the left                                 the right
```
p.447; 50-column flush.

```text
pipe "(ENDCHAR ?) literal ABC| O: overlay | console ? literal   X| O:"
```

Output:
```text
ABX
```

```text
pipe "(ENDCHAR ?) literal A*C| O: overlay * | console ? literal *B*| O:"
```

Output:
```text
ABC
```

```text
pipe "literal hello| over | console"
```

Output:
```text
hello
```

```text
pipe "literal a | overlay pad | hole"
```

Error:
```text
PIPE: OVERLAY: invalid xorc pad
```

Invented operands (`pad`, `anycase`, `stop`, `first`) and a second
operand after the pad are rejected. `ove` is not an abbreviation.

### REVERSE — reverse contents of records

Filter (Author’s Edition 1.1.11 p.474). Verb `REVERSE` is all-caps (`rev`
is not an abbreviation). No operands. Each record is written with its
bytes reversed (portable ASCII: one character is one byte).

```text
pipe "literal Hello, World!| reverse | console"
```

Output:
```text
!dlroW ,olleH
```
p.474; no space before `|` so LITERAL has no trailing blank.

```text
pipe "literal fooxyz|literal abxyz|literal xy| reverse | find zyx| reverse | console"
```

Output:
```text
abxyz
fooxyz
```
p.474: records that end in xyz; literal cascade is reverse order.

```text
pipe "literal a | reverse 1 | hole"
```

Error:
```text
PIPE: REVERSE: unexpected operand 1
```

Invented operands are rejected.

### RETAB — replace runs of blanks with tabulate characters

Filter (Author’s Edition 1.1.11 p.473). Verb `RETAB` is all-caps (`reta`
is not an abbreviation). Operand is optional: default `-3` (a tab stop in
column 1 and every 3 columns, `1 4 7 …`), or a single negative number, or
a list of positive stop numbers in any order. The smallest positive number
is the left margin; columns before it are discarded. Converse of `UNTAB`.
Portable ASCII: tabulate is HT X'09' (the book cites EBCDIC X'05'). The
examples use `change x09 /*/` so a tab shows as `*`. The book’s
`< ebcdic file|retab -8|block 4000 c term|xlate e2a|> unix file a` example
is a CMS/Unix file stand-in; the lines below use `literal`.

```text
pipe "literal a  b| retab | change x09 /*/ | console"
```

Output:
```text
a*b
```
Default `-3`; two blanks from column 2 reach stop 4.

```text
pipe "literal a  b| retab -3 | change x09 /*/ | console"
```

Output:
```text
a*b
```

```text
pipe "literal a       b| retab -8 | change x09 /*/ | console"
```

Output:
```text
a*b
```
p.473 `-8` (stops 1, 9, 17, …). Seven blanks from column 2 reach stop 9.

```text
pipe "literal a  b| retab -8 | change x09 /*/ | insert /*/ after | console"
```

Output:
```text
a  b*
```
Two blanks do not reach column 9, so they stay blanks.

```text
pipe "literal a  b| retab 7 1 4 | change x09 /*/ | console"
```

Output:
```text
a*b
```
Positive stops may be in any order.

```text
pipe "literal WXYZ     Q| retab 5 10 | change x09 /*/ | console"
```

Output:
```text
*Q
```
Left margin 5 discards `WXYZ`; five blanks from column 5 reach stop 10.

```text
pipe "literal a  |retab | change x09 /*/ | console"
```

Output:
```text
a*
```
Trailing blanks that reach a stop become a tab. No space before `|`.

```text
pipe "literal| retab | insert /./ after | console"
```

Output:
```text
.
```
Null record; INSERT marks it so the empty record is visible.

```text
pipe "literal a | retab pad | hole"
```

Error:
```text
PIPE: RETAB: unexpected operand pad
```

```text
pipe "literal a | reta | hole"
```

Error:
```text
PIPE: unknown stage: RETA
```

Invented operands (`pad`, `blank`, `tabulate`, `*`) and mixing a negative
interval with a positive list are rejected.

### UNTAB — replace tabulate characters with blanks

Filter (Author’s Edition 1.1.11 p.575). Verb `UNTAB` is all-caps (`unta`
is not an abbreviation). Same operand forms as `RETAB` (default `-3`, a
single `-number`, or a positive list). Expands HT to blanks so the next
character lines up on a tab stop. A tab past the last finite stop becomes
one blank. UNTAB does not discard a left-margin prefix (that Operation
paragraph is RETAB only). The book’s `var data | untab | var data` example
is REXX VAR (Phase 5); the lines below use `change /*/ x09` to inject a tab.

```text
pipe "literal a*b| change /*/ x09 | untab | insert /*/ after | console"
```

Output:
```text
a  b*
```
Default `-3`; tab at column 2 expands to column 4.

```text
pipe "literal a*b| change /*/ x09 | untab -3 | insert /*/ after | console"
```

Output:
```text
a  b*
```

```text
pipe "literal a*b| change /*/ x09 | untab -8 | insert /*/ after | console"
```

Output:
```text
a       b*
```

```text
pipe "literal hello| untab | console"
```

Output:
```text
hello
```
No tabs: the record is unchanged.

```text
pipe "literal *X| change /*/ x09 | untab | insert /*/ after | console"
```

Output:
```text
   X*
```
A leading tab expands to three blanks (columns 1–3), then `X` at column 4.

```text
pipe "literal a*b| change /*/ x09 | untab 1 | insert /*/ after | console"
```

Output:
```text
a b*
```
Finite stop list `1`: the tab at column 2 is past the last stop, so one blank.

```text
pipe "literal| untab | insert /./ after | console"
```

Output:
```text
.
```

```text
pipe "literal a | untab pad | hole"
```

Error:
```text
PIPE: UNTAB: unexpected operand pad
```

```text
pipe "literal a | unta | hole"
```

Error:
```text
PIPE: unknown stage: UNTA
```

### 64ENCODE — encode a byte stream to MIME Base-64

Filter (Author’s Edition 1.1.11 pp.613–614). Verb `64ENCODE` is all-caps
(`64encod` is not an abbreviation). No operands. RFC 1521 MIME Base-64.
Input is a **byte stream**: records are concatenated; null records are
discarded. Output records are 76 characters except possibly the last.
Portable ASCII alphabet `A–Z a–z 0–9 + /` (the book says encoded data are
EBCDIC). Padding uses `=`.

```text
pipe "literal hello| 64encode | console"
```

Output:
```text
aGVsbG8=
```

```text
pipe "literal Man| 64encode | console"
```

Output:
```text
TWFu
```

```text
pipe "literal a| 64encode | console"
```

Output:
```text
YQ==
```
One leftover byte: two pad characters.

```text
pipe "literal ab| 64encode | console"
```

Output:
```text
YWI=
```

```text
pipe "literal abc| 64encode | console"
```

Output:
```text
YWJj
```

```text
pipe "literal lo|literal|literal hel| 64encode | console"
```

Output:
```text
aGVsbG8=
```
Null record discarded; `hel` then `lo` encode as `hello`.

```text
pipe "literal AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA| 64encode | console"
```

Output:
```text
QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB
```
57 bytes of `A` fill one 76-character record (no pad).

```text
pipe "literal a | 64encode 76 | hole"
```

Error:
```text
PIPE: 64ENCODE: unexpected operand 76
```

```text
pipe "literal a | 64encod | hole"
```

Error:
```text
PIPE: unknown stage: 64ENCOD
```

### 64DECODE — decode MIME Base-64 records

Filter (Author’s Edition 1.1.11 p.613). Verb `64DECODE` is all-caps.
No operands. One output record per input record. Characters outside the
alphabet are ignored. The record length need not be a multiple of four.

```text
pipe "literal aGVsbG8=| 64decode | console"
```

Output:
```text
hello
```

```text
pipe "literal hello| 64encode | 64decode | console"
```

Output:
```text
hello
```

```text
pipe "literal YQ| 64decode | console"
```

Output:
```text
a
```
Unpadded leftover two characters (p.613: multiple of four is not enforced).

```text
pipe "literal YQ== | 64decode | console"
```

Output:
```text
a
```
Blanks in the encoded record are ignored.

```text
pipe "literal a | 64decode PAD | hole"
```

Error:
```text
PIPE: 64DECODE: unexpected operand PAD
```

```text
pipe "literal a | 64decod | hole"
```

Error:
```text
PIPE: unknown stage: 64DECOD
```

### QPENCODE — encode records as MIME quoted-printable

Filter (Author’s Edition 1.1.11 p.467). Verb `QPENCODE` is all-caps
(`qpencod` is not an abbreviation). No operands. RFC 2045 quoted-printable
in the ASCII domain. Each input record becomes as many 76-byte output
records as required; continuation records end with `=` so `QPDECODE`
splices them. Trailing blanks are encoded as `=20`. In addition to RFC
2045, characters that are not codepage-invariant in the EBCDIC domain
are escaped (`!`, `"`, `#`, `$`, `@`, `[`, `\`, `]`, `^`, backtick,
`{`, `|`, `}`, `~`).

```text
pipe "literal abc| qpencode | console"
```

Output:
```text
abc
```

```text
pipe "literal a=b| qpencode | console"
```

Output:
```text
a=3Db
```

```text
pipe "literal @| qpencode | console"
```

Output:
```text
=40
```

```text
pipe "literal aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa| qpencode | console"
```

Output:
```text
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa=
aa
```
77 `a`s: a 76-byte continuation (75 letters plus soft-break `=`) and a
2-byte last record.

```text
pipe "literal a | qpencode 76 | hole"
```

Error:
```text
PIPE: QPENCODE: unexpected operand 76
```

```text
pipe "literal a | qpencod | hole"
```

Error:
```text
PIPE: unknown stage: QPENCOD
```

### QPDECODE — decode MIME quoted-printable records

Filter (Author’s Edition 1.1.11 p.466). Verb `QPDECODE` is all-caps.
No operands. `=XX` (two hex digits, case ignored) becomes one byte; other
characters pass unchanged. A trailing `=` splices with the next record.
Invalid escapes fail the stage unless a secondary output is connected,
in which case any splice buffer goes to primary and the bad record goes
to secondary.

```text
pipe "literal a=3Db| qpdecode | console"
```

Output:
```text
a=b
```

```text
pipe "literal a=3db| qpdecode | console"
```

Output:
```text
a=b
```
Hex case is ignored.

```text
pipe "literal abc| qpencode | qpdecode | console"
```

Output:
```text
abc
```

```text
pipe "literal aa|literal aaaaaaaa=| qpdecode | console"
```

Output:
```text
aaaaaaaaaa
```
Soft-break `=` splices the two input records.

```text
pipe "literal a=GG | qpdecode | hole"
```

Error:
```text
PIPE: QPDECODE: invalid escape sequence
```

```text
pipe "(ENDCHAR ?) literal ok|literal a=GG|literal AB=| Q: qpdecode | console ? Q: | console"
```

Output:
```text
AB
ok
a=GG
```
Both streams write to the terminal. The splice buffer `AB` and the
following good record go to primary; the bad record goes to secondary.

```text
pipe "literal a | qpdecode PAD | hole"
```

Error:
```text
PIPE: QPDECODE: unexpected operand PAD
```

```text
pipe "literal a | qpdecod | hole"
```

Error:
```text
PIPE: unknown stage: QPDECOD
```

### SNAKE — build a multicolumn page layout

Filter (Author’s Edition 1.1.11 pp.491–492; walkthrough pp.16–17). Verb
`SNAKE` is all-caps (`snak` is not an abbreviation). Operand order is
fixed: a required number of columns, optional page depth. When the depth
is omitted, SNAKE reads the file and uses the minimum rows that hold the
records. Columns are filled **down, then across** (first output line is
records 1, n+1, 2·n+1). SNAKE neither pads nor truncates; the walkthrough
uses `pad` first so columns line up. There is no separator operand and no
secondary stream. The book’s `listpds` / `< many users` producers are CMS;
the lines below use `literal` / `split` / `pad`. Portable ASCII: one
character is one byte.

```text
pipe "literal a b c d | split | snake 2 | console"
```

Output:
```text
ac
bd
```
p.492 book example.

```text
pipe "literal 1 2 3 4 5 6 | split | snake 3 | console"
```

Output:
```text
135
246
```
Down the column, then across; n = 2.

```text
pipe "literal 1 2 3 4 5 | split | snake 2 | console"
```

Output:
```text
14
25
3
```
Last column not filled completely.

```text
pipe "literal a b c d e f g | split | snake 2 2 | console"
```

Output:
```text
ac
bd
eg
f
```
Specified page depth; last partial page.

```text
pipe "literal AA BB CC DD EE FF | split | pad 3 | snake 3 | console"
```

Output:
```text
AA CC EE 
BB DD FF 
```
pp.16–17 pad-then-snake stand-in.
Record 1 ends with one trailing blank.
Record 2 ends with one trailing blank.

```text
pipe "literal aa b ccc d | split | snake 2 | console"
```

Output:
```text
aaccc
bd
```
Neither pads nor truncates.

```text
pipe "literal a | snake columns 2 | hole"
```

Error:
```text
PIPE: SNAKE: unexpected operand columns
```

Invented operands (`columns`, `separator`, `pad`, `across`) and a missing
or zero column count are rejected.

### SPACE — space words like REXX SPACE

Filter (Author’s Edition 1.1.11 p.495). Verb `SPACE` is all-caps (`spac` is
not an abbreviation). Operand order is fixed: optional count, optional
replacement (`xorc` or `[STRing] dstring`), optional delimiter (`xorc` or
`[ANYof] dstring`). Defaults are count 1, replacement one blank, delimiter
one blank. Leading and trailing delimiter characters are removed; internal
runs become that many copies of the replacement. A single string argument
is the replacement unless `ANYOF` is specified or the count is zero (then
it is the delimiter). Portable ASCII: `BLANK`/`SPACE` is X'20' (the book
cites EBCDIC X'40').

```text
pipe "literal         a   b    b    |space|insert /*/ after | console"
```

Output:
```text
a b b*
```
p.495.

```text
pipe "literal         a   b    b    |space blank|insert /*/ after | console"
```

Output:
```text
a b b*
```
p.495.

```text
pipe "literal 3.6=7,3|space 0 /,.=/ | console"
```

Output:
```text
3673
```
p.495; no space before `|` so LITERAL has no trailing blank.

```text
pipe "literal a   b| space 2 | console"
```

Output:
```text
a  b
```

```text
pipe "literal a   b| space * | console"
```

Output:
```text
a*b
```

```text
pipe "literal a,b.c| space string /-/ anyof /,./ | console"
```

Output:
```text
a-b-c
```

```text
pipe "literal a | space pad | hole"
```

Error:
```text
PIPE: SPACE: unexpected operand pad
```

Invented operands (`pad`, `both`, `leading`) and a number after `ANYOF`
are rejected. `STRing` min 3; `ANYof` min 3.

### TOKENISE — split a record into tokens

Filter (Author’s Edition 1.1.11 p.558). Verbs `TOKENISE` and `TOKENIZE`
are all-caps (`token` is not an abbreviation). Syntax:
`TOKENISE dstring [dstring]`. Blanks always delimit and are discarded.
Characters in the first dstring are single-character tokens written on
their own. The optional second dstring is written as a separate record
after each input line. A null first dstring (`//`) splits on blanks only.
A null input record writes no tokens (the second dstring, if present, is
still written). Portable ASCII: blank is X'20' (the book cites EBCDIC
X'40').

```text
pipe "literal apples = bananas(cherries+dates) | tokenise /()=+/ | console"
```

Output:
```text
apples
=
bananas
(
cherries
+
dates
)
```
p.558.

```text
pipe "literal a=b| tokenize /=/ | console"
```

Output:
```text
a
=
b
```

```text
pipe "literal apples=bananas| tokenise // | console"
```

Output:
```text
apples=bananas
```
Null first dstring: blanks only.

```text
pipe "literal a b| tokenise // /:/ | console"
```

Output:
```text
a
b
:
```
Second dstring after each line.

```text
pipe "literal a | tokenise | hole"
```

Error:
```text
PIPE: TOKENISE: delimitedString is required
```

```text
pipe "literal a | tokenise ANYCASE /(/ | hole"
```

Error:
```text
PIPE: TOKENISE: unexpected operand ANYCASE
```

Invented operands and a missing first dstring are rejected.

### FIND / NFIND — XEDIT-style prefix selection

Selection stages (Author’s Edition 1.1.11 pp.349–350, 438–439). Verbs
`FIND` and `NFIND` are all-caps (`fin` / `nfin` are not abbreviations).
`NOTFIND` is a synonym of NFIND. The string is the rest of the arguments
after exactly one blank (blanks significant; not a delimited string —
use `STRFIND` / `STRNFIND` for a dstring). XEDIT FIND rules: a null string
matches any record; a blank in the string is a don't-care that must still
be present; an underscore requires a blank; other characters must equal
the corresponding column (case-sensitive prefix). Matches go to primary;
rejects go to secondary. NFIND inverts that.

```text
pipe "literal axc | literal abc | find a c| console"
```

Output:
```text
abc 
axc 
```
p.350: don't-care blank; trailing blanks on the literals.
Record 1 ends with one trailing blank.
Record 2 ends with one trailing blank.

```text
pipe "literal axc | literal abc| find a c | console"
```

Output:
```text
axc 
```
p.350: FIND string is four bytes; abc is three.
The record ends with one trailing blank.

```text
pipe "literal abc axc axy xyc | split | nfind a c| console"
```

Output:
```text
axy
xyc
```
p.439 book example.

```text
pipe "literal a b|literal axb| find a_b| console"
```

Output:
```text
a b
```
Underscore = required blank.

```text
pipe "literal /abc/|literal abc| find /abc/| console"
```

Output:
```text
/abc/
```
Slashes are literal, not a LOCATE dstring.

```text
pipe "(ENDCHAR ?) examples/emit.rexx abc xyz | F: find a| console ? F: | console"
```

Output:
```text
xyz
abc
```
Both streams write to the terminal.

`ANYCASE` is a STRFIND keyword, not FIND: `find ANYCASE abc` searches for
that prefix.

### STRFIND / STRNFIND — FIND with a delimited string

Selection stages (Author’s Edition 1.1.11 pp.349, 438, 534). Prefix `STR`
so the argument is a `delimitedString` (pair / hex / binary). Optional
`ANYcase` (min 3; synonyms `CASEANY` / `IGNORECASE` / `CASEIGNORE`) comes
first. XEDIT FIND/NFIND matching still applies: blank is don't-care,
underscore is a required blank. `strfind x02` is byte X'02' in column 1
(p.534); FIND `x02` is the characters x, 0, 2.

```text
pipe "literal xabc|literal abc| strfind /abc/ | console"
```

Output:
```text
abc
```

```text
pipe "literal Abc|literal abc| strfind anycase /abc/ | console"
```

Output:
```text
abc
Abc
```

```text
pipe "literal abc axc axy xyc | split | strnfind /a c/ | console"
```

Output:
```text
axy
xyc
```
Dstring form of the p.439 NFIND example.

```text
pipe "literal a | strfind abc | hole"
```

Error:
```text
PIPE: STRFIND: unrecognized operand abc
```
Dstring required.

`STRLITERAL` is still Phase 6 (p.412 `PREFACE` / `APPEND` / `IFEMPTY`).

### FRLABEL / TOLABEL / WHILELABEL — label runs

Selection stages (Author’s Edition 1.1.11 pp.353–354, 559–560, 596–597).
Verbs are all-caps (`frlab` is not an abbreviation). `FROMLABEL` is a
synonym of FRLABEL. The string is the rest of the arguments after exactly
one blank (no space before `|`; blanks are significant). Matching is a
literal prefix, not XEDIT FIND don't-care. A null string matches any
record; a shorter record does not match.

FRLABEL discards records until the first prefix match, then passes that
record and the rest (INCLUSIVE default). TOLABEL copies records up to
but not including the match (EXCLUSIVE default); remaining records
including the match go to secondary if it is connected. WHILELABEL copies
while records begin with the string, then the rest go to secondary.

STR forms take `[ANYcase] [EXCLUSIVe | INCLUSIVe] delimitedString`
(`EXCLUSIVE` / `INCLUSIVE` only on STRFRLABEL / STRTOLABEL, min 8).
`STRWHILElabel` may be abbreviated `STRWHILE`. Synonym `STRFROMLABEL`.

```text
pipe "literal c|literal b|literal abc|literal a| frlabel abc| console"
```

Output:
```text
abc
b
c
```
p.353: from the first record beginning with abc.

```text
pipe "literal c|literal abc|literal x| fromlabel abc| console"
```

Output:
```text
abc
c
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx a abc b c | F: frlabel abc| console ? F: | console"
```

Output:
```text
a
abc
b
c
```
Both streams write to the terminal.

```text
pipe "literal c|literal .end|literal b|literal a| tolabel .end| console"
```

Output:
```text
a
b
```
p.559: up to .end; the matching record is not selected.

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b .end c | T: tolabel .end| console ? T: | console"
```

Output:
```text
a
b
.end
c
```
Both streams write to the terminal.

```text
pipe "literal *d|literal c|literal *b|literal *a| whilelabel *| console"
```

Output:
```text
*a
*b
```
p.596: run of records beginning with *.

```text
pipe "literal keep|literal *b|literal *a| whilelabel *| hole"
```

Output: *(no records)*

```text
pipe "literal c|literal abc|literal a| strfrlabel /abc/ | console"
```

Output:
```text
abc
c
```

```text
pipe "literal c|literal abc|literal a| strfrlabel exclusive /abc/ | console"
```

Output:
```text
c
```
p.353 EXCLUSIVE: matching record discarded.

```text
pipe "literal c|literal .end|literal a| strtolabel inclusive /.end/ | console"
```

Output:
```text
a
.end
```
p.559 INCLUSIVE: matching record selected.

```text
pipe "literal Ad|literal x|literal aC|literal Ab| strwhilelabel anycase /a/ | console"
```

Output:
```text
Ab
aC
```

```text
pipe "literal a | strfrlabel abc | hole"
```

Error:
```text
PIPE: STRFRLABEL: unrecognized operand abc
```

```text
pipe "literal a | strwhilelabel exclusive /a/ | hole"
```

Error:
```text
PIPE: STRWHILELABEL: unrecognized operand exclusive
```

```text
pipe "literal a | frlab abc | hole"
```

Error:
```text
PIPE: unknown stage: FRLAB
```

```text
pipe "literal a | strliteral /x/ | hole"
```

Error:
```text
PIPE: unknown stage: STRLITERAL
```

Invented operands and STR keywords on the plain verbs (they become part
of the remainder string) are not accepted as keywords. `STRASMFIND` /
`STRASMNFIND` stay out of scope with assembler.

### BETWEEN — select records between labels (inclusive)

Selection stage (Author’s Edition 1.1.11 pp.272–273). Verb `BETWEEN` is
all-caps (`betwee` is not an abbreviation). Syntax:
`BETWEEN [ANYcase] dstring (number | dstring)`. Match is a prefix
(“begins with”). The group **includes** the start and end label records.
A count must be 2 or larger. Selected groups go to primary; records
before, between, and after groups go to secondary. `ANYCASE` min 3;
Notes synonyms `CASEANY` / `IGNORECASE` / `CASEIGNORE`. Column ranges
are not in the railroad. Portable ASCII: hex/binary dstrings are raw
bytes (the book’s `xf1f2f3` sample is EBCDIC).

```text
pipe "(ENDCHAR ?) examples/emit.rexx a start b c end d | B: between /start/ /end/ | console ? B: | console"
```

Output:
```text
a
start
b
c
end
d
```
Both streams write to the terminal.

```text
pipe "examples/emit.rexx a x 1 x b x 2 x c | between /x/ /x/ | console"
```

Output:
```text
x
1
x
x
2
x
```
Repeated ranges with same pattern.

```text
pipe "examples/emit.rexx a start b c d | between /start/ /end/ | console"
```

Output:
```text
start
b
c
d
```
Range never closes, runs to EOF.

```text
pipe "examples/emit.rexx a start b c d e | between /start/ 3 | console"
```

Output:
```text
start
b
c
```
Numeric count: 3 records from the start match.

```text
pipe "(ENDCHAR ?) examples/emit.rexx a START b END c | B: between anycase /start/ /end/ | console ? B: | console"
```

Output:
```text
a
START
b
END
c
```
Both streams write to the terminal.

```text
pipe "examples/emit.rexx :xmp.one body :exmp. after | split | between /:xmp./ /:exmp./ | console"
```

Output:
```text
:xmp.one
body
:exmp.
```
p.273 Script tags; split for separate records.

```text
pipe "literal a | between 1-4 /x/ | hole"
```

Error:
```text
PIPE: BETWEEN: unrecognized operand 1-4
```

```text
pipe "literal a | between /x/ 1 | hole"
```

Error:
```text
PIPE: BETWEEN: count must be at least 2
```

```text
pipe "literal a | betwee /a/ /b/ | hole"
```

Error:
```text
PIPE: unknown stage: BETWEE
```

### INSIDE — select records between labels (exclusive)

Selection stage (Author’s Edition 1.1.11 p.393). Verb `INSIDE` is
all-caps. Same operand order as BETWEEN. The group **excludes** the
start and end labels (Note 1 p.394). A count must be non-negative
(0 selects nothing after a start label). Converse `NOTINSIDE`.

```text
pipe "(ENDCHAR ?) examples/emit.rexx a start b c end d | I: inside /start/ /end/ | console ? I: | console"
```

Output:
```text
a
start
b
c
end
d
```
Both streams write to the terminal.

```text
pipe "examples/emit.rexx :xmp. body :exmp. after | split | inside /:xmp./ /:exmp./ | console"
```

Output:
```text
body
```
p.394: discard the example tags.

```text
pipe "examples/emit.rexx a x 1 x b | inside /x/ /x/ | console"
```

Output:
```text
1
```

```text
pipe "examples/emit.rexx a start b c | inside /start/ 1 | console"
```

Output:
```text
b
```

```text
pipe "examples/emit.rexx a start b | inside /start/ 0 | console"
```

Output: *(no records)*

```text
pipe "literal a | inside /x/ -1 | hole"
```

Error:
```text
PIPE: INSIDE: unrecognized operand -1
```

```text
pipe "literal a | insid /a/ /b/ | hole"
```

Error:
```text
PIPE: unknown stage: INSID
```

### NOTINSIDE — converse of INSIDE

Selection stage (Author’s Edition 1.1.11 pp.442–443). Diagram
`NOTINSIDe` (min 7: `notinsi` works, `notins` does not). Synonym
`NINSIDE`. Keeps the label records; discards the interior (or copies
it to secondary).

```text
pipe "examples/emit.rexx :xmp. body :exmp. after | split | notinside /:xmp./ /:exmp./ | console"
```

Output:
```text
:xmp.
:exmp.
after
```
p.443: retain the tags.

```text
pipe "examples/emit.rexx a start b end d | ninside /start/ /end/ | console"
```

Output:
```text
a
start
end
d
```

```text
pipe "examples/emit.rexx a start b end | notinsi /start/ /end/ | console"
```

Output:
```text
a
start
end
```

```text
pipe "literal a | notins /start/ /end/ | hole"
```

Error:
```text
PIPE: unknown stage: NOTINS
```

### OUTSIDE — converse of BETWEEN

Selection stage (Author’s Edition 1.1.11 pp.445–446). Verb `OUTSIDE`
is all-caps. Inclusive labels (the BETWEEN groups) go to secondary;
everything else to primary. Count must be 2 or larger. The book’s
`mctoasa | outside /1/ 2` heading example is CMS ASA carriage control;
the lines below use an ASCII `1` prefix.

```text
pipe "(ENDCHAR ?) examples/emit.rexx a start b end d | O: outside /start/ /end/ | console ? O: | console"
```

Output:
```text
start
b
end
a
d
```
Both streams write to the terminal.

```text
pipe "examples/emit.rexx 1heading rest body 1next more body2 | split | outside /1/ 2 | console"
```

Output:
```text
body
body2
```
p.446 two-line headings.

```text
pipe "literal a | outside /x/ 1 | hole"
```

Error:
```text
PIPE: OUTSIDE: count must be at least 2
```

```text
pipe "literal a | outsid /1/ 2 | hole"
```

Error:
```text
PIPE: unknown stage: OUTSID
```

### Cascade (Phase 1 exit criteria)

```text
pipe "disk examples/lines.txt | take 20 | nlocate /x/ | strip | console"
```

Output:
```text
apple
banana
apricot
spaced
Xy
a.b.
todo: one
todo: two
done: three

blank-above
```
Record 10 is empty (a blank line).

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
```

Output:
```text
>>>hello
```

```text
pipe "examples/emit.rexx apple banana apricot | examples/filter.rexx ap | console"
```

Output:
```text
apple
apricot
```
