# Python-language stages (non-IBM extension)

**Status:** proposed  
**Audience:** implementers of a later `pipe-python` crate  
**Authority:** this is **not** an IBM built-in. IBM REXX filters and ADDRESS `PIPELINE` commands stay the semantic model to mirror (Author’s Edition 1.1.11 Chapter 6 and Chapter 21). Built-in stage verbs remain IBM-only; see [`../builtins-roadmap.md`](../builtins-roadmap.md).

---

## 1. Goal and scope

Offer **Python as an alternative language for user-written stages**, the same role REXX filters play today: a program that sits in a PIPE specification, reads and writes **records** through the dispatcher, and never calls neighbouring stages.

It is **not**:

- a new Appendix A / Chapter 19 verb
- a replacement for `COMMAND` (host process runner)
- a reason to register `PYTHON` in `StageRegistry` (`crates/pipe-stages/src/registry.rs`)

In-scope for the feature family: resolve a user script as a stage, expose a small Python API that mirrors IBM pipeline commands, and map each call onto existing `StageCtx` services (`read` / `write` / `commit` / `short` / `callpipe` / stream select). Out of scope until later: embedding CPython, shipping a Python runtime in the Windows `pipe.exe` zip, STEM/VAR-style host variable pools, and any invented IBM-looking stage name.

### How REXX does this today (the pattern to copy)

| Piece | Where |
|---|---|
| Verb resolution | `pipe_stages::run::resolve_stage` — if `StageRegistry::create` misses, a verb ending in `.rexx` / `.rex` or equal to `REXX` becomes `rexx_stage_stub` |
| Weak hook | `pipe_stages::set_rexx_hook` / `RexxHook`; CLI calls `pipe_rexx::install_hook()` from `crates/pipe-cli/src/main.rs` |
| Runner | `pipe_rexx::run_rexx_stage` — path is the verb, or `args[0]` when the verb is `REXX` |
| Live host (subset) | `RexxPipeHost` in `crates/pipe-rexx/src/subcommands.rs`, talking to `StageCtx` |
| Buffered host (Regina) | same type in buffered mode; `run_rexx_stage` drains `ctx.read()` then `RexxStart` (`crates/pipe-rexx/src/regina/bridge.rs`) |
| Subset commands | `interp::exec_simple` (`READTO`, `OUTPUT`, `CALLPIPE`, `ADDPIPE`, `SHORT`, `SELECT INPUT/OUTPUT n`) |
| Regina commands | `dispatch_pipeline_command` (adds `COMMIT`; still no `PEEKTO`) |
| Nested PIPE | `pipe_rexx::callpipe_spec` → `pipe_stages::callpipe_from_spec` → `StageCtx::callpipe` |
| Fuller Chapter 21 map | `pipcmd::dispatch_command` (`OUTPUT`, `CALLPIPE`/`ADDPIPE`, `COMMIT`, `SELECT`, `SHORT`, `SEVER`, `STREAMNUM`, `STREAMSTATE`, …) |

Python should plug in the same way: a path-shaped verb, a hook installed by the CLI, a host that only calls `StageCtx`. It must **not** grow a second dispatcher.

---

## 2. User-facing model

### Invocation (recommendation)

**Canonical form: a `.py` path as the stage verb**, matching `.rexx` / `.rex`.

```text
pipe "literal hello | examples/upper.py | console"
pipe "examples/emit.py a b c | examples/filter.py ap | console"
```

Also accept an explicit **non-IBM** keyword `PYTHON` with the script path as the first operand, matching `REXX` in `resolve_stage` / `run_rexx_stage`:

```text
pipe "literal hello | python examples/upper.py | console"
```

`PYTHON` is resolver sugar only. It must not be `StageRegistry::register`’d, must not appear in the IBM done table, and PIPCMD must not grow a `PYTHON` command (PIPCMD already rejects REXX-only Chapter 21 verbs in `pipcmd::is_rexx_only`).

Prefer documenting the `.py` path. Keep `.py` case-insensitive on the suffix (same as `to_ascii_lowercase().ends_with(".rexx")`). Do not treat every `.py` token as a stage if a built-in already matched — registry wins first, as with REXX.

Stage arguments are the remainder of the stage, blank-separated, passed into the script the way `PARSE ARG` / `ARG.n` work for REXX (`run_rexx_stage`’s `stage_args`).

Missing interpreter: fail the stage with a discovery hint (see §5), analogous to “REXX script not found” / Regina `discovery_hint()`. Tests skip instead of failing (see §6).

### Example programs

