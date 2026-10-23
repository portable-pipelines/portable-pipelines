# Author’s Edition → Portable Pipelines

Use CMS/TSO Pipelines Author's Edition 1.1.11 (document SL26-0018-05) as the behavioral authority when IBM docs and our code disagree. Keep your local copy of the PDF; it is not included in this repo. Record chosen interpretations in [`../architecture.md`](../architecture.md) plus a regression test.

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
| `casei` | Ch.19 pp.285–286; tutorial p.122; ZONE p.610 | Run a selection stage case-insensitively (`ZONE` / `REVERSE`) |
| `not` | Ch.19 pp.441–442 | Run a stage with its output streams inverted |
| `dam` | Ch.19 pp.310–311; example p.311 | Wait for a primary record, then short all stream pairs (converse of GATE) |
| `juxtapose` | Ch.19 pp.403–405; tutorial pp.83–84 Figures 173–174; collate example p.297 | Preface the last primary record with each secondary record |
| `synchronise` | Ch.19 pp.536–539; examples pp.537–538; tutorial p.84; dispatcher delay p.219 | Wait for a record on every input, then copy one from each. Synonym `synchronize` |
| `predselect` | Ch.19 pp.459–460; tutorial p.85 Figure 175; FANOUT example p.341 | Route the original record after a destructive test on copies |
| `noeofback` | Ch.19 pp.440–441; GATE example p.441 | Pass records and ignore end-of-file on output. Synonym `noteofback` |
| `eofback` | Ch.19 p.337; message 39I example p.665 | Copy through, then run an output device driver so EOF can propagate backwards |
| `aggrc` | Ch.19 pp.258–259; host-command note p.39; PIPE return codes p.219 | Aggregate numeric return codes (most negative, else largest) |
| `delay` | Ch.19 pp.321–322; tutorial Ch.13 pp.148–150 | Suspend the stream for a time-of-day or a `+` interval taken from each record |
| `beat` | Ch.19 pp.271–272; example p.272 | Heartbeat: copy records; write a marker on secondary if none arrive in the interval |
| `update` | Ch.19 pp.576–577; tutorial p.86 Figure 176 | Apply a CMS UPDATE SEQ8 file from secondary onto the primary master |
| `pipcmd` | Ch.19 pp.457–458; pipeline commands Ch.21 pp.638–660 | Issue pipeline commands from input records |
| `pipestop` | Ch.19 pp.458–459; tutorial p.150; DELAY p.322 | Stop stages waiting on an external event (portable `waiting_until` stand-in for CMS ECBs) |
| `runpipe` | Ch.19 pp.481–483; Appendix G p.798; PIPE syntax Ch.17 p.231 | Issue nested PIPE specifications, intercepting messages |
| `xlate` | Ch.19 pp.604–607; tutorial pp.44–46; syntactic variables pp.194–197 | Transliterate record contents. Synonym `translate` |
| `combine` | Ch.19 pp.298–300 | Combine a run of records (or a two-stream pair) bitwise or by FIRST/LAST column |
| `overlay` | Ch.19 p.447 | Overlay records from all connected input streams (`BLANK` / xorc) |
| `insert` | Ch.19 p.392; REXX tutorial pp.100–101, 106; syntactic variables pp.194–196 | Insert a string in records (`BEFORE` / `AFTER` / `inputRange`) |
| `abbrev` | Ch.19 p.254; REXX `ABBREV()` | Select records whose leading word is an abbreviation |
| `substring` | Ch.19 p.536; `inputRange` pp.195–196; SPEC p.496 | Write a substring of each record (`SUBSTRing`) |
| `verify` | Ch.19 p.594; `inputRange` pp.195–196; REXX `VERIFY()` | Select records that contain only specified characters |
| `all` | Ch.19 pp.259–260; locate p.414 | Boolean of substring tests (`&` / `!` / `¬`) |
| `wildcard` | Ch.19 pp.597–598; `inputRange` pp.195–196 | Select records matching a LISTFILE-style pattern |
| `escape` | Ch.19 pp.337–338 | Insert escape characters before listed characters |
| `xrange` | Ch.19 pp.609–610; xorc/xrange pp.197 | Write a range of characters (device driver) |
| `random` | Ch.19 pp.469–470 | Generate pseudo-random numbers |
| `digest` | Ch.19 pp.326–327 | Compute a message digest (SHA-1/256/384/512, MD5) |
| `pad` | Ch.19 p.451; xorc p.197; socka2ip example p.519 | Expand short records (`Right` / `Left` / `MODULO` / xorc) |
| `reverse` | Ch.19 p.474 | Reverse contents of records |
| `retab` | Ch.19 p.473 | Replace runs of blanks with tabulate characters (`-number` / stop list). Converse of `untab` |
| `untab` | Ch.19 p.575 | Replace tabulate characters with blanks (`-number` / stop list). Converse of `retab` |
| `64encode` | Ch.19 pp.613–614 | MIME Base-64 encode (RFC 1521). Byte stream; 76-character lines |
| `64decode` | Ch.19 p.613 | MIME Base-64 decode. One output record per input record |
| `qpencode` | Ch.19 p.467 | MIME quoted-printable encode (RFC 2045). 76-byte wrap |
| `qpdecode` | Ch.19 p.466 | MIME quoted-printable decode. Soft-break splice; optional secondary for bad input |
| `snake` | Ch.19 pp.491–492; walkthrough pp.16–17 Figure 27; listpds example p.411 | Multicolumn page layout (`SNAKE number [number]`) |
| `space` | Ch.19 p.495 | Space words like REXX SPACE (`STRing` / `ANYof` / xorc) |
| `tokenise` | Ch.19 p.558 | Tokenise records. Synonym `tokenize` |
| `find` | Ch.19 pp.349–350; XEDIT FIND; STRFIND p.534 | Select records that begin with an XEDIT FIND string |
| `nfind` | Ch.19 pp.438–439; synonym `notfind`; STRNFIND p.534 | Converse of FIND |
| `frlabel` | Ch.19 pp.353–354; synonym `fromlabel`; STRFRLABEL p.534 | From the first record with a leading string |
| `tolabel` | Ch.19 pp.559–560; STRTOLABEL p.534 | Through the record before the first leading string |
| `whilelabel` | Ch.19 pp.596–597; STRWHILELABEL p.534 | While records begin with a leading string |
| `between` | Ch.19 pp.272–273 | Select records between labels (inclusive). Converse `outside` |
| `inside` | Ch.19 p.393 | Select records between labels (exclusive). Converse `notinside` |
| `notinside` | Ch.19 pp.442–443; synonym `ninside` | Select records not between labels (exclusive) |
| `outside` | Ch.19 pp.445–446 | Select records not between labels (inclusive). Converse `between` |
| `strxxxxx` | Ch.19 p.534; FIND/NFIND/FRLABEL/TOLABEL/WHILELABEL/LITERAL STR branches | `STR` prefix: argument is a delimited string |
| `strliteral` | Ch.19 pp.412–413, 534 | STR form of LITERAL (`PREFACE` / `APPEND` / `CONDitional` / `IFEMPTY`) |
| `spill` | Ch.19 pp.502–504 | Spill long lines at word boundaries; optional secondary for unsplittable words |
| `frtarget` | Ch.19 pp.354–355; synonym `fromtarget` | From the first record selected by an argument stage |
| `totarget` | Ch.19 pp.560–561 | Through the first record selected by an argument stage |
| `fanintwo` | Ch.19 pp.340–341 | Two-stream fan-in; secondary input has priority |
| `fanoutwo` | Ch.19 pp.342–343; tutorial p.90 Figure 180 | Copy to both outputs; EOF backwards from the primary |
| `fillup` | Ch.19 p.345 | Pass records to output streams as they fill up |
| `threeway` | Ch.19 p.557 | Split each record onto three outputs. Synonym `3way` |
| `collate` | Ch.19 pp.295–297 | Collate master and detail streams |
| `merge` | Ch.19 pp.436–437 | Merge already-ordered streams |
| `deal` | Ch.19 pp.313–316 | Pass records to output streams |
| `gather` | Ch.19 pp.366–367 | Copy records from input streams |
| `deblock` | Ch.19 pp.317–320 | Deblock external data formats (portable LINEND / CRLF subset) |
| `parcel` | Ch.19 p.452 | Parcel a byte stream into records |
| `httpsplit` | Ch.19 p.387 | Split an HTTP stream into headers and body (ASCII default) |
| `copy` | Ch.19 p.305; elastic Note 1 p.335 | Copy records, allowing a one-record delay |
| `urldeblock` | Ch.19 pp.577–578 | Process a universal resource locator into fields (CGI) |
| `spec` deep dive | Ch.14 tutorial p.153, Ch.19 p.496, Ch.20 p.615 | Expand `SPEC` subset (Appendix A keyword `SPECS` → `SPEC`) |
| Messages 1–4 / MSGLEVEL | Ch.22 messages; PIPE options MSGLEVEL (~p.231); debugging examples (~p.124 in the z/VM User's Guide) | Stage context: Running / Issued from stage N of pipeline M / NAME / Processing |

## Design invariants (always)

1. Stages exchange **records** via the dispatcher only — never call each other.
2. At most **one record in flight** per connection unless a stage explicitly buffers.
3. Prefer **drain-biased** scheduling (consumers that can progress first).
4. **Commit** coordinates when stages may produce side effects.
5. Labels attach **secondary/tertiary streams** of the same stage instance.
6. `CALLPIPE` `*:` connectors attach nested ends to the caller’s selected streams.

## Out of scope for v1 (even if in the book)

3270/fullscreen, tape, CP/CMS/TSO command fidelity, DB2/SQL stages, ISPF, EBCDIC as default, full 100+ built-in catalog.
