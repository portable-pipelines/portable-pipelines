# Regina stages out of process

**Status:** implemented  
**Audience:** implementers of `pipe-rexx`  
**Authority:** ADDRESS `PIPELINE` command names and return codes stay IBM (Author's Edition 1.1.11 Chapter 21). The helper process is a Portable Pipelines host detail, not an IBM program.

## Why

The dispatcher is single-threaded stackful coroutines. `StageCtx::read` / `write` suspend the stage. Regina's `RexxStart` occupies a C stack and is not safe to interleave with that suspend. The previous host drained every input record (`read_all`), ran `RexxStart`, then flushed output. That made an infinite producer hang, serialized two Regina stages on one process-global lock, and lost live SHORT/PEEKTO behaviour.

Python stages already avoid this by running in another process and talking a framed protocol so only the parent maps I/O onto `StageCtx`. Regina now does the same.

## Shape

1. Shared crate `pipe-helper`: frame codec, child spawn, and the parent drive loop. Python uses it unchanged from the caller's point of view (still `python -m pipelines.stage`, still one input and one output).
2. The parent relaunches `std::env::current_exe()` with hidden argv[1] `--internal-regina-helper`. There is no second executable. Windows spawn sets `CREATE_NO_WINDOW`.
3. The helper hosts Regina (`RexxStart`, ADDRESS `PIPELINE`, variable pool, EXIT). The parent drive loop is the same as Python: `READTO` / `PEEKTO` / `OUTPUT` / `SHORT` / `EXIT` / `FAIL` go live through `StageCtx`. There is no `read_all()` on this path.
4. Every I/O message carries a `u32le` stream number (default 0) so secondary streams can be added later. Only stream 0 is honoured. A label that connects stream 1 fails with a clear stage error.
5. `CALLPIPE` issued inside the helper still buffers: remaining input is drained into memory, the nested spec runs, then output is written back. That is a later increment (live CALLPIPE from Regina), not this one.
6. `PIPE_REGINA_INPROC=1` keeps the old in-process drain path as a fallback.

The helper flag is not listed in `--help`. The CLI checks it before treating remaining argv as a specification.

## Protocol

Same layout as Python (`docs/design/python-stages.md` section 5):

```text
u32le payload_len
u8    msg_type
u8[payload_len] payload
```

I/O payloads (child to parent `READTO` `PEEKTO` `OUTPUT` `SHORT`, parent to child `RECORD` `EOF` `OK`) start with `u32le stream`. `SHORT` may add a side byte (`0` both, `1` input, `2` output). Python sends stream 0 and omits the side byte, which still means both sides.

`HELLO` is unchanged for Python (version, flags, path, args). The Regina parent appends `u32le stage_num`, `u8 eof_report`, `u8 nocommit`. Python's decoder ignores trailing bytes.

The helper duplicates stdin/stdout for the protocol and points stdin at the null device. It points stdout at a clone of the parent process stdout (Unix fd 3, set with `pre_exec` before exec) so Regina `SAY` and a nested `CONSOLE` still write where in-process Regina wrote. Python stages are unchanged: `print()` still goes to stderr.

## Fallback

`PIPE_REGINA_INPROC=1` restores drain-then-`RexxStart` in the pipe process. Use it only when diagnosing a helper spawn problem. Nested `.rexx` under an active `RexxStart` still uses the subset interpreter (Regina 3.9.x hangs on re-entry).

Without Regina, `.rexx` stages still use the subset interpreter. Tests skip helper-only cases unless `PIPE_REQUIRE_REGINA=1`.

## Limits that stay

- Named stream identifiers and live I/O on stream 1+ are still planned.
- `CALLPIPE` / `ADDPIPE` from a Regina helper still buffer at the CALLPIPE (the rest of the stage streams).
- `SUSPEND` cannot yield the parent dispatcher while Regina is in bytecode; the parent is blocked on the next protocol frame.
- Variable pool stages (STEM, VAR, and friends) still work through CALLPIPE because that nested pipeline runs in the helper process next to Regina.

## Tests

Helper-only cases live in `crates/pipe-rexx/src/helper_tests.rs`:

- Infinite upstream, downstream TAKE N stops the pipeline (proves no drain).
- Two Regina stages in one pipeline (series and labelled FANOUT).
- EXIT / SETRC.
- SHORT after one record.
- Helper abort yields "REXX helper exited unexpectedly".

They skip when Regina is missing, and fail if `PIPE_REQUIRE_REGINA=1` and Regina was not loaded.
