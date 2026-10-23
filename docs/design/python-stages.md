# Python-language stages (non-IBM extension)

**Status:** implemented (v1 subset)  
**Audience:** implementers of `pipe-python`  
**Authority:** this is **not** an IBM built-in. IBM REXX filters and ADDRESS `PIPELINE` commands stay the semantic model to mirror (Author’s Edition 1.1.11 Chapter 6 and Chapter 21). Built-in stage verbs remain IBM-only; see [`../builtins-roadmap.md`](../builtins-roadmap.md).

---

## 1. Goal and scope

Offer **Python as an alternative language for user-written stages**, the same role REXX filters play today: a program that sits in a PIPE specification, reads and writes **records** through the dispatcher, and never calls neighbouring stages.

It is **not**:

- a new Appendix A / Chapter 19 verb
- a replacement for `COMMAND` (host process runner)
- a reason to register `PYTHON` in `StageRegistry` (`crates/pipe-stages/src/registry.rs`)

In-scope for v1: resolve a `.py` path as a stage, spawn the system interpreter with a helper package, expose a small Python API that mirrors IBM pipeline commands, and map each call onto existing `StageCtx` services (`read` / `write` / `short`). Out of scope until later: a `PYTHON` keyword, embedding CPython, shipping a Python runtime in the Windows `pipe.exe` zip, STEM/VAR-style host variable pools, multi-stream Python, and any invented IBM-looking stage name.

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

Python plugs in the same way: a path-shaped verb, a hook installed by the CLI, a host that only calls `StageCtx`. It must **not** grow a second dispatcher. There is no `PYTHON` keyword and no PIPCMD `PYTHON` command in v1.

---

## 2. User-facing model

### Invocation

**v1 form: a `.py` path as the stage verb**, matching `.rexx` / `.rex`.

```text
pipe "literal hello | examples/upper.py | console"
pipe "examples/emit.py a b c | examples/upper.py | console"
```

Keep `.py` case-insensitive on the suffix (same as `to_ascii_lowercase().ends_with(".rexx")`). Do not treat every `.py` token as a stage if a built-in already matched: registry wins first, as with REXX. A bare `upper.py` (no slash) must keep its original spelling in the parser, the same as `.rexx`.

Script search is the same as REXX filters: the path as written, relative to the process working directory. REXX today only checks that the path exists (`pipe_rexx::find_script`); Python does the same. No extra search path in v1.

Stage arguments are the remainder of the stage, blank-separated, passed into the script the way `PARSE ARG` / `ARG.n` work for REXX (`run_rexx_stage`’s `stage_args`).

Missing interpreter: fail the stage with a discovery hint (see §5). Tests skip instead of failing unless `PIPE_REQUIRE_PYTHON=1` (see §6).

### Example programs

Records are **bytes**. Matching `pipe "…"` lines assume the repo-root work directory used by `tests/pipelines` fixtures.

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
    if rec is None:           # also p.rc == 12
        break
    rec = p.readto()          # consume
    p.output(b"[" + rec + b"]")
```

```text
pipe "literal hi | examples/bracket.py | console"
```

**Producer** (`examples/emit.py` — analogue of `examples/emit.rexx`):

```python
import pipelines as p

for a in p.args:
    p.output(a)
```

```text
pipe "examples/emit.py a b c | console"
```

**Word counts** (`examples/wordfreq.py`): reads records, counts words longer than 3, writes the N most common.

**Fetch a URL** (`examples/webget.py`): each line of the response is a record. Optional `truststore` so HTTPS inspection on Windows can use the system certificate store.

**`print()` vs `output()`** (`examples/printtest.py`): `print()` goes to the terminal (stderr); only `output()` writes records.

`.rexx` and `.py` stages can be mixed in one pipeline (`examples/emit.rexx a b c | examples/upper.py`).

### Debugging a Python stage

Python stages run as separate processes (`import pipelines as p`, `p.records()`, `p.output()`, and the rest). There is no REXX-style `TRACE ?R`.

Quick options:

- `print()` writes to stderr and never becomes a pipeline record.
- For large inputs, send `logging` to a file.
- Keep record logic in plain functions you can unit-test outside the pipeline.

Breakpoint debugging with **debugpy** is the recommended way. Install it into the interpreter `py` launches (the current folder does not matter). If `PIPE_PYTHON` points at a venv, install into that venv:

```text
py -m pip install debugpy
```

Add this near the top of the stage, after imports:

```python
import debugpy
debugpy.listen(5678)
print("waiting for debugger on 5678")
debugpy.wait_for_client()
breakpoint()
```

One-time VS Code setup: install Microsoft's Python extension (it includes Python Debugger). Open Run and Debug (Ctrl+Shift+D), choose "create a launch.json file", then Python Debugger, Remote Attach. Use this config:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "name": "Python Debugger: Remote Attach",
      "type": "debugpy",
      "request": "attach",
      "connect": { "host": "localhost", "port": 5678 }
    }
  ]
}
```

