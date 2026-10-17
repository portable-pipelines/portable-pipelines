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

Summary of IBM verbs registered today: LITERAL, CONSOLE, DISK (portable line I/O), COMMAND (portable process runner), LOCATE, NLOCATE (synonym NOTLOCATE), TAKE (subset), DROP (subset; not an alias of HOLE), STRIP (subset), BETWEEN (subset), SPLIT, CHANGE, COUNT, SORT, BUFFER, FANOUT/FANIN/FANINANY (subset), HOLE, SPEC (subset; keyword SPECS), COPY, REXX/`*.rexx`. Invented aliases HOST/CMD/FILE/OUTPUT-as-console were removed. EBCDIC/3270/tape/CP and other CMS-only Appendix A families stay out of scope.

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
| `/` string delimiter vs Windows flags | `/ee/` (both slashes in one token) is delimited; `/s` / `/b` are Words for `COMMAND`. Spaced patterns use `%foo bar%` (or another non-`/` delimiter), not `/foo bar/`. |
| `*` connector vs operand vs Windows globs | A lone `*` that is an entire stage is a CALLPIPE connector (a connector stage, `*:`, `*.input:`, or `*.output:`). A lone `*` after a stage verb is the operand star (`take *`, `drop last *`). Path/glob stars (`c:\*.*`, `*.json`) stay Words. |
| TAKE / DROP remainder | Record counts with `FIRST` (default), `LAST`, a number (default 1), `0`, and `*`. `BYTES` is rejected. The counted records and the others go to opposite streams (TAKE keeps the count on primary; DROP is the converse). After the count, the counted stream is severed once its in-flight record is consumed, then the remainder is copied. Our `short` severs; it does not splice the producer onto the other stream. If the remainder stream is not connected, the input is severed instead of draining the file. |
| NLOCATE null string | Same contains-subset as LOCATE, routes inverted. No operand (or an empty delimited string) selects only empty records. `NOTLOCATE` is the documented synonym. |
| STRIP targets | Character trim. Default is both ends, blank. `LEADING` / `TRAILING` / `BOTH`, `BLANK`, a one-character word, a delimited set, `ANYOF`, `NOT` / `TO`, `ANYCASE`, and an optional per-side count are implemented. `STRING` and hex xrange are rejected. |
| SPLIT | Full stage per Author's Edition 1.1.11 pp. 505–506. Operand order is fixed: `[ANYCase] [MINimum number] [AT \| [snumber] BEFORE \| [snumber] AFTER] [NOT] [BLANK \| target [number]]`. Target is an `xrange` (xorc, `xorc-xorc`, `xorc.number`), `STRing dstring`, or `ANYof dstring`; a one-character dstring may omit the keyword. Default target is blank. `AT` (default) discards the match; `BEFORE`/`AFTER` keep it, with signed offsets (`n AFTER` ≡ `m BEFORE` where `m = -n - length(target)`). Note 1: null input records are copied; generated null records are dropped. Abbreviations: `ANYCase` min 4 (`ANYof` wins at 3), `MINimum` min 3, `STRing` min 3; `CASEANY` / `IGNORECASE` / `CASEIGNORE` synonym `ANYCASE`. Portable ASCII: `BLANK`/`SPACE` xorc is X'20' (book cites EBCDIC X'40'); `TABULATE` xorc is ASCII HT X'09' (book cites EBCDIC X'05'). |
| BETWEEN operands | Requires two operands (plus optional ANYCASE before them). From first match of operand 1 to match of operand 2 (delimited string), or if operand 2 is numeric, select that many records starting from the match (inclusive). When both operands are the same pattern, enables repeated ranges in one input. Selected records go to primary output; others to secondary. Match is substring contains. No support for column ranges, bit masks, or XORC. OUTSIDE/INSIDE/NOTINSIDE remain in phase 6. |

## Authoritative reference

Behavioral authority: **CMS/TSO Pipelines Author’s Edition** (SL26-0018-05), local PDF at `docs/reference/dfsc4a00.pdf` or `C:\zVM53\dfsc4a00.pdf`. See [`reference/authors-edition-map.md`](reference/authors-edition-map.md). The PDF is gitignored (IBM copyright).

## TRACE

`(TRACE)` in the options group, or CLI `--trace` / `-t`, enables `TraceConfig` on the dispatcher. Events: dispatcher start/end, stage start/end, READ/WRITE (length + short preview), COMMIT request/wait/proceed, SHORT, DELAY, CALLPIPE enter/exit. Output defaults to stderr with a `PIPE TRACE:` prefix; tests may use `TraceConfig::capture`.
