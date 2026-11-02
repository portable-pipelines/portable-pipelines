# Named stream identifiers

**Status:** implemented  
**Audience:** implementers of stream selection and REXX pipeline commands  
**Authority:** Author's Edition 1.1.11 stream identifiers on FANOUT, FANIN, DEAL, GATHER, SELECT, ADDSTREAM, and STREAMNUM. Labels (`L:`) stay a separate mechanism.

## What a name is

A stream is still a number (`StreamId`). A name is a word on one stage that resolves to that number.

Reserved words (any case):

- `PRIMARY` is 0
- `SECONDARY` is 1
- `TERTIARY` is 2

User names are case-insensitive. They live in a per-stage map (`StreamNames` on `StageCtx`, and a copy on a Regina helper host that has no live `StageCtx`).

## How a name is defined

1. **ADDSTREAM name** creates the next unused stream (same numbering as a bare ADDSTREAM) and binds the word. The return code is that stream number. A second ADDSTREAM of the same name, or ADDSTREAM of PRIMARY / SECONDARY / TERTIARY, returns RC -178. Renaming PRIMARY is out of scope.
2. **FANOUT** and **FANIN** identifier lists define each new name on first use. The name is bound to the lowest stream number not already taken by that list or by a name already on the stage. `FANOUT LEFT RIGHT` writes to streams 0 and 1. `FANIN 1 SPARE` reads stream 1, then stream 0 (SPARE). The same name twice in one list is `duplicate stream identifier NAME`.

SELECT, STREAMNUM, STREAMSTATE, DEAL SECONDARY / STREAMID, and GATHER STREAMID only look names up. An unknown word is `unknown stream identifier NAME` on those stages, or the existing command return code (SELECT 4, STREAMNUM -102, STREAMSTATE -4).

Numeric forms and every previous return code for a number, a missing operand, or extra words stay the same.

## CALLPIPE scoping

A nested SELECT FOO does **not** see the caller's FOO.

Names belong to the stage that defined them. CALLPIPE builds a new pipeline. Each nested stage starts with an empty map (reserved words still work). The caller keeps its own map, so SELECT FOO after ADDSTREAM FOO on that same stage still works.

This is the simplest safe rule: no inheritance, and names do not travel along connections. Labels (`L:`) already attach another occurrence of a stage as stream 1, 2, … and are unchanged.

CALLPIPE and ADDPIPE connectors accept `*.input.n:`, `*.output.n:`, PRIMARY / SECONDARY / TERTIARY, and a name defined on the caller (`*.output.FOO:`). The name is resolved on the issuing stage. See [ADDPIPE topology](addpipe-topology.md).

## Helper process

SELECT stays local in the Regina helper (it changes the stream number on later I/O). ADDSTREAM still sends a number to the parent. The helper binds the name in its own map after the parent returns the stream number. No new protocol message.

`PIPE_REGINA_INPROC=1` uses the buffered host map. That fallback still drains, then runs, then flushes.

## What is still numeric-only or deferred

- FANINANY has no identifier list (STRICT only).
- Python stages stay single-stream.
- Renaming PRIMARY / SECONDARY / TERTIARY.

Those limits are also listed in [docs/STATUS.md](../STATUS.md) and [docs/design/regina-out-of-process.md](regina-out-of-process.md).