Two pitfalls in the generated config:

- `"host": "localhost:5678"` fails with `getaddrinfo ENOTFOUND localhost:5678`. Host must be just `localhost`; the port is a separate field.
- The generated `pathMappings` block (`remoteRoot` ".") is for remote machines. Remove it for local debugging.

Then start the pipe and wait for `waiting for debugger on 5678` (the stage waits indefinitely). Press F5 in VS Code. Execution stops at `breakpoint()`. Step through the `p.records()` loop. The rest of the pipeline waits while the stage is paused; Continue lets it finish normally. A Windows Firewall prompt can be denied; localhost still works.

Remove the debugpy lines when you are done. `wait_for_client` blocks forever without a debugger.

`pdb` is not recommended. At spawn, the child's stdin and stdout are the framed protocol pipes to the host. The helper then points fd 0 / `sys.stdin` at the null device and rebinds fd 1 / `sys.stdout` to stderr so `print()` cannot corrupt frames. `pdb` reads commands from `sys.stdin`, which has nothing to read after that rebind.

This workflow was checked on Windows 11 with Python 3.15 and VS Code.

---

## 3. The Python API (`pipelines`)

Ship a tiny stdlib-style module named **`pipelines`**. On Windows it lives in a `pipelines` folder next to `pipe.exe` (see §7). The host finds that package and prepends its parent to the child's `PYTHONPATH` (keeps any existing value; platform path separator) so user scripts write `import pipelines as p`. Search order: `PIPE_PYTHONPATH` if set, then a `pipelines` folder next to the executable, then `env!("CARGO_MANIFEST_DIR")/python` when that path still exists on disk (so `.\target\release\pipe.exe` from a repo clone works with no env var). A miss fails the stage and names the places looked. It is a host-facing façade, not a pip package the user installs.

Functions use **IBM command names in lowercase** so a REXX filter and a Python filter read as the same protocol. Do not invent `writeto`.

| Python | IBM command (Ch. 21) | Dispatcher / host |
|---|---|---|
| `readto() -> bytes \| None` | `READTO` p.650 | `StageCtx::read`. `None` and `rc = 12` at end of file. No exception. `PipeError::Severed` → `rc = 12` (PIPCMD’s mapping). |
| `peekto() -> bytes \| None` | `PEEKTO` p.649 (REXX still **planned**) | Wait until the selected input has a record or EOF (`input_record_ready` / `input_at_eof` / `wait_any_input`), then `StageCtx::peek()` — **do not consume**. `StageCtx::peek` itself does not wait; the host must. `None` and `rc = 12` at end of file. No exception. |
| `output(data: bytes \| str) -> None` | `OUTPUT` p.648 | `StageCtx::write`. `str` is encoded UTF-8 with `surrogateescape` (no `TypeError`). Unconnected output is a no-op (`WriteOutcome::NoConnection`); severed → `rc = 12`. |
| `short() -> None` | `SHORT` p.656 | `StageCtx::short` (selected in and out). |
| `rc` (int, last command) | REXX `RC` special | After each call update a module global. |
| `exit(code=0)` | REXX `EXIT n` | `StageCtx::set_return_code` then child `EXIT` frame. Must affect the PIPE return code the same way REXX `EXIT n` does (see Decisions). |
| `args: list[str]`, `arg: str` | `PARSE ARG` / `ARG.n` | Remainder after the script path. |
| `records()` | (sugar) | Iterator: repeated `readto` until `None` / `rc == 12`. `for rec in records():`. |

v2 (not in this increment): `select_input` / `select_output`, `commit`, `callpipe` / `addpipe`, `streamnum` / `streamstate`.

Uncaught Python exception → host `StageCtx::fail` (pipeline abort), not a silent RC. That matches `rexx_stage_stub` calling `ctx.fail` when the hook returns `Err`. The fail path must carry CMS stage context (`... Issued from stage N of pipeline M`).

