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
| `lookup` | Ch.19 pp.416–423; tutorial pp.87–88 Figure 178 | Keyed detail/master match; six streams |
| `elastic` | Ch.19 p.335; tutorial Ch.5 pp.88–89; FANIN note 2 p.338 | Stall-avoiding buffer; secondary input is a to-do/feedback list |
| `gate` | Ch.19 pp.365–366; examples pp.366, 441, 458, 518 | Pass higher stream pairs until a primary record stops the gate |
| `if` | Ch.19 pp.389–390 | Selection stage plus FANINANY in if-then / if-then-else label topology |
| `dam` | Ch.19 pp.310–311; example p.311 | Wait for a primary record, then short all stream pairs (converse of GATE) |
| `juxtapose` | Ch.19 pp.403–405; tutorial pp.83–84 Figures 173–174; collate example p.297 | Preface the last primary record with each secondary record |
| `synchronise` | Ch.19 pp.536–539; examples pp.537–538; tutorial p.84; dispatcher delay p.219 | Wait for a record on every input, then copy one from each. Synonym `synchronize` |
| `predselect` | Ch.19 pp.459–460; tutorial p.85 Figure 175; FANOUT example p.341 | Route the original record after a destructive test on copies |
| `noeofback` | Ch.19 pp.440–441; GATE example p.441 | Pass records and ignore end-of-file on output. Synonym `noteofback` |
| `spec` deep dive | Ch.14 tutorial p.153, Ch.19 p.496, Ch.20 p.615 | Expand `SPEC` subset (Appendix A keyword `SPECS` → `SPEC`) |

## Design invariants (always)

1. Stages exchange **records** via the dispatcher only — never call each other.
2. At most **one record in flight** per connection unless a stage explicitly buffers.
3. Prefer **drain-biased** scheduling (consumers that can progress first).
4. **Commit** coordinates when stages may produce side effects.
5. Labels attach **secondary/tertiary streams** of the same stage instance.
6. `CALLPIPE` `*:` connectors attach nested ends to the caller’s selected streams.

## Out of scope for v1 (even if in the book)

3270/fullscreen, tape, CP/CMS/TSO command fidelity, DB2/SQL stages, ISPF, EBCDIC as default, full 100+ built-in catalog.
