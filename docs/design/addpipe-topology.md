# ADDPIPE insert-between topology

**Status:** implemented  
**Audience:** implementers of nested pipelines and REXX pipeline commands  
**Authority:** ADDPIPE adds a specification to the running set (Author's Edition 1.1.11 Chapter 21). This note records Portable Pipelines topology. It is not IBM text.

## CALLPIPE versus ADDPIPE

Both commands spawn nested stages on the parent dispatcher.

| | CALLPIPE | ADDPIPE |
|---|---|---|
| When the caller continues | After every nested stage ends | As soon as the stages are spawned |
| Streams | Temporary loans; restored when the call ends | Permanent insert-between |
| Commit group | A new group, so a nested COMMIT does not wait on the parked caller | The caller's own group, so added stages join the same commit wave |
| STREAMSTATE | After restore, the caller sees its original connections | After ADDPIPE, STREAMSTATE is the live topology |

IF, NOT, and CASEI already use `spawn_stage` to add siblings. ADDPIPE uses the same spawn primitive, then attaches connectors.

PIPCMD, subset REXX, and a Regina helper share `pipeline_cmd`. The helper sends ADDPIPE as protocol type `0x1C` (same utf-8 specification payload as CALLPIPE). The parent spawns and replies `OK` without waiting. `PIPE_REGINA_INPROC=1` still drains, then runs, then flushes, including ADDPIPE.

## Insert-between

The calling stage stays on the path.

### Input connector (`*:`, `*.input:`, `*.input.n:`, `*.input.NAME:`)

Upstream writes into the nested pipeline first. The nested last stage writes into the caller's input. The caller keeps reading and sees what the nested pipeline produces.

That needs an extra connection, not only a retarget:

1. Retarget the existing reader from the caller onto the nested first stage.
2. Add a new connection from the nested last stage onto the caller's input stream.

### Output connector (`*:`, `*.output:`, `*.output.n:`, `*.output.NAME:`)

The caller writes into the nested pipeline first. The nested last stage writes to the original downstream. The caller keeps writing.

1. Retarget the existing writer from the caller onto the nested last stage.
2. Add a new connection from the caller's output onto the nested first stage.

### Both connectors (`*: | filter | *:`)

One single-stream filter cannot sit on both the input path and the output path at once. Input insert-between is applied first (the caller keeps reading). If the output half would reuse an already-connected nested end, that half is skipped and the caller's original output stays in place. A later OUTPUT still runs.

### Numbered and named streams

`*.input:` / `*.output:` / a bare `*:` use the caller's currently selected stream.

`*.input.n:` / `*.output.n:` use stream number `n`.

`PRIMARY`, `SECONDARY`, and `TERTIARY` are 0, 1, and 2.

`*.input.NAME:` / `*.output.NAME:` resolve `NAME` on the issuing stage (ADDSTREAM or a FANOUT / FANIN list). An unknown name is `unknown stream identifier NAME`. A nested SELECT FOO still does not see the caller's FOO. Names do not travel along connections.

The same resolution applies to CALLPIPE connectors.

### No connector

The nested specification joins the running set with no splice onto the caller. Stages that write and have no output connected end after a failed write, the same as a top-level stage in that position.

### When a splice cannot be completed

If the extra connection's nested end is already taken, that attachment is skipped. The caller is not donated. STREAMSTATE stays true: a skipped input insert leaves the original upstream; a skipped output insert leaves the original downstream.

## Stall hazards

A caller that keeps reading a stream that nothing will write waits forever until the dispatcher stall detector fires. The message is the existing plain-words stall text. The CLI exit status is 8.

Typical cases:

- Input insert of `*: | a: fanout | f: fanin | *: ? a: | f:` then a later read. One leftover record is copied through and does not stall. Three leftovers deadlocks FANOUT into FANIN without ELASTIC after the first record, so a second READTO (or PIPCMD waiting for the next command) waits to read a stream that will not be written again.
- Output insert of `literal hello | *:` then a later OUTPUT does not stall. LITERAL writes its argument, then copies remaining input (`write_then_copy`), so both hello and the later record appear. ELASTIC is not required on that path. A write stall needs a nested first stage that will not read (for example a FANOUT/FANIN deadlock on the output path). Do not claim a stall without running the variant.

STREAMSTATE after ADDPIPE reports the live connections (0 connected, 12 not connected). A donated or unconnected stream is 12, not a hang on READTO.

## What stays deferred

- `PIPE_REGINA_INPROC=1` still drains the whole stage, then runs, then flushes. That fallback is not a live ADDPIPE path.
- `SUSPEND` cannot yield the parent while Regina is in bytecode.
- Python stages stay single-stream and have no ADDPIPE.
- FANINANY has no identifier list.
- Renaming PRIMARY / SECONDARY / TERTIARY.

Those limits are also listed in [docs/STATUS.md](../STATUS.md) and [docs/design/regina-out-of-process.md](regina-out-of-process.md).