### Bytes vs `str`

`pipe_core::Record` is a byte string (`crates/pipe-core/src/record.rs`). Host file stages already treat the OS stream as bytes: they split on LF and strip a single CR immediately before that LF. REXX currently goes through `Record::to_string_lossy` in `RexxPipeHost::readto` and Regina drain — **lossy**. Python must not repeat that as the default.

- **Default API is `bytes`.** `readto` / `peekto` return `bytes` (or `None`).
- **`output` accepts `bytes` or `str`.** `str` is encoded UTF-8 with `surrogateescape` (PEP 383) so non-UTF-8 code points that arrived as text can round-trip. Do not default to `latin-1` (hides encoding bugs) or strict UTF-8 (raises on the binary files this port already passes through).
- Optional `readto_text()` / `output_text(s)` are cheap aliases of the same encoding; they are not required for v1 if `output(str)` plus `.decode('utf-8', 'surrogateescape')` is documented.

### Pythonic sugar

Yes, in v1, as a **thin extra**, not a second I/O model:

```python
for rec in p.records():      # repeated readto until rc == 12
    ...
```

Iterator must call the same `readto` path (live wait, one record). Do not slurp the stream up front. No implicit `output` of the loop’s last expression.

---

## 4. Architecture options

The dispatcher is single-threaded stackful coroutines (`corosensei`). `StageCtx::read` / `write` / `commit` **suspend** the stage (`WaitReason`) instead of blocking the OS thread. Language runtimes that occupy the same C stack cannot yield that coroutine.

That is already why Regina is buffered: `run_rexx_stage` comments that `RexxStart` must never suspend a dispatcher coroutine, so Regina stage I/O drains input, runs `RexxStart`, then flushes output. The subset interpreter *can* be live because `exec_simple` calls `RexxPipeHost` on the coroutine stack between statements.

### A — Out-of-process interpreter (v1)

Spawn the system interpreter once per Python stage. Talk a framed binary protocol over duplicated stdin/stdout handles (not an extra pipe: `std::process::Command` cannot pass one cleanly on Windows). Host loop:

1. Read one request frame from the child (Python is computing or about to I/O).
2. Map it to `StageCtx` — **this** is where `read`/`write` may suspend.
3. Write one response frame. Repeat until `EXIT` or child death.

While the child is in Python bytecode, the host is in a blocking read of the next frame: same occupancy as the subset interpreter running `TRANSLATE`. Other stages do not need to run until this stage hits dispatcher I/O. When it does, the child’s stack is in another process, so suspend is safe — **unlike Regina / PyO3 callbacks**.

**Windows zip:** keep shipping `pipe.exe` without a CPython bundle. Ship the `pipelines` helper package in a `pipelines` folder next to `pipe.exe`. Discovery of the interpreter (first hit wins):

1. `PIPE_PYTHON` (file, or directory containing `python.exe` / `python3`)
2. Windows **py launcher**: `py -3 -c "import sys; print(sys.executable)"`
3. `python3` on `PATH`
4. `python` on `PATH`

On Unix, step 2 is skipped. Skip macOS `/usr/bin/python3` if `python3 -c "import sys"` fails (the Command Line Tools stub). Do not prefer bare `python` ahead of `python3` on Unix.

Minimum version is **Python 3.10**. An older interpreter fails the stage with a clear message that names the version found.

**Cost per record:** one request/response (a few syscalls). Fine for filters; not for millions of tiny records. Batching is a later optimisation, not v1.

**Peek / blocking:** live protocol can implement `PEEKTO` correctly (wait, don’t consume). A Regina-style drain-all buffer can *fake* peek on a vector but has already pulled every upstream record — `SHORT` and producer delay then diverge from IBM. v1 is **live**, not buffered.

**Multi-stream deadlock:** same CMS rules (one record in flight per connection). Extra risk vs REXX: if the host blocked on the protocol *while holding* a `write` wait, a peer Python stage could stall. Avoid by never reading the next child request from inside `StageCtx::write`’s wait path — only after the response is sent. Two Python stages that wait on each other deadlock exactly like two built-ins.

### B — Embed CPython via PyO3 (not v1)

`PyO3` would link `python3.dll` / `libpython` and call the script in-process. Callbacks for `readto` would re-enter Rust on the Python C stack — **same constraint as Regina**. Live suspend would corrupt or deadlock the interpreter unless every `StageCtx` wait hops to a worker thread, which is option A with extra GIL pain.

