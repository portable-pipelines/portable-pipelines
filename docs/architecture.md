# Architecture: CMS Pipelines → Rust

## Design rules (Hartmann)

1. **Stages never call each other.** They call dispatcher services on `StageCtx`: `read`, `write`, `commit`, `delay_record`, `short`, `callpipe`.
2. **Records**, not byte streams, are the unit of transfer (`pipe_core::Record`).
3. **At most one record in flight** per connection unless a stage explicitly buffers (`BUFFER`, `SORT`).
4. **Drain-biased scheduling:** consumers that can read are preferred over producers.
5. **Device drivers vs filters:** disk/command/console live in driver stages; filters only transform records.

## Type map

| CMS concept | Rust |
|---|---|
| Record | `pipe_core::Record` |
| Stream number | `pipe_core::StreamId` |
| Stage | coroutine running a `StageFn` / `FnOnce(&mut StageCtx)` |
| Dispatcher | `pipe_core::Dispatcher` via `PipelineBuilder::run` |
| PIPE specification | `pipe_parser::parse_pipeline` |
| Built-in program | factory in `pipe_stages::StageRegistry` |
| CALLPIPE | `StageCtx::callpipe` + connectors |
| REXX stage | `pipe_rexx` (`READTO` / `OUTPUT` / `CALLPIPE` / …) |

## Coroutine model

Each stage runs on a **stackful coroutine** (`corosensei`). When `read`/`write`/`commit` cannot complete, the stage **suspends**; the dispatcher resumes another ready stage. A thread-local holds the current `Yielder` so `StageCtx` stays easy to call.

## Multi-stream labels

```text
(ENDCHAR ?) literal a|literal b | L: locate /a/ | console ? L: | console
```

- First `L:` defines the stage and its primary streams.
- Later `L:` occurrences attach secondary/tertiary streams of that same stage instance.

## CALLPIPE / ADDPIPE

Nested specifications may start/end with IBM `*:` connectors (bare `*` still
accepted as a synonym):

- Leading `*:` — feed nested pipeline from the caller’s selected input.
- Trailing `*:` — drain nested output to the caller’s selected output.
- Also recognized: `*.input:`, `*.output:`.

v1 implements connector CALLPIPE in **buffering** mode (read all → run nested → write all), which is record-correct and sufficient for REXX filters. Streaming bridges can replace this later without changing the stage API.

## REXX / Regina

`pipe-rexx` runs `.rexx` stages with this preference order:

1. **Regina** (user-installed) — dynamically load `regina.dll` / `libregina.so` / `libregina.dylib`, register ADDRESS `PIPELINE`, `RexxStart` the script file.
2. **Subset interpreter** — if Regina is not found or cannot load.

Discovery env vars: `PIPE_REGINA`, `REGINA`, `REGINA_HOME` (file or directory). Also searches PATH, the `pipe` executable directory, and common install prefixes.

Pipeline subcommands use **IBM names only** (ADDRESS `PIPELINE` / subset statements):

| Subcommand | Meaning |
|---|---|
| `READTO var` | Read one record; RC=12 at EOF |
| `OUTPUT` | Write a record (command string after verb) |
| `CALLPIPE` | Nested pipeline |
| `ADDPIPE` | Attach nested network (v1 ≈ CALLPIPE) |
| `SHORT` | Sever selected streams |
| `SELECT INPUT/OUTPUT n` | Choose stream |
| `COMMIT n` | Commit level (Regina path) |
| `PEEKTO` | Not yet |

## Built-in stages

Living checklist (done / phase-N / out-of-scope): [`builtins-roadmap.md`](builtins-roadmap.md).

Summary of IBM verbs registered today: LITERAL, CONSOLE (synonym TERMINAL), DISK (portable line I/O), COMMAND (synonym CMD; portable process runner), LOCATE, NLOCATE (synonym NOTLOCATE), TAKE (subset), DROP (subset; not an alias of HOLE), STRIP (subset), BETWEEN (subset), SPLIT, JOIN, CHANGE, COUNT, SORT, BUFFER, FANOUT/FANIN/FANINANY, HOLE, SPEC (subset; keyword SPECS), COPY, REXX/`*.rexx`. Invented aliases HOST/FILE/OUTPUT-as-console were removed. EBCDIC/3270/tape/CP and other CMS-only Appendix A families stay out of scope.

## Fidelity vs IBM

For features we implement, match **IBM CMS Pipelines** (Author’s Edition / z/VM reference) — names, record semantics, dispatcher rules. Do not invent alternate spellings (e.g. no `WRITETO`; the write subcommand is `OUTPUT`). Out-of-scope items stay omitted rather than approximated under a different name; track them in [`builtins-roadmap.md`](builtins-roadmap.md).