Records are **bytes**. The snippets below use the bytes API from §3. Matching `pipe "…"` lines assume the repo-root work directory used by `tests/pipelines` fixtures.

**Simple filter** (`examples/upper.py` — analogue of `examples/upper.rexx`):

```python
import pipelines as p

for rec in p.records():
    p.output(rec.upper())
```

```text
pipe "literal hello | examples/upper.py | console"
```

**Peek / read / output loop** (no iterator sugar; explicit RC like REXX `IF RC = 12 THEN LEAVE`):

```python
import pipelines as p

while True:
    rec = p.peekto()          # does not consume
    if p.rc == 12:
        break
    rec = p.readto()          # consume
    p.output(b"[" + rec + b"]")
```

```text
pipe "literal hi | examples/bracket.py | console"
```

**Two-stream stage** (primary keeps the record; secondary gets a copy). Same label instance, IBM-style `(ENDCHAR ?)`:

```python
import pipelines as p

while True:
    rec = p.readto()
    if rec is None:
        break
    p.select_output(0)
    p.output(rec)
    p.select_output(1)
    p.output(rec)
```

```text
pipe "(ENDCHAR ?) examples/emit.rexx a b | T: examples/tee.py | console ? T: | console"
```

**CALLPIPE** (v2; analogue of `examples/callpipe_demo.rexx`). Do not `readto` first — a leading `*:` consumes remaining input, the same warning as the REXX sample:

```python
import pipelines as p

p.callpipe("*: | spec /[/ WORD 1 /]/ | *:")
```

```text
pipe "literal hello | examples/wrap.py | console"
```

---

## 3. The Python API (`pipelines`)

Ship a tiny stdlib-style module named **`pipelines`**, injected via `PYTHONPATH` (or `-m`) so user scripts write `import pipelines as p`. It is a host-facing façade, not a pip package the user installs.

Functions use **IBM command names in lowercase** so a REXX filter and a Python filter read as the same protocol. Do not invent `writeto`.

| Python | IBM command (Ch. 21) | Dispatcher / host |
|---|---|---|
| `readto() -> bytes \| None` | `READTO` p.650 | `RexxPipeHost::readto` → `StageCtx::read`. `None` and `rc = 12` at EOF; `PipeError::Severed` → `rc = 12` (PIPCMD’s mapping). |
| `peekto() -> bytes \| None` | `PEEKTO` p.649 (REXX still **planned**) | Wait until the selected input has a record or EOF (`input_record_ready` / `input_at_eof` / `wait_any_input`), then `StageCtx::peek()` — **do not consume**. `StageCtx::peek` itself does not wait; the host must. |
| `output(data: bytes) -> None` | `OUTPUT` p.648 | `RexxPipeHost::output` → `StageCtx::write`. Unconnected output is a no-op (`WriteOutcome::NoConnection`); severed → `rc = 12`. |
| `short() -> None` | `SHORT` p.656 | `StageCtx::short` (selected in and out). |
| `select_input(n: int)` / `select_output(n: int)` | `SELECT INPUT/OUTPUT n` p.654 | `StageCtx::select_input` / `select_output` (`StreamId`). v2 may add `BOTH` / `ANYINPUT` as in `pipcmd::cmd_select`. |
| `commit(level: int)` | `COMMIT` p.642 | `StageCtx::commit` (Regina path already; subset interp does not parse it in `exec_simple`). |
| `callpipe(spec: str)` | `CALLPIPE` p.641 | `callpipe_from_spec` (v1 nested PIPE is still **buffering** connectors; `docs/architecture.md`). |
| `addpipe(spec: str)` | `ADDPIPE` p.639 | v1 ≈ `callpipe`, same as `RexxPipeHost::addpipe`. |
| `streamnum(side, ident="*") -> int` | `STREAMNUM` p.657 | `pipcmd::cmd_streamnum` |
| `streamstate(side, ident=None) -> int` | `STREAMSTATE` p.658 | `pipcmd::cmd_streamstate` (not `ALL`; PIPCMD rejects that as REXX-only). |
| `rc` (int, last command) | REXX `RC` special | `RexxPipeHost.rc`; after each call update a module global. |
| `exit(code=0)` | REXX `EXIT n` | `StageCtx::set_return_code` then child `EXIT` frame. Today `run_rexx_stage` does **not** copy `host.rc` onto the stage; Python should, so `AGGRC` / process status work (`pipe_core::aggregate_return_codes`, CLI `ExitCode`). |
| `args: list[str]`, `arg: str` | `PARSE ARG` / `ARG.n` | Remainder after the script path. |