**Windows packaging:** the zip would need a matching CPython, or users must have a dev install plus import libs. Feature-gating does not help a prebuilt `pipe.exe`.

### Recommendation

**Option A for v1.** The code already shows that in-process language runtimes cannot interleave with `corosensei`. Out-of-process Python gets live I/O, keeps the zip free of a CPython bundle, and avoids a `libpython` feature matrix. Revisit B only if a later phase needs in-process speed *and* a proven GIL/suspend design.

### macOS and Linux

Windows is the primary target (John runs `pipe.exe` on Windows 11 with Windows PowerShell 5.1). macOS and Linux are not precluded: option A is `Command` plus pipes on all three OSes. Differences that matter: binary stdio on Windows so protocol frames are not CR/LF-translated; `TerminateProcess` vs `SIGTERM`/`SIGKILL`; records stay LF-delimited bytes. This repo has no GitHub Actions today; one matrix job is enough when CI exists.

---

## 5. Protocol spec (option A)

**Transport.** Child stdin = host→child, stdout = child→host at spawn. stderr is **inherited**. Spawn with `python -u` (unbuffered). No extra pipe.

**Keep the record protocol off the user’s stdout.** At startup the helper:

1. Duplicates the original stdin/stdout handles for the protocol and marks those copies **non-inheritable**.
2. Rebinds fd 1 / `sys.stdout` to stderr so `print()` shows on the console like REXX `SAY`.
3. Points fd 0 at the null device so user code cannot read protocol bytes from `sys.stdin`.
4. On Windows, also resets the process standard handles (`SetStdHandle`) so a subprocess the script launches cannot write into the protocol.

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

**Errors.** Unknown type → `ERR` rc 8 (Regina subcom error rc) and `fail` if the child cannot continue. `PipeError::Aborted` → kill child, end. Uncaught Python exception: bootstrap sends `FAIL`; host `fail`s with stage context.

**Termination and cleanup.**

1. Normal: script returns → bootstrap sends `EXIT 0` (or `exit(n)`). Host `wait`s, then returns from the stage body.
2. Exception: bootstrap sends `FAIL`; host `fail`s.
3. Child crash / unexpected stdout EOF: `fail` with a clear stage error (no hang). Close pipes and `wait` so no zombies.
4. Pipeline abort: host `kill` (Windows `TerminateProcess`, Unix `SIGTERM` then `SIGKILL` after a short wait). Drop stdin to unblock a child stuck in `readto`.
5. Always close pipes and `wait`.

**Stderr passthrough.** Inherit the parent stderr fd. Do not frame it. Do not merge stdout. TRACE stays on the host (`PIPE TRACE:`); the child does not emit dispatcher trace.

**Python bootstrap.** Host equivalent of:

```text
$PIPE_PYTHON -u -m pipelines.stage -- /path/to/script.py [args…]
```

The host sets the child's `PYTHONPATH` to the parent of the `pipelines` package it found (`PIPE_PYTHONPATH`, next to the executable, or `crates/pipe-python/python` via `CARGO_MANIFEST_DIR`), prepended ahead of any existing `PYTHONPATH`.

---

## 6. Phased plan

### v1 — single-stream filters

**Ship:** `.py` path resolution (clone of `resolve_stage` / `rexx_stage_stub` / `set_rexx_hook`; no `PYTHON` keyword), spawn + hello, live `readto` / `peekto` / `output` / `short`, `rc`, `records()` iterator, bytes API plus `output(str)`, `PIPE_PYTHON` / `py -3` / `python3` / `python` discovery, stderr inherit via handle rebinding, `fail` on crash/exception, `set_return_code` on `exit`. Minimum Python 3.10.

**Not in v1:** `PYTHON` keyword, `select`, `commit`, `callpipe`, `streamnum`/`streamstate`, PyO3, a CPython bundle.

**Tests** (`tests/pipelines/python.pipefix` plus unit tests in `pipe-python`):

- basic filter (`upper.py`)
- `records()`
- peek-then-read does not duplicate or drop a record
- `output(str)` including non-ASCII
- `print()` does not corrupt pipeline output
- `exit(n)` PIPE RC matches REXX `EXIT n`
- two Python stages in one pipeline (same `.py` file twice)
- a Python stage in an `(ENDCHAR ?)` spec (single-stream position)
- a Python stage referenced by a label that would connect a secondary stream: clear error with `... Issued from stage N of pipeline M`
- Python syntax error / exception surfaces as a stage error
- Python older than 3.10: clear message naming the version found (mock if needed)

