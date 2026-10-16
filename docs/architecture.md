# Architecture: CMS Pipelines → Rust

## Design rules (Hartmann)

1. **Stages never call each other.** They call dispatcher services on `StageCtx`: `read`, `write`, `commit`, `delay_record`, `short`, `callpipe`.
2. **Records**, not byte streams, are the unit of transfer (`pipe_core::Record`).
3. **At most one record in flight** per connection unless a stage explicitly buffers (`BUFFER`, `SORT`).
4. **Drain-biased scheduling:** consumers that can read are preferred over producers.
5. **Device drivers vs filters:** host/file/console live in driver stages; filters only transform records.

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

## Built-in stage checklist (v1)

| Stage | Status |
|---|---|
| LITERAL | done |
| CONSOLE / OUTPUT | done |
| FILE / DISK | done (line-oriented) |
| HOST / COMMAND / CMD | done |
| LOCATE | done (primary/secondary) |
| CHANGE | done (first occurrence) |
| COUNT | done |
| SORT | done |
| BUFFER | done |
| FANOUT / FANIN / FANINANY | done (subset) |
| HOLE / DROP | done |
| SPECS / SPEC | subset (WORD, literals, 1-*) |
| COPY | done |
| REXX / `*.rexx` (also `*.rex`) | done (subset interp) |
| Full IBM catalog (100+) | not yet |
| EBCDIC / 3270 / tape / CP | out of scope |

## Fidelity vs IBM

For features we implement, match **IBM CMS Pipelines** (Author’s Edition / z/VM reference) — names, record semantics, dispatcher rules. Do not invent alternate spellings (e.g. no `WRITETO`; the write subcommand is `OUTPUT`). Out-of-scope items (3270, tape, EBCDIC default, full stage catalog, …) stay omitted rather than approximated under a different name.

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

## Authoritative reference

Behavioral authority: **CMS/TSO Pipelines Author’s Edition** (SL26-0018-05), local PDF at `docs/reference/dfsc4a00.pdf` or `C:\zVM53\dfsc4a00.pdf`. See [`reference/authors-edition-map.md`](reference/authors-edition-map.md). The PDF is gitignored (IBM copyright).

## TRACE

`(TRACE)` in the options group, or CLI `--trace` / `-t`, enables `TraceConfig` on the dispatcher. Events: dispatcher start/end, stage start/end, READ/WRITE (length + short preview), COMMIT request/wait/proceed, SHORT, DELAY, CALLPIPE enter/exit. Output defaults to stderr with a `PIPE TRACE:` prefix; tests may use `TraceConfig::capture`.
