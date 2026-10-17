# Pipeline examples

Run from the repo root so paths resolve:

```powershell
cargo run -p pipe-cli -- "literal hello | console"
# or, after a release build:
.\target\release\pipe.exe "literal hello | console"
```

Chained `literal a|literal b` only yields `b` (each LITERAL replaces the stream).
For several records, use `examples/emit.rexx`, `disk`, or `fanin` with parallel pipelines.

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

pipe "literal xxyzz | strip anyof /xy/ | console"
# → zz

pipe "literal aaa | strip leading anyof /a/ 2 | console"
# → a

pipe "literal AaA | strip anycase anyof /a/ | console"
# → (empty)

pipe "literal a b | strip not | console"
# → ␠  (NOT blank: strip non-blanks from both ends)

pipe "literal a.b. | strip trailing . | console"
# → a.b
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
| `chop.rexx` | `SUBSTR`, `LENGTH`, `STRIP` |
| `take.rexx` | REXX prototype; prefer built-in `take` |
| `wrap.rexx` | Multi-step assign, `TRANSLATE`, `\|\|` |
| `tee_say.rexx` | `SAY` to stderr + pass-through |
| `callpipe_demo.rexx` | `CALLPIPE` with `*:` connectors |

### Stage arguments

Arguments after the script path become `ARG.1`, `ARG.2`, … (`ARG.0` is the count):

```text
pipe "literal hello | examples/prefix.rexx >>> | console"
# → >>>hello

pipe "examples/emit.rexx apple banana apricot | examples/filter.rexx ap | console"
# → apple / apricot
```
