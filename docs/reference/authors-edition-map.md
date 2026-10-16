# Author’s Edition → this Rust port

Use [`dfsc4a00.pdf`](dfsc4a00.pdf) (or `C:\zVM53\dfsc4a00.pdf`) as the behavioral authority when IBM docs and our code disagree. Record chosen interpretations in [`../architecture.md`](../architecture.md) plus a regression test.

## High-value sections for the port

| Topic | Approx. TOC location | Relevance |
|---|---|---|
| Building PIPE commands / drivers vs filters | Ch. 4 | Stage taxonomy |
| Multistream / stall / EOF backwards | Ch. 5 fine points (~p.88–90) | Dispatcher correctness |
| REXX in a pipeline, CALLPIPE, COMMIT | Ch. 6 (~p.91+) | `pipe-rexx`, nested pipes |
| Pipeline dispatcher, stage states, commit, read/write, delay | Ch. ~18 (~p.213–219) | `pipe-core` |
| Inventory of built-in programs | Ch. 19 (~p.221+) | Stage checklist / parity |
| `spec` deep dive | specialised chapters (~p.158+) | Expand `SPECS` subset |

## Design invariants (always)

1. Stages exchange **records** via the dispatcher only — never call each other.
2. At most **one record in flight** per connection unless a stage explicitly buffers.
3. Prefer **drain-biased** scheduling (consumers that can progress first).
4. **Commit** coordinates when stages may produce side effects.
5. Labels attach **secondary/tertiary streams** of the same stage instance.
6. `CALLPIPE` `*:` connectors attach nested ends to the caller’s selected streams.

## Out of scope for v1 (even if in the book)

3270/fullscreen, tape, CP/CMS/TSO command fidelity, DB2/SQL stages, ISPF, EBCDIC as default, full 100+ built-in catalog.