Pipefix cases that need an interpreter use `need: python3` so they skip cleanly when Python is absent. `PIPE_REQUIRE_PYTHON=1` makes those cases (and a dedicated unit test) fail instead of skip, mirroring `PIPE_REQUIRE_REGINA`.

`preflight_callpipe` / buffered CALLPIPE resolution already special-cases `.rexx` / `.rex` / `REXX`; v1 should allow `.py` the same way so a later `CALLPIPE` of a Python stage parses.

### v2 — multi-stream, select, commit, callpipe

`select_input`/`select_output`, optional `SELECT BOTH` / `ANYINPUT`, `commit`, `callpipe`/`addpipe` (buffering connectors, same as REXX), two-stream tee fixture with `(ENDCHAR ?)` labels.

### Later

`streamnum` / `streamstate` / `maxstream` / `sever`; optional batching; only then reconsider PyO3. STEM/VAR stay Phase 5 IBM work on the REXX side, not Python.

---

## 7. Decisions (v1)

Accepted for this increment:

1. **`.py` paths only.** No `PYTHON` keyword. It may come later. No `StageRegistry` entry and no PIPCMD `PYTHON` command.
2. **`output()` accepts `bytes` or `str`.** `str` is encoded UTF-8 with `surrogateescape`. No `TypeError`.
3. **`readto()` / `peekto()` return `None` at end of file.** No exception. They set `rc` 12.
4. **Minimum Python is 3.10.** An older interpreter fails the stage with a clear message that names the version found.
5. **Ship the `pipelines` helper package in a `pipelines` folder next to `pipe.exe`** in the Windows zip. `scripts/package-windows.ps1` must copy it. The script stays Windows PowerShell 5.1 compatible.
6. **Python `exit(n)` must affect the PIPE return code the same way REXX `EXIT n` does.** Finding (this increment): subset `EXIT n` used to write `host.rc` only (the last-command slot; a later `READTO` at EOF left `RC=12` and hid the exit), and Regina ignored `RexxStart` `retcode`. CMS (Author’s Edition 1.1.11 Chapter 6 / return codes p.219) treats the program return code as the stage return code, which the PIPE aggregate (most negative, else largest) then uses. The small fix was applied for both engines: distinguish `EXIT n` from last-command `RC` (`exit_rc`) and call `StageCtx::set_return_code(n)`. Python `exit(n)` / `sys.exit(n)` send protocol `EXIT` and use the same `set_return_code` path. No follow-up.
7. **`records()` iterator is in v1:** `for rec in records():`.
8. **Protocol stays off the user’s stdout** by duplicating the original stdin/stdout handles (non-inheritable), rebinding fd 1 / `sys.stdout` to stderr, pointing fd 0 at the null device, and on Windows resetting process standard handles with `SetStdHandle`. No extra pipe.

---

## 8. Pipeline topology and concurrency

Multiple Python stages per pipeline are supported. Each stage is its own Python process. The same `.py` file may appear more than once with different arguments.

Per-stage interpreter startup is tens of milliseconds on Windows, more with antivirus. That cost is paid once per stage, not once per record.

Python stages are allowed anywhere a **single-stream** stage fits in `ENDCHAR` multi-pipeline specs. Example that is valid in v1:

```text
pipe "(ENDCHAR ?) examples/emit.py a b | examples/upper.py | console ? literal z | console"
```

v1 is single-stream. A Python stage referenced by a label that would connect a secondary stream must fail with a clear error, including the CMS context line `... Issued from stage N of pipeline M`. Example that must fail:

```text
pipe "(ENDCHAR ?) examples/emit.py a b | T: examples/upper.py | console ? T: | console"
```

### Future work

- Multi-stream Python (select / secondary streams / labelled tee).
- Data exchange between pipelines and Python variables (STEM/VAR-like; for example running a pipeline from Python with list input and output).
- Free-threaded (no-GIL) Python: only relevant to a possible future in-process runtime, because each v1 stage is already a separate process.

---

## Implementation sketch

New crate `pipe-python` (no change to the IBM registry). `pipe-cli` calls `pipe_python::install_hook()` beside `pipe_rexx::install_hook()`. `pipe-stages` grows a `PythonHook` next to `RexxHook` in `run.rs` only, still not a built-in verb. Feature flags are unnecessary for v1 if the host just `Command::new`s an optional interpreter.
