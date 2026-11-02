# Regina stages out of process

**Status:** implemented (live I/O and live CALLPIPE; STEM/VAR CALLPIPE still in-helper)  
**Audience:** implementers of `pipe-rexx`  
**Authority:** ADDRESS `PIPELINE` command names and return codes stay IBM (Author's Edition 1.1.11 Chapter 21). The helper process is a Portable Pipelines host detail, not an IBM program.

## Why

The dispatcher is single-threaded stackful coroutines. `StageCtx::read` / `write` suspend the stage. Regina's `RexxStart` occupies a C stack and is not safe to interleave with that suspend. The previous host drained every input record (`read_all`), ran `RexxStart`, then flushed output. That made an infinite producer hang, serialized two Regina stages on one process-global lock, and lost live SHORT/PEEKTO behaviour.

Python stages already avoid this by running in another process and talking a framed protocol so only the parent maps I/O onto `StageCtx`. Regina does the same, and CALLPIPE from that helper now reuses the parent dispatcher loans (the same machinery as a live in-process caller).

## Shape

1. Shared crate `pipe-helper`: frame codec, child spawn, and the parent drive loop. Python uses it unchanged from the caller's point of view (still `python -m pipelines.stage`, still one input and one output).
2. The parent relaunches `std::env::current_exe()` with hidden argv[1] `--internal-regina-helper`. There is no second executable. Windows spawn sets `CREATE_NO_WINDOW`.
3. The helper hosts Regina (`RexxStart`, ADDRESS `PIPELINE`, variable pool, EXIT). The parent drive loop maps `READTO` / `PEEKTO` / `OUTPUT` / `SHORT` / `CALLPIPE` / `COMMIT` / `ADDSTREAM` / `STREAMSTATE` / `WAIT_ANY` / `EXIT` / `FAIL` onto `StageCtx`. There is no `read_all()` on this path.
4. Every I/O message carries a `u32le` stream number (default 0). The helper honours the streams listed in `HELLO` (including stream 1+). Python still sends stream 0 only and still rejects a label that connects a secondary stream.
5. `CALLPIPE` / `ADDPIPE` issued inside the helper is hybrid:
   - Specs without a variable-pool stage run in the parent. The parent loans the helper stage's live streams to the nested pipeline (`StageCtx::callpipe`, the same path as subset REXX and PIPCMD). An infinite upstream plus downstream `take N` stops early.
   - Specs that name STEM, VAR, VARSET, VARFETCH, REXXVARS, VARLOAD, or VARDROP stay in the helper process next to Regina. The helper drains remaining input, runs the nested spec, then writes output back. Reverse variable-pool access (the parent reading Regina variables while `RexxStart` is mid-run) is not implemented.
6. `PIPE_REGINA_INPROC=1` keeps the old in-process drain path as a fallback.

The helper flag is not listed in `--help`. The CLI checks it before treating remaining argv as a specification.

## Protocol

Same layout as Python (`docs/design/python-stages.md` section 5):

```text
u32le payload_len
u8    msg_type
u8[payload_len] payload
```

I/O payloads (child to parent `READTO` `PEEKTO` `OUTPUT` `SHORT`, parent to child `RECORD` `EOF` `OK`) start with `u32le stream`. `SHORT` may add a side byte (`0` both, `1` input, `2` output, `3` sever output). Python sends stream 0 and omits the side byte, which still means both sides.

`HELLO` is unchanged for Python (version, flags, path, args). The Regina parent appends `u32le stage_num`, `u8 eof_report`, `u8 nocommit`, then the connected input stream list and the connected output stream list (`u32le` count plus ids). Python's decoder ignores trailing bytes.

Extra child-to-parent types used by Regina only:

| Type | Name | Payload |
|---|---|---|
| `0x16` | `CALLPIPE` | utf-8 specification |
| `0x17` | `COMMIT` | `i32le` level |
| `0x18` | `ADDSTREAM` | `u8` sides (bit 0 input, bit 1 output), optional `u32le` number |
| `0x19` | `STREAMSTATE` | `u8` side (1 input, 2 output), `u32le` stream |
| `0x1A` | `WAIT_ANY` | `u32le` count, then stream ids |

SELECT stays local in the helper (it changes the stream number on later I/O). SEVER / SHORT / COMMIT / ADDSTREAM / WAIT_ANY go to the parent.

The helper duplicates stdin/stdout for the protocol and points stdin at the null device. It points stdout at a clone of the parent process stdout (Unix fd 3, set with `pre_exec` before exec) so Regina `SAY` and a nested `CONSOLE` still write where in-process Regina wrote. Python stages are unchanged: `print()` still goes to stderr.

## Stall handling

The dispatcher has one stall detector (`idle_passes`, `friendly_stall_message`, `abort_and_drain`, process status 8). A helper CALLPIPE parks the caller as `waiting for CALLPIPE`, the same flag as PIPCMD and subset REXX. Nested FANOUT into FANIN, a circular wait on a stream the parked caller cannot write, and a leftover that is not restored, all go through that detector. There is no second watchdog.

While Regina is computing (no protocol frame yet) the parent drive loop blocks on the next frame. That is still a limit: `SUSPEND` cannot yield the dispatcher during bytecode. A `READTO` / `OUTPUT` / `CALLPIPE` that maps onto `StageCtx` suspends the helper stage as I/O, so `pick_next` does not busy-spin.

## Fallback

`PIPE_REGINA_INPROC=1` restores drain-then-`RexxStart` in the pipe process. Use it only when diagnosing a helper spawn problem. Nested `.rexx` under an active `RexxStart` still uses the subset interpreter (Regina 3.9.x hangs on re-entry).

Without Regina, `.rexx` stages still use the subset interpreter. Tests skip helper-only cases unless `PIPE_REQUIRE_REGINA=1`.

## Limits that stay

- Named stream identifiers (FANOUT lists, SELECT FOO) are still planned.
- ADDPIPE is still a CALLPIPE alias, not a true add to the running set.
- `SUSPEND` cannot yield the parent dispatcher while Regina is in bytecode; the parent is blocked on the next protocol frame.
- Variable-pool stages (STEM, VAR, and friends) still need the nested pipeline to run in the helper process next to Regina. A CALLPIPE that names those verbs stays on the buffered in-helper path. Reverse variable-pool access from a parent-loaned nested pipeline is deferred.
- Python stages remain single-stream (stream 0 only).

## Tests

Helper-only cases live in `crates/pipe-rexx/src/helper_tests.rs`:

- Infinite upstream, downstream TAKE N stops the pipeline (proves no drain).
- Live CALLPIPE `*: | xlate | *:` plus TAKE N on an infinite producer.
- TOLABEL leftover restored after a live CALLPIPE.
- STEM/VAR CALLPIPE still works (`varpool_demo.rexx`).
- SELECT / COMMIT / SEVER live, including SELECT of stream 1.
- FANOUT into FANIN through a helper CALLPIPE stalls with the plain-words message and `waiting for CALLPIPE`.
- Circular wait on a caller stream the parked helper cannot write.
- Nested COMMIT in a CALLPIPE child does not wait on the caller.
- Two Regina stages in one pipeline (series and labelled FANOUT).
- EXIT / SETRC.
- SHORT after one record.
- Helper abort yields "REXX helper exited unexpectedly".
- `PIPE_REGINA_INPROC=1` CALLPIPE still works.

They skip when Regina is missing, and fail if `PIPE_REQUIRE_REGINA=1` and Regina was not loaded.