Uncaught Python exception → host `StageCtx::fail` (pipeline abort), not a silent RC. That matches `rexx_stage_stub` calling `ctx.fail` when the hook returns `Err`.

### Bytes vs `str`

`pipe_core::Record` is a byte string (`crates/pipe-core/src/record.rs`). Host file stages already treat the OS stream as bytes and only strip CR before LF (`docs/architecture.md`, “Host-file / device records”). REXX currently goes through `Record::to_string_lossy` in `RexxPipeHost::readto` and Regina drain — **lossy**. Python must not repeat that as the default.

- **Default API is `bytes`.** `output` accepts `bytes`; `readto` / `peekto` return `bytes`.
- **Optional text helpers**, not the core: `readto_text()` / `output_text(s)` using **UTF-8 with `surrogateescape`** (PEP 383) so non-UTF-8 records round-trip. Do not default to `latin-1` (hides encoding bugs) or strict UTF-8 (raises on the binary files this port already passes through).
- Passing `str` to `output` is a `TypeError` in v1 so scripts do not silently encode. Revisit if that is too sharp (open question).

### Pythonic sugar

Yes, as a **thin extra**, not a second I/O model:

```python
for rec in p.records():      # repeated readto until rc == 12
    ...
```

Iterator must call the same `readto` path (live wait, one record). Do not slurp the stream up front. No implicit `output` of the loop’s last expression.

---

## 4. Architecture options

The dispatcher is single-threaded stackful coroutines (`corosensei`). `StageCtx::read` / `write` / `commit` **suspend** the stage (`WaitReason`) instead of blocking the OS thread. Language runtimes that occupy the same C stack cannot yield that coroutine.

That is already why Regina is buffered: `run_rexx_stage` comments that `RexxStart` must never suspend a dispatcher coroutine; architecture.md records “Regina stage I/O | Buffered (drain input → RexxStart → flush output)”. The subset interpreter *can* be live because `exec_simple` calls `RexxPipeHost` on the coroutine stack between statements.

### A — Out-of-process `python3` (recommended for v1)

Spawn the system interpreter once per Python stage. Talk a framed binary protocol over the child’s stdin/stdout (or a dedicated pipe pair). Host loop:

1. Read one request frame from the child (Python is computing or about to I/O).
2. Map it to `StageCtx` — **this** is where `read`/`write`/`commit` may suspend.
3. Write one response frame. Repeat until `EXIT` or child death.

While the child is in Python bytecode, the host is in a blocking read of the next frame: same occupancy as the subset interpreter running `TRANSLATE`. Other stages do not need to run until this stage hits dispatcher I/O. When it does, the child’s stack is in another process, so suspend is safe — **unlike Regina / PyO3 callbacks**.

**Windows zip:** keep shipping `pipe.exe` only. Do not bundle CPython. Discovery (first hit wins):

1. `PIPE_PYTHON` (file, or directory containing `python.exe` / `python3`)
2. `python3` then `python` on `PATH`
3. Windows **py launcher**: `py -3 -c "import sys; print(sys.executable)"`

**Cost per record:** one request/response (a few syscalls). Fine for filters; not for millions of tiny records. Batching is a later optimisation, not v1.

**Peek / blocking:** live protocol can implement `PEEKTO` correctly (wait, don’t consume). A Regina-style drain-all buffer can *fake* peek on a vector but has already pulled every upstream record — `SHORT` and producer delay then diverge from IBM. v1 should be **live**, not buffered.

**Multi-stream deadlock:** same CMS rules (one record in flight per connection). Extra risk vs REXX: if the host blocked on the protocol *while holding* a `write` wait, a peer Python stage could stall. Avoid by never reading the next child request from inside `StageCtx::write`’s wait path — only after the response is sent. `CALLPIPE` (v2) runs the nested dispatcher on the host while the child waits on its stdin; that is idle, not nested-protocol. Two Python stages that wait on each other deadlock exactly like two built-ins.

### B — Embed CPython via PyO3 (optional cargo feature)

`PyO3` would link `python3.dll` / `libpython` and call the script in-process. Callbacks for `readto` would re-enter Rust on the Python C stack — **same constraint as Regina**. Live suspend would corrupt or deadlock the interpreter unless every `StageCtx` wait hops to a worker thread, which is option A with extra GIL pain.

**Windows packaging:** the zip would need a matching CPython, or users must have a dev install plus import libs. Feature-gating does not help a prebuilt `pipe.exe`.

**Performance:** cheaper per record if live callbacks worked; with forced buffering it is the Regina model again (no real `PEEKTO` / `SHORT` during the script).

### Recommendation