## Ambiguities vs IBM

When behavior is unclear, we document the choice here and lock it with a test under `tests/compat/`.

| Topic | Choice |
|---|---|
| REXX write subcommand | IBM `OUTPUT` only (not `WRITETO`) |
| CALLPIPE connectors | IBM `*:` (buffering mode in v1; bare `*` synonym) |
| Regina ADDRESS name | `PIPELINE` (portable); on CMS the stage environment is unnamed |
| Regina stage I/O | Buffered (drain input → RexxStart → flush output) so Regina is never interleaved across coroutines |
| REXX stage arguments | IBM argument string via `PARSE ARG` (also exposed as `ARG.n` in the subset) |
| REXX subcommand quoting | Prefer IBM `'READTO VAR'` / `'OUTPUT' expr` so Regina does not substitute variables into the command |
| Operand scanning | The pipeline scanner only finds verbs, `\|`, labels, and ENDCHAR. A `\|` inside `/foo\|bar/` is still a stage separator (1.1.11 p.208): the book does **not** protect delimited strings at scan time. Two adjacent separators (`\|\|`) or end characters are self-escaping (pp.114–115). `ESCAPE` (no default) deletes the escape and forces the next character to be ordinary. `SEPARATOR` / `STAGESEP` and `ENDCHAR` take an xorc; `()`, `*`, `:`, `.`, and blank are reserved. Each stage then rescans `ParsedStage.raw_args` (`pipe_parser::scan_operands` / `operands_for_stage`). IBM filters treat `/ /` and `/foo bar/` as delimited strings and keep every blank. `COMMAND` word-splits so `/s` `/b` stay flags. `DISK` takes the remainder as a file name, so `/tmp/x.txt` is one path. `LITERAL` keeps the text after exactly one blank (p.412), including a blank that sits before `\|`. |
| `*` connector vs operand vs Windows globs | A lone `*` that is an entire stage is a CALLPIPE connector (a connector stage, `*:`, `*.input:`, or `*.output:`). A lone `*` after a stage verb is the operand star (`take *`, `drop last *`). Path/glob stars (`c:\*.*`, `*.json`) stay Words. |
| TAKE / DROP remainder | Record counts with `FIRST` (default), `LAST`, a number (default 1), `0`, and `*`. `BYTES` is rejected. The counted records and the others go to opposite streams (TAKE keeps the count on primary; DROP is the converse). After the count, the counted stream is severed once its in-flight record is consumed, then the remainder is copied. Our `short` severs; it does not splice the producer onto the other stream. If the remainder stream is not connected, the input is severed instead of draining the file. |
| NLOCATE null string | Same contains-subset as LOCATE, routes inverted. No operand (or an empty delimited string) selects only empty records. `NOTLOCATE` is the documented synonym. |
| STRIP targets | 1.1.11 pp.532–533. Character or string trim. Default is both ends, blank. `LEADING` / `TRAILING` / `BOTH`, `BLANK`, a one-character word, a delimited set, `ANYOF` (min 3), `STRING` (min 3), xrange (`xorc-xorc` / `xorc.n`), `NOT` / `TO`, `ANYCASE` (min 4; `ANYOF` wins at 3), and an optional per-side count are implemented. |
| SPLIT | Full stage per Author's Edition 1.1.11 pp. 505–506. Operand order is fixed: `[ANYCase] [MINimum number] [AT \| [snumber] BEFORE \| [snumber] AFTER] [NOT] [BLANK \| target [number]]`. Target is an `xrange` (xorc, `xorc-xorc`, `xorc.number`), `STRing dstring`, or `ANYof dstring`; a one-character dstring may omit the keyword. Default target is blank. `AT` (default) discards the match; `BEFORE`/`AFTER` keep it, with signed offsets (`n AFTER` ≡ `m BEFORE` where `m = -n - length(target)`). Note 1: null input records are copied; generated null records are dropped. Abbreviations: `ANYCase` min 4 (`ANYof` wins at 3), `MINimum` min 3, `STRing` min 3; `CASEANY` / `IGNORECASE` / `CASEIGNORE` synonym `ANYCASE`. Portable ASCII: `BLANK`/`SPACE` xorc is X'20' (book cites EBCDIC X'40'); `TABULATE` xorc is ASCII HT X'09' (book cites EBCDIC X'05'). |
| BETWEEN operands | 1.1.11 pp.272–273. Requires two operands (plus optional ANYCASE / CASEANY / IGNORECASE / CASEIGNORE before them). A group begins with a record that **begins with** the first string (prefix, not substring contains). The group ends at the next prefix match of the second string, or after a count of records (the count must be 2 or larger). Selected records go to primary output; others to secondary. Column ranges and bit masks are rejected. OUTSIDE/INSIDE/NOTINSIDE remain in phase 6. |
| LITERAL argument | 1.1.11 pp.412–413. The string starts after exactly one blank; extra blanks are kept. Omitted argument writes a null record, then input is copied (so `literal a\|literal b` yields `b` then `a`). A space before `\|` is a trailing blank in the argument. |
| CHANGE occurrences | 1.1.11 pp.286–288. Default is every occurrence (not first-only). A null first string prefixes once. `ANYCASE` uses the book’s case-of-match rule when the old string is lowercase. Invented `FIRST` is rejected. Column ranges stay subset. |
| COUNT default | 1.1.11 p.306 requires at least one keyword. Bare `COUNT` defaults to `LINES` so existing pipes keep working. `CHARACTERs` minimum abbreviation is 8; `CHARS` / `BYTES` are documented synonyms. Empty input: min length is 2G−1, max is 0. |
| CONSOLE placement | 1.1.11 pp.303–305. First vs not-first is “primary input connected?”, matching device-driver placement. `TERMINAL` is a synonym. CMS-only `DIRECT` / `DARK` / `INVISIBLE` / `ASYNCHRONOUSLY` are rejected. |
| DISK vs diskfast | 1.1.11 p.328. First stage reads; missing file is empty. Not-first appends/creates. `OUTPUT` / `APPEND` are portable stand-ins for `>` / `>>`, not IBM operands of `DISK`. CMS filemode syntax is out of scope. |
| FANIN / FANOUT / FANINANY | FANIN (p.338) reads connected inputs in order, or a numeric stream list. Named stream identifiers are rejected. FANINANY (p.339) uses `read_any`; `STRICT` is accepted. FANOUT (p.341) writes every connected output; `STOP ALLEOF` / `ANYEOF` / `n`. |
| COPY / HOLE operands | COPY (p.305) and HOLE (p.382) take no operands; extras are rejected. COPY consumes each record before writing (one-record delay potential). HOLE drains every connected input. |
| JOIN | Full stage per Author's Edition 1.1.11 pp. 399–401. Operand order is fixed: `[COUNT] [number \| * \| KEYLENgth number] [delimitedString [TERMinate]] [number]`. The first number is how many records are appended to the first of a set (default 1 = pairs; 0 = one-record sets; `*` = all). When a secondary input is connected and no number/`*`/`KEYLENGTH` is given, the default is unlimited. The second number is a maximum output length in bytes, exclusive of the COUNT prefix; it cannot be the first word. An input record at least that long is written unchanged (not truncated). `COUNT` prefixes a 10-character right-aligned contributor count. `KEYLENgth` (min 6) joins while the leading key matches and drops the key from records 2–n; a record shorter than the key uses the whole record as the key. `TERMinate` (min 4) appends the insert string after the last record of a set. Null input records are ordinary empty contributors, not set terminators; a set that contains only nulls writes a null record; no input writes nothing. Records go to the primary output only; a connected secondary output is rejected. Secondary input flushes the buffer and discards that record (EOF on primary is ignored while secondary is live). When both inputs have a record, primary is consumed first so a split-then-join rebuild can drain pieces before a fanout copy flushes. `TERMINATE` is applied at flush and is not included in the “would this input record exceed the maximum?” check. No documented synonyms; `ANYCASE` is rejected. JOIN 0 does not splice streams (our `short` severs). The book's blank insert `/ /` is a delimited string (see Operand scanning). |

## Authoritative reference

Behavioral authority: **CMS/TSO Pipelines Author’s Edition** (SL26-0018-05), local PDF at `docs/reference/dfsc4a00.pdf` or `C:\zVM53\dfsc4a00.pdf`. See [`reference/authors-edition-map.md`](reference/authors-edition-map.md). The PDF is gitignored (IBM copyright).

## TRACE

`(TRACE)` in the options group, or CLI `--trace` / `-t`, enables `TraceConfig` on the dispatcher. Events: dispatcher start/end, stage start/end, READ/WRITE (length + short preview), COMMIT request/wait/proceed, SHORT, DELAY, CALLPIPE enter/exit. Output defaults to stderr with a `PIPE TRACE:` prefix; tests may use `TraceConfig::capture`.