**Option A for v1.** The code already shows that in-process language runtimes cannot interleave with `corosensei` (`RexxPipeHost::buffered`, `run_rexx_stage` drain loop). Out-of-process Python gets live I/O, keeps the zip Python-free, and avoids a `libpython` feature matrix. Revisit B only if a later phase needs in-process speed *and* a proven GIL/suspend design.

### macOS and Linux

Windows stays the primary target. The host in §4 must not preclude macOS or Linux: option A is `Command` plus pipes on all three OSes. Discovery after `PIPE_PYTHON` (which is also how a venv is selected) is `python3` on `PATH` — Homebrew, python.org, or Xcode CLT on macOS; the distro package on Linux. Skip macOS `/usr/bin/python3` if `python3 -c "import sys"` fails (the Command Line Tools stub). Do not prefer bare `python` on Unix. Differences that matter: binary stdio on Windows so protocol frames are not CR/LF-translated; `TerminateProcess` vs `SIGTERM`/`SIGKILL`; records stay LF-delimited bytes. This repo has no GitHub Actions today; one matrix job is enough when the feature ships.

---

## 5. Protocol spec (option A)

**Transport.** Child stdin = host→child, stdout = child→host. stderr is **inherited** (passthrough: `SAY`-like `print` / trace). Spawn with `python -u` (unbuffered). Do not let user code write protocol bytes: at bootstrap, `pipelines` replaces `sys.stdout` with a wrapper that writes to stderr (or to the inherited stderr fd).

**Framing.** Little-endian, binary, not JSON (records may contain any byte including newline):

```text
u32le payload_len
u8    msg_type
u8[payload_len] payload
```

`payload_len` is the payload only (not counting the type byte). Max payload: 16 MiB in v1 (reject larger with `fail`). Empty payload is allowed (null record, `SHORT`, `READTO`).

**Hello (host → child, first message, type `0x01`).** Payload:

```text
u16le version=1
u32le flags=0
u32le path_len ; utf-8 path
u32le argc
repeat argc:
    u32le arg_len ; utf-8 arg
```

Child execs the script with `runpy.run_path` (or `compile` + `exec`) after installing `pipelines`. `sys.argv[0]` is the script path; `sys.argv[1:]` are stage args.

**Child → host (requests)**

| Type | Name | Payload | Host action |
|---|---|---|---|
| `0x10` | `READTO` | empty | `StageCtx::read`; reply `RECORD` or `EOF` |
| `0x11` | `PEEKTO` | empty | wait + `peek`; reply `RECORD` or `EOF` |
| `0x12` | `OUTPUT` | record bytes | `StageCtx::write`; reply `OK` (rc 0 or 12) |
| `0x13` | `SHORT` | empty | `StageCtx::short`; reply `OK` |
| `0x14` | `EXIT` | `i32le` rc | `set_return_code`; join child; end stage |
| `0x15` | `FAIL` | utf-8 message | `StageCtx::fail`; kill child |

v2 types (same frame layout): `SELECT_IN`/`SELECT_OUT` (`u32le`), `COMMIT` (`i32le`), `CALLPIPE` (utf-8 spec), `STREAMNUM`/`STREAMSTATE` (side + optional ident, utf-8).

**Host → child (replies)**

| Type | Name | Payload |
|---|---|---|
| `0x80` | `HELLO_OK` | empty (after hello) |
| `0x81` | `RECORD` | record bytes |
| `0x82` | `EOF` | empty (command rc 12) |
| `0x83` | `OK` | `i32le` rc |
| `0x84` | `ERR` | `i32le` rc + utf-8 message |

Protocol is **strictly request/response** after hello. The child never sends a second request until it has the reply. The host never sends unsolicited records.

**Errors.** Unknown type → `ERR` rc 8 (Regina subcom error rc) and `fail` if the child cannot continue. `PipeError::Aborted` → kill child, end. `PipeError::stage` / parse errors from `CALLPIPE` → `ERR` with the message; Python raises `pipelines.PipelineError`.

**Termination and cleanup.**

1. Normal: script returns → bootstrap sends `EXIT 0` (or `exit(n)`). Host `wait`s, then returns from the stage body.
2. Exception: bootstrap sends `FAIL`; host `fail`s.
3. Child crash / unexpected stdout EOF: `fail("python stage exited unexpectedly")`.
4. Pipeline abort: host `kill` (Windows `TerminateProcess`, Unix `SIGTERM` then `SIGKILL` after a short wait). Drop stdin to unblock a child stuck in `readto`.
5. Always close pipes and `wait` so no zombies.

**Stderr passthrough.** Inherit the parent stderr fd. Do not frame it. Do not merge stdout. TRACE stays on the host (`PIPE TRACE:`); the child does not emit dispatcher trace.

**Python bootstrap (sketch).** Host equivalent of:

```text
$PIPE_PYTHON -u -m pipelines.stage -- /path/to/script.py [args…]
```

with `PYTHONPATH` pointing at the bundled `crates/pipe-python/python` tree (or a copied share dir next to `pipe.exe` on Windows). The zip stays valid without that tree only if Python stages are unused; shipping the small `pipelines` package beside `pipe.exe` is enough and still is not a CPython bundle.

---

## 6. Phased plan

Sizes are order-of-magnitude new code (Rust host + Python module + tests), not calendar time.

### v1 — single-stream filters (~700–1 100 LOC)

**Ship:** `.py` / `PYTHON` resolution (clone of `resolve_stage` / `rexx_stage_stub` / `set_rexx_hook`), spawn + hello, live `readto` / `peekto` / `output` / `short`, `rc`, `records()` iterator, bytes API, `PIPE_PYTHON` discovery, stderr inherit, `fail` on crash/exception, `set_return_code` on `exit`.

**Not in v1:** `select`, `commit`, `callpipe`, `streamnum`/`streamstate`, PyO3, text helpers beyond a documented `surrogateescape` pair if cheap.

**Tests** (`tests/pipelines/python.pipefix` plus a few unit tests in the new crate):

- upper/filter/prefix analogues of `examples/*.rexx`
- peek-then-read does not duplicate or drop a record
- `short` after one record; upstream is not parked
- null record (`literal` with omitted argument)
- non-UTF-8 record round-trip (`x` hex dstring → Python `output` unchanged)
- missing script → error substring
- missing interpreter → **skip**, not fail

Extend the pipefix runner (`crates/pipe-cli/tests/pipeline_fixtures.rs`) with a **conditional** directive, e.g. `need: python3`, so the case runs when discovery succeeds and is counted skipped otherwise. Today `skip: reason` is always skip (`tests/pipelines/README.md`) — do not overload that into a silent ignore of real failures.

`preflight_callpipe` in `pipcmd.rs` already special-cases `.rexx` / `.rex` / `REXX`; v1 should allow `.py` the same way so a later `CALLPIPE` of a Python stage parses.

### v2 — multi-stream, select, commit, callpipe (~400–700 LOC more)

`select_input`/`select_output`, optional `SELECT BOTH` / `ANYINPUT`, `commit`, `callpipe`/`addpipe` (buffering connectors, same as REXX), two-stream tee fixture with `(ENDCHAR ?)` labels. Tests for select-then-read on stream 1, CALLPIPE `*: | spec … | *:`, and a documented stall if a script reads two streams without writing.

### Later

`streamnum` / `streamstate` / `maxstream` / `sever`; text helpers if not in v1; optional batching; only then reconsider PyO3. STEM/VAR stay Phase 5 IBM work on the REXX side, not Python.

---

## 7. Open questions

1. **Keep the `PYTHON` keyword** in v1, or only `.py` paths until someone needs the `REXX`-style spelling?
2. **`output(str)`:** `TypeError` (strict bytes) vs implicit `surrogateescape` encode?
3. **EOF style:** `None` + `rc == 12` (REXX-like, recommended), exception `pipelines.Eof`, or both (`readto(eof="none"|"raise")`)?
4. **Minimum Python:** 3.8 (Windows 10 / Server still common) vs 3.10 (`match`, `int | None` without `from __future__`)?
5. **Ship `pipelines/` next to `pipe.exe`** in the zip vs locating it via `PIPE_PYTHONPATH` / compile-time `include_str!` written to a temp dir?
6. **Should Python `exit(n)` become the PIPE process status** even though REXX `EXIT n` currently does not call `set_return_code`? (Recommendation: yes, and treat the REXX gap as a separate fix.)
7. **Iterator in v1** (`records()`) or examples-only loops until sugar has tests?
8. **Protocol on stdin/stdout vs an extra pipe** so naive `print()` cannot corrupt framing if bootstrap redirection fails?

---

## Implementation sketch (when this leaves docs)

New crate `pipe-python` (no change to IBM registry). `pipe-cli` calls `pipe_python::install_hook()` beside `pipe_rexx::install_hook()`. `pipe-stages` grows a `PythonHook` next to `RexxHook` (or one `LanguageHook` keyed by suffix) in `run.rs` only — still not a built-in verb. Feature flags are unnecessary for v1 if the host just `Command::new`s an optional interpreter.
