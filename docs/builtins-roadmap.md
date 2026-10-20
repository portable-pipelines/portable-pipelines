# Built-in stage roadmap

Checklist of **IBM CMS/TSO Pipelines** stages (Author’s Edition Appendix A / Ch. 19).
See [`reference/authors-edition-map.md`](reference/authors-edition-map.md).
Engine decisions stay in [`architecture.md`](architecture.md).

Only IBM stage verbs and their documented operands belong here. Do not add invented
stage names (e.g. no `TAKELAST` — IBM is `TAKE LAST`).

Statuses:

| Status | Meaning |
|---|---|
| `done` | Registered under the IBM verb (or noted IBM synonym) and usable |
| `phase-N` | Planned in that phase (not implemented yet) |
| `subset` | Present, but not full Author’s Edition surface |
| `out-of-scope` | Intentionally skipped (CMS/z-specific or non-portable) |

Rule: every new stage gets a compat or unit test under `tests/compat/` or `crates/pipe-stages/tests/`.

---

## Done (IBM stages)

Page cites are Author’s Edition 1.1.11 (SL26-0018-05). Audited against Chapter 19 unless noted.

| Stage | Status | Notes |
|---|---|---|
| LITERAL | done | pp.412–413. Argument starts after exactly one blank; omitted argument writes a null record; then copies input. A cascade is reverse order. |
| CONSOLE | done | pp.303–305. First stage reads stdin (`EOF` dstring / `NOEOF`; default EOF is the null line). Not-first writes the terminal and copies through. `TERMINAL` synonym. `DIRECT` / `DARK` / `INVISIBLE` / `ASYNCHRONOUSLY` rejected (CMS). |
| DISK | subset | p.328 `diskfast` stand-in. First stage reads; missing file is empty. Not-first appends/creates and copies through. Host path operand (not CMS `fn ft fm`). Invented `OUTPUT` / `APPEND` rejected. |
| `<` | subset | p.230. Must be first. Reads a host path. Missing file is an error. |
| `>` | subset | p.237. Must not be first. Replaces or creates a host path; copies through. |
| `>>` | subset | p.245. Must not be first. Appends to or creates a host path; copies through. |
| COMMAND | subset | pp.300–301. `CMD` synonym. Portable process runner (not CMS SVC 202). Argument string plus non-blank input lines. No secondary return-code stream. |
| LOCATE | subset | pp.414–415. `ANYCASE` (and synonyms) / `ANYOF`; dstring is pair / hex / binary (p.194). No operand selects length ≥ 1. Primary/secondary. `MIXED` / `ONES` / `ZEROS` and column ranges rejected. |
| CHANGE | subset | pp.286–288. Default is **all** occurrences; `//new/` prefixes; `/old/new/` or two dstrings (pair / hex / binary, p.194); optional count / `*`; `ANYCASE` case rules. Column ranges rejected. |
| COUNT | done | pp.306–307. At least one keyword required. `CHARACTERs` (min 8) / `CHARS` / `BYTES`, `WORDS`, `LINES` / `RECORDS`, `MINline` / `MAXline`. Output order is characters, words, lines, min, max. Secondary copies then writes the count. Bare `COUNT` is rejected. |
| SORT | subset | pp.493–494. Default ascending whole record (binary). `UNIQUE` / `COUNT` first; `DESCENDING` / `ASCENDING` / `ANYCASE`. `PAD` / `NOPAD` and field ranges rejected. |
| BUFFER | done | pp.279–281. No args: delay until EOF. `n [dstring]`: copy each null-delimited subfile `n` times, delimiter between copies, then copy the null. |
| FANOUT | subset | p.341. Copies to every connected output. `STOP ALLEOF` / `ANYEOF` / `n`. Named stream-id list not implemented. |
| FANIN | subset | p.338. Primary to EOF, then secondary, …; optional numeric stream list (`0` / `1`). Named stream identifiers rejected. |
| FANINANY | done | p.339. Any-ready via `StageCtx::read_any`. `STRICT` accepted (lowest ready stream when the dispatcher exposes it). |
| HOLE | done | p.382. Reads every connected input; no output; no operands. |
| SPEC | subset | pp.496–502 (tutorial Ch.14 pp.153–162; reference Ch.20 pp.615–628). Implemented: `FIELDSEPARATOR`/`FS` / `WORDSEPARATOR`/`WS` (p.196); input ranges `n` / `n-m` / `n.len` / `n;m` / `*-*` / negatives / `WORD`/`FIELD` (elided `w1`) / `SUBSTRING … OF`; output column, range, `NEXT`/`NEXTWORD`/`NEXTFIELD` (and `NWORD`/`NW` / `NFIELD`/`NF`, `NEXT.n`); `LEFT`/`CENTRE`/`CENTER`/`RIGHT`; mixed literal + field; `PAD` xorc; `STRIP`; portable conversions `C2X`/`X2C`/`C2B`/`B2C`/`X2B`/`B2X`; `WRITE`/`READ`/`READSTOP`; `NUMBER`/`RECNO` with `FROM`/`BY`. Keyword `SPECS` → `SPEC`. Missing: expression language / counters / `IF`/`THEN`/`SELECT`/`EOF`/`BREAK`/`SET`/`PRINT`/`PICTURE` (pp.163–187, 615–637); `TODCLOCK` (p.498); packed/float/date/TOD conversions `C2D`/`D2C`/`C2F`/`F2C`/`C2P`/`P2C`/`C2V`/`V2C`/`C2I`/`I2C`/`C2T`/`T2C` (pp.624–627); `STOP`/`PRINTONLY`/`OUTSTREAM`/`NOWRITE`/`ID` (pp.618–623); parenthesised output column expressions (p.627). |
| COPY | done | p.305. Consume each input record, then write. No operands. |
| REXX | subset | p.475. Verb is a `.rexx` / `.rex` path or `REXX`. Regina preferred; subset interp fallback. STEM/VAR stay Phase 5. |
| TAKE | subset | p.542. `FIRST` (default) / `LAST`, number (default 1), `0`, `*`. `BYTES` rejected. |
| DROP | subset | p.333. Converse of TAKE. `BYTES` rejected. Not an alias of HOLE. |
| NLOCATE | subset | pp.439–440. Inverse of the LOCATE subset. No operand selects null records. Synonym `NOTLOCATE`. |
| STRIP | subset | pp.532–533. `BOTH` / `LEADING` / `TRAILING`, blank default, `STRING` / `ANYOF` / xrange, `NOT` / `TO`, `ANYCASE` (min 4; `ANYOF` wins at 3), optional per-side count. |
| BETWEEN | done | pp.272–273. `BETWEEN [ANYcase] dstring (number \| dstring)`. Inclusive labels; count ≥ 2. Prefix match. Synonyms for ANYCASE in Notes. Column ranges are not documented. |
| INSIDE | done | p.393. `INSIDE [ANYcase] dstring (number \| dstring)`. Exclusive labels; count ≥ 0. Prefix match. Converse `NOTINSIDE`. |
| NOTINSIDE | done | pp.442–443. `NOTINSIDe [ANYcase] dstring (number \| dstring)` (min 7). Exclusive labels; count ≥ 0. Synonym `NINSIDE`. Converse of INSIDE. |
| OUTSIDE | done | pp.445–446. `OUTSIDE [ANYcase] dstring (number \| dstring)`. Inclusive labels; count ≥ 2. Converse of BETWEEN. |
| SPLIT | done | Break records at a target. Author's Edition 1.1.11 p.505. Full syntax: `ANYCASE`, `MINIMUM`, `AT` / `[snumber] BEFORE` / `[snumber] AFTER`, `NOT`, `BLANK`, xrange / `STRING` / `ANYOF` (dstring pair / hex / binary), optional count. Copies null input records; does not generate null records. |
| JOIN | done | Reassemble records. Author's Edition 1.1.11 p.399. Full syntax: `COUNT`, append count / `*` / `KEYLENGTH n`, delimited insert (pair / hex / binary), `TERMINATE`, optional maximum length. Default joins pairs; secondary input defaults to unlimited and flushes the buffer. Null records are empty contributors. |
| JOINCONT | done | Join continuation lines. Author's Edition 1.1.11 pp.401–403. Full syntax: `ANYCASE`, `NOT`, `TRAILING` (default) / `RANGE inputRange` / `LEADING`, `DELAY`, `ANYOF`, continuation dstring, `KEEP`, optional insert dstring. |
| CHOP | done | Truncate records. Author's Edition 1.1.11 pp.289–290 (examples pp.6, 12, 58–59). Column form (default 80; synonym `TRUNCATE`) or target form: `ANYCASE`, `[snumber] BEFORE` / `AFTER`, `NOT`, xrange / `STRING` / `ANYOF`. Prefix → primary; remainder → secondary. |
| UNIQUE | done | Discard or retain adjacent duplicates. Author's Edition 1.1.11 pp.572–574 (tutorial Figure 116 p.56). `COUNT`, `NOPAD` / `PAD xorc`, `ANYCASE`, `inputRange` or `( inputRange [PAD|NOPAD] … )`, `LAST` (default) / `FIRST` / `SINGLES` / `MULTIPLE` / `PAIRWISE`. |
| DUPLICATE | done | Copy records. Author's Edition 1.1.11 p.334 (tutorial pp.148–149). `DUPlicate [snumber | *]`. Default additional count 1 (write twice). Number must be −1 or larger; `*` copies the first record until the output severs (cap 2 147 483 647, p.149). `duplicate -1` consumes all input and writes nothing. |
| PICK | done | Relation select. Author's Edition 1.1.11 pp.454–456 (tutorial pp.53–54, 68). `[NOPAD \| PAD xorc] [ANYcase] (inputRange \| dstring) operator (inputRange \| dstring)`. Strict string operators `==` `¬==` `<<` `<<=` `>>` `>>=`; numeric `=` `¬=` `<` `<=` `>` `>=`. Primary/secondary. |
| ZONE | subset | Run a stage on a column subset. Author's Edition 1.1.11 pp.610–611 (tutorial pp.52, 67 Figure 140). Syntax: `ZONE inputRange [CASEI] [REVERSE] word [string]`. `word` is any registered stage (unknown verbs error). Missing: (1) no-added-delay streaming — buffers until EOF; (2) commit level −2; (3) Note 3 EOF propagation to the nested stage’s streams. |
| CASEI | done | Case-insensitive wrapper. Author's Edition 1.1.11 pp.285–286 (tutorial p.122). Syntax: `CASEI [ZONE inputRange] [REVERSE] word [string]`. Uppercases the nested argument and the data passed to it (portable ASCII A–Z); writes originals. `word` is any registered stage. |
| NOT | done | Invert primary/secondary. Author's Edition 1.1.11 pp.441–442. Syntax: `NOT word [string]`. Nested primary out → NOT secondary (if defined); nested secondary out → NOT primary. |
| PREFACE | done | pp.461–462. Nested CALLPIPE first (unconnected start, drain to primary output), then copy primary input. Argument is a pipeline spec (`||` self-escapes `|`). |
| APPEND | done | pp.263–264. Copy primary input first, then nested CALLPIPE. Same argument / connector rules as PREFACE. |
| ELASTIC | done | p.335 (tutorial Ch.5 pp.88–89; FANIN note 2 p.338). Stall-avoiding buffer. No operands. Primary-only or secondary-input to-do/feedback. |
| GATE | done | pp.365–366 (examples pp.366, 441, 458, 518). `GATE [STRICT]`. Pass higher stream pairs until a primary record arrives, then short primary and stop. |
| IF | done | pp.389–390. `IF word [string]`. Selection stage plus FANINANY in an if-then (two streams) or if-then-else (three streams) topology. |
| DAM | done | pp.310–311 (example p.311). `DAM`. Wait for a primary record, then short all stream pairs in numerical order (converse of GATE). |
| JUXTAPOSE | done | pp.403–405 (tutorial pp.83–84; collate example p.297). `JUXTAPOSE [COUNT]`. Store each primary in a one-record buffer; append each secondary and write the combined record. Unused primaries go to the secondary output. |
| SYNCHRONISE | done | pp.536–539 (examples pp.537–538; tutorial p.84). `SYNChronise` / `SYNChronize`. Peek every input, write one to each corresponding output, then consume. EOF on any stream stops the cycle. |
| PREDSELECT | done | pp.459–460 (tutorial p.85 Figure 175; FANOUT example p.341). `PREDSELect`. Store each primary record; a secondary-input trigger writes it to primary output, a tertiary-input trigger writes it to secondary output. |
| NOEOFBACK | done | pp.440–441 (GATE example p.441). `NOEOFBACK`. Pass records while the output is connected; when the output is at EOF, consume remaining input. Synonym `NOTEOFBACK`. |
| EOFBACK | done | p.337 (file-driver example p.665). `EOFBACK word [string]`. Copy through; after each record is consumed, run the nested output driver so EOF can propagate backwards. |
| AGGRC | done | pp.258–259 (host-command note p.39; PIPE aggregate p.219). `AGGRC`. Aggregate numeric return codes (most negative, else largest). Last stage sets the PIPE return code. |
| DELAY | done | pp.321–322 (tutorial Ch.13 pp.148–150). `DELAY`. No operands; the first blank-delimited word of each record is a time-of-day or a `+` interval. Portable `Instant` / `thread::sleep` stand-in (not CMS TODCLOCK). |
| BEAT | done | pp.271–272. `BEAT [ONCE] number[.number] [delimitedString]`. Copy on primary; marker on secondary when quiet. `ONCE` is one marker per input record. Portable `wait_until` / `wait_until_or_input` stand-in (not CMS TODCLOCK). |
| UPDATE | done | pp.576–577 (tutorial Figure 176 p.86). `UPDATE [range] [FIRST] [LAST] [* string]`. SEQ8 `./ I` / `./ D` / `./ R` / `./ S` / `./ *` from secondary onto the primary master; log on secondary output. Portable ASCII sequence digits (default columns 73-80). |
| PIPCMD | subset | pp.457–458 (commands Ch.21 pp.638–660). `PIPCMD [ STOP [ ALLEOF \| ANYEOF \| number ] ]`. Issues command records into the running set. Honoured: OUTPUT, CALLPIPE, ADDPIPE (≈ CALLPIPE), COMMIT, SELECT INPUT/OUTPUT/BOTH/ANYINPUT, SHORT, SEVER, RESOLVE, STAGENUM, STREAMNUM, STREAMSTATE (not ALL), MAXSTREAM, SUSPEND, MESSAGE. Missing allowlist members (p.639): ADDSTREAM (p.640), EOFREPORT (p.643), ISSUEMSG (p.645), REXX subcommand (p.651), SETRC (p.655). REXX-only verbs are rejected (p.457). |
| PIPESTOP | subset | pp.458–459 (tutorial p.150; DELAY p.322). `PIPESTOP`. No operands. When a record arrives, wake stages parked in `waiting_until` (DELAY/BEAT) and forbid further time waits, then copy the record through. Missing: CMS ECB post with completion code X'3F' (p.458); PIPMOD STOP immediate command (p.150, p.322). |
| RUNPIPE | subset | pp.481–483. `RUNPIPE [ MSGLevel number \| Xhex ] [ TRACE \| EVENTS [ MASK hex ] ]`. Issues each input record as a new PIPE set; intercepts messages onto primary output (`PIPES0001E` / `PIPES0002I` / `PIPES0003I`). Missing: EVENTS/MASK Appendix G records (p.798); live pause-on-message (p.481); PIPMOD MSGLEVEL bits (p.482); IBM message numbers. |
| XLATE | subset | pp.604–607 (tutorial pp.44–46). `XLATE` / `TRANSlate` (min 5). Input ranges, `INput` / `OUTput` / `UPper` / `LOWer`, from-to xrange pairs, secondary-input table. ASCII case tables (A–Z / a–z). Missing: `TO`/`FROM` `[CODEPAGE] number` (p.604); Figure 339 code pages 37, 37-2, 273, 274, 275, 277, 278, 279, 280, 281, 282, 283, 284, 285, 297, 437, 500, 819, 850, 863, 865, 871, 1047 (pp.604–605); `A2E` ≡ FROM 819 and `E2A` ≡ TO 819 (Note 1 p.607); live CMS SET INPUT / SET OUTPUT tables (p.604; keywords accepted as the documented identity default). |
| INSERT | done | p.392 (REXX tutorial pp.100–101, 106). `INSERT delimitedString [BEFORE \| AFTER] [inputRange]`. Default BEFORE; default range is the entire record. Missing range uses the first or last column. |
| PAD | done | p.451 (socka2ip example p.519). `PAD [Right \| Left] [[number] MODULO] number [BLANK \| xorc]`. Default right-pad with blank. First number (offset) must be smaller than the second (modulo). |
| REVERSE | done | p.474. `REVERSE`. No operands. Reverses the bytes of each record. |
| RETAB | done | p.473. `RETAB [ -number \| number … ]`. Default `-3` (stops `1 4 7 …`). Replace runs of blanks with tabulate characters (converse of UNTAB). |
| UNTAB | done | p.575. `UNTAB [ -number \| number … ]`. Default `-3` (stops `1 4 7 …`). Replace tabulate characters with blanks (converse of RETAB). |
| 64ENCODE | done | pp.613–614. `64ENCODE`. MIME Base-64 encode (RFC 1521). Byte stream; 76-character output records except the last; null input records discarded. Portable ASCII alphabet (book: encoded data are EBCDIC). |
| 64DECODE | done | p.613. `64DECODE`. MIME Base-64 decode. One output record per input record. Length need not be a multiple of four. Portable ASCII alphabet. |
| QPENCODE | done | p.467. `QPENCODE`. MIME quoted-printable encode (RFC 2045). 76-byte wrap with soft-break `=`. Also encodes ASCII `!"#$`, `@`, `[\]^`, backtick, `{|}~` (book: not EBCDIC-invariant). ASCII domain. |
| QPDECODE | done | p.466. `QPDECODE`. MIME quoted-printable decode. `=XX` hex (case ignored); trailing `=` splices. Invalid escape: fail, or secondary gets the bad record. ASCII domain. |
| SNAKE | done | pp.491–492 (walkthrough pp.16–17 Figure 27; listpds example p.411). `SNAKE number [number]`. Columns down then across; optional page depth. Neither pads nor truncates. |
| SPACE | done | p.495. `SPACE [number] [xorc \| [STRing] delimitedString] [xorc \| [ANYof] delimitedString]`. Collapse delimiter runs like REXX SPACE. |
| TOKENISE | done | p.558. `TOKENISE \| TOKENIZE dstring [dstring]`. Split a record into tokens. Synonym `TOKENIZE`. |
| COMBINE | done | pp.298–300. `COMBINE [number \| * \| KEYLENgth number \| STOP [ALLEOF \| ANYEOF \| number]] Or \| aNd \| eXclusiveor \| FIRST \| LAST`. Combine a run (or a two-stream pair) bitwise or by FIRST/LAST column presence. |
| OVERLAY | done | p.447. `OVERlay [BLANK \| xorc]` (min 4). Highest-numbered non-pad character wins; all connected inputs; primary output only. |
| FIND | done | pp.349–350. `FIND [string]`. XEDIT FIND (tabs 1, image off, case mixed respect). Remainder after one blank; blank = don't-care; underscore = required blank. Primary/secondary. |
| NFIND | done | pp.438–439. `NFIND [string]`. Converse of FIND. Synonym `NOTFIND`. |
| FRLABEL | done | pp.353–354. `FRLABEL [string]`. From the first prefix match through EOF. Synonym `FROMLABEL`. Remainder after one blank. |
| TOLABEL | subset | pp.559–560. `TOLABEL [string]`. Through the record before the first prefix match. Remaining including the match go to secondary if connected. Missing: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. |
| WHILELABEL | subset | pp.596–597. `WHILELABEL [string]`. While records begin with the string; rest to secondary. Missing: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. |
| STRFIND | done | pp.349, 534. `STRFIND [ANYcase] delimitedString`. XEDIT FIND with a dstring / hex / binary. |
| STRNFIND | done | pp.438, 534. `STRNFIND [ANYcase] delimitedString`. Converse of STRFIND. |
| STRFRLABEL | done | pp.353, 534. `STRFRLABEL [ANYcase] [EXCLUSIVe \| INCLUSIVe] delimitedString`. Synonym `STRFROMLABEL`. |
| STRTOLABEL | subset | pp.559, 534. `STRTOLABEL [ANYcase] [EXCLUSIVe \| INCLUSIVe] delimitedString`. Missing: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. |
| STRWHILELABEL | subset | pp.596, 534. `STRWHILElabel [ANYcase] delimitedString` (min 8). Missing: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. |
| STRxxxxx | subset | Family p.534. Implemented members: STRFIND, STRNFIND, STRFRLABEL, STRTOLABEL, STRWHILELABEL. Missing: STRLITERAL (`PREFACE` / `APPEND` / `CONDitional` / `IFEMPTY`, p.412). `STRASMFIND` / `STRASMNFIND` stay out with assembler. |

### Removed non-IBM registry aliases

These were registered earlier and have been dropped so the registry matches Appendix A:

| Former alias | Replacement |
|---|---|
| OUTPUT (as console stage) | Use `CONSOLE`. `OUTPUT` remains the REXX pipeline subcommand only |
| FILE | Use `DISK` |
| HOST | Use `COMMAND` |
| DROP → HOLE | `DROP` is its own stage (phase 1). It does not alias `HOLE` |

Kept: `SPECS` → `SPEC` (Appendix A keyword synonym); `CMD` → `COMMAND` (1.1.11 p.300); `TERMINAL` → `CONSOLE` (1.1.11 p.305); `NOTLOCATE` → `NLOCATE`; `TRUNCATE` → `CHOP` (1.1.11 p.289); `NOTEOFBACK` → `NOEOFBACK` (1.1.11 p.441); `NOTFIND` → `NFIND` (1.1.11 p.439); `FROMLABEL` → `FRLABEL` (1.1.11 p.353); `STRFROMLABEL` → `STRFRLABEL` (1.1.11 p.534); `NINSIDE` → `NOTINSIDE` (1.1.11 p.443).

---

## Phase 1 — Selection and trimming

Shipped. TAKE, DROP, NLOCATE, and STRIP are in the done table. BETWEEN slipped to phase 2.

Exit criteria: `… | take 20 | nlocate /x/ | strip | console` without REXX.

---

## Phase 2 — Record reshape

Shipped. BETWEEN, SPLIT, JOIN, JOINCONT, CHOP, UNIQUE, DUPLICATE, PICK, and ZONE are in the done table.

Exit criteria: split → sort → unique → join style pipes work. ZONE wraps a selection stage on an `inputRange`.

---

## Phase 3 — Deepen SPEC

One stage, large surface — expand `builtins::spec` for IBM **SPEC**, not new verbs.
First increment shipped (see Done table). Remaining items are a later increment.

| Item | Status | Notes |
|---|---|---|
| Field separators | done | `FIELDSEPARATOR`/`FS`, `WORDSEPARATOR`/`WS` (p.196); persist until re-specified (p.502 Note 5) |
| Ranges `n-m` / `n.len` / `n;m` / negatives / `WORD`/`FIELD` | done | pp.154–157, 195–196, 498. Same-sign end-before-start is an error |
| Insertion / `NEXT` / `NEXTWORD` / `NEXTFIELD` | done | pp.155, 499, 627–628. `NWORD`/`NW`, `NFIELD`/`NF`; `NEXT.n` explicit length |
| Alignment `LEFT`/`CENTRE`/`RIGHT` | done | pp.158, 499–500, 628. `CENTER` synonym |
| Mixed literal + field builds | done | dstring / hex / binary literals (p.498) with input ranges |
| `PAD` / `STRIP` | done | pp.159, 498–499. NEXTWORD always inserts a blank (not the pad) |
| Portable conversions | done | `C2X` `X2C` `C2B` `B2C` `X2B` `B2X` (pp.499, 624–627). Others rejected |
| `WRITE` / `READ` / `READSTOP` | done | pp.160–162, 500. Multiple records from one cycle |
| `NUMBER` / `RECNO` | done | pp.158, 498, 623. `FROM`/`BY`. Not the counter/`SET`/`PRINT` language |
| Expression language | missing | Counters `#n`, `SET`, `PRINT`/`PICTURE`, `IF`/`THEN`/`ELSE`, `SELECT`, `EOF`/`BREAK`, run-in/run-out (pp.163–187, 615–637) |
| `TODCLOCK` / remaining conversions | missing | pp.498, 624–627 (`C2D`/`D2C`/`C2F`/`F2C`/`C2P`/`P2C`/`C2V`/`V2C`/`C2I`/`I2C`/`C2T`/`T2C`) |
| Multistream / `STOP` / `PRINTONLY` / `OUTSTREAM` | missing | pp.161–162, 500, 618–621 |

Cite Author’s Edition SPEC chapters when expanding; tests in `crates/pipe-stages/tests/phase3_spec.rs`.

---

## Phase 4 — Multistream and flow control

| Stage / feature | Status | Notes |
|---|---|---|
| FANINANY fidelity | done | Any-ready via `read_any` (1.1.11 p.339). Named stream identifiers still omitted. |
| ELASTIC | done | Stall-avoiding buffer (p.335). See Done table. |
| GATE | done | Pass until stopped (pp.365–366). See Done table. |
| PREFACE | done | Nested CALLPIPE, then copy primary (pp.461–462) |
| APPEND | done | Copy primary, then nested CALLPIPE (pp.263–264) |
| Streaming CALLPIPE | phase-4 | Replace buffering bridges in dispatcher. Also required for TOLABEL / WHILELABEL / STRTOLABEL / STRWHILELABEL leftover: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. |
| AGGRC | done | Aggregate numeric return codes (most negative, else largest). pp.258–259. See Done table. |
| BEAT | done | Heartbeat: copy records; write a marker on secondary if none arrive in the interval (pp.271–272). See Done table. |
| DAM | done | Wait for a primary record, then short all streams (pp.310–311). See Done table. |
| DELAY | done | Suspend the stream for a time-of-day or interval (pp.321–322). See Done table. |
| EOFBACK | done | Copy through, then run an output driver so EOF can propagate backwards (p.337). See Done table. |
| IF | done | Run a selection stage in an if-then / if-then-else multistream topology. pp.389–390. See Done table. |
| JUXTAPOSE | done | Preface the last primary record with each secondary record (pp.403–405). See Done table. |
| NOEOFBACK | done | Pass records and ignore end-of-file on output (pp.440–441). See Done table. |
| PIPCMD | subset | Issue pipeline commands from input records (pp.457–458). See Done table. Missing p.639 allowlist members: ADDSTREAM (p.640), EOFREPORT (p.643), ISSUEMSG (p.645), REXX subcommand (p.651), SETRC (p.655). |
| PIPESTOP | subset | Stop stages waiting on an external event (pp.458–459). See Done table. Missing: CMS ECB post with completion code X'3F' (p.458); PIPMOD STOP immediate command (p.150, p.322). |
| PREDSELECT | done | Route the original record after a destructive test on copies (pp.459–460). See Done table. |
| RUNPIPE | subset | Issue nested PIPE specifications, intercepting messages (pp.481–483). See Done table. Missing: EVENTS/MASK Appendix G records (p.798); live pause-on-message (p.481); PIPMOD MSGLEVEL bits (p.482). |
| SYNCHRONISE | done | Wait for a record on every input, then copy one from each (pp.536–539). Synonym `SYNCHRONIZE`. See Done table. |
| UPDATE | done | Apply a CMS-format UPDATE file from secondary onto the primary master. pp.576–577. See Done table. |
| LOOKUP | done | Full stage per Author’s Edition 1.1.11 pp.416–423 (tutorial pp.87–88, Figure 178). |
| ELASTIC | done | Full stage per Author’s Edition 1.1.11 p.335 (tutorial Ch.5 pp.88–89; FANIN note 2 p.338). Stall-avoiding buffer; secondary input is a to-do/feedback list. |

---

## Phase 5 — Portable device / host extras (thin)

Only IBM verbs where a portable OS mapping is obvious.

| Stage | Status | Notes |
|---|---|---|
| STEM | phase-5 | REXX/Regina stem variables (p.528) |
| VAR | phase-5 | Single variable in a REXX/CLIST pool (p.580) |
| VARSET / VARFETCH | phase-5 | Set/fetch helpers in the same family (pp.590, 585) |
| REXXVARS | phase-5 | Retrieve variables from a REXX or CLIST pool. p.477. |
| VARLOAD | phase-5 | Set variables in a REXX/CLIST pool from records. p.587. |
| VARDROP | phase-5 | Drop variables in a REXX pool. p.583. |
| DATECONVERT | phase-5 | Only if a portable subset is useful; no invented `DATE` stage (p.311) |
| GETFILES | phase-5 | Read host files named on input records (portable `<` mapping). p.368. |
| GREG2SEC | phase-5 | Gregorian `yyyymmddhhmmss` to seconds since 1970. p.369. |
| SEC2GREG | phase-5 | Seconds since epoch to a Gregorian timestamp (converse of GREG2SEC). p.485. |
| STACK | phase-5 | Read or write a FIFO/LIFO program stack (CMS console stack; portable queue mapping). p.513. |
| SUBCOM | phase-5 | Issue commands to a named subcommand environment. p.534. |
| SYSVAR | phase-5 | Write host/system variables to the pipeline (thin env mapping). p.541. |
| TIMESTAMP | phase-5 | Prefix a portable date-and-time stamp to each record. p.557. |

`DISK` remains the portable diskfast verb. `<` / `>` / `>>` are registered with a host-path operand (see architecture.md). CMS `fn ft fm`, `<MDSK`, `>SFS`, and the rest of the disk-driver family stay out of scope.

---

## Phase 6 — Polish / on-demand

Niche IBM filters only when a real pipe needs them:

| Stage | Status | Notes |
|---|---|---|
| OVERLAY | done | Overlay data from input streams (p.447). See Done table. |
| XLATE | subset | ASCII/code-page subset. See Done table. Missing: `TO`/`FROM` `[CODEPAGE] number` (p.604); Figure 339 code pages (pp.604–605); `A2E`/`E2A` (Note 1 p.607); live CMS SET INPUT/OUTPUT (p.604). |
| PACK / UNPACK | phase-6 | On demand (pp.449, 574) |
| PAD | done | Expand short records (p.451). See Done table. |
| REVERSE | done | Reverse contents of records (p.474). See Done table. |
| INSERT | done | Insert a string in records (p.392). See Done table. |
| FIND / NFIND | done | XEDIT-style selection (pp.349–350, 438–439). Synonym `NOTFIND`. See Done table. |
| FRLABEL / TOLABEL / WHILELABEL | subset | Label runs (pp.353–354, 559–560, 596–597). Synonym `FROMLABEL`. FRLABEL is done. TOLABEL / WHILELABEL missing: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. See Done table. |
| INSIDE / OUTSIDE / NOTINSIDE | done | Label-range family with BETWEEN (pp.393, 445, 442). See Done table. |
| COLLATE / MERGE / DEAL / GATHER | phase-6 | Multistream helpers as needed (pp.295, 436, 313, 366) |
| CASEI | done | Case-insensitive wrapper (pp.285–286). See Done table. Same wording as ZONE (p.610): the argument is *assumed* to be a selection stage and *should* behave like one; the diagram takes any `word`. |
| NOT | done | Invert primary/secondary (pp.441–442). See Done table. |
| FILLUP | phase-6 | Pass records to output streams (p.345) |
| COPY | phase-6 | One-record delay fidelity if still short (p.305) |
| ABBREV | phase-6 | Select records whose first word is an abbreviation of the argument. p.254. |
| ADDRDW | phase-6 | Prefix a record descriptor word to each record. p.255. |
| ALL | phase-6 | Select records matching a boolean string expression (`&` / `!` / `¬`). p.259. |
| BLOCK | phase-6 | Block to an external format; portable FIXED / STRING / LINEND / CRLF / C only (p.273). CMS / V / VB / VS / VBS / SF / AWSTAPE / NETDATA / CKD stay out. |
| COMBINE | done | Combine a run of records into one. pp.298–300. See Done table. |
| DEBLOCK | phase-6 | Inverse of BLOCK; same portable formats only. p.317. |
| DIGEST | phase-6 | SHA-1/256/384/512 or MD5 over the file (RFC/FIPS). p.326. |
| ESCAPE | phase-6 | Insert escape characters before listed characters. p.337. |
| FANINTWO | phase-6 | Two-stream fan-in (arcane FANIN). p.340. |
| FANOUTWO | phase-6 | Copy to both outputs; EOF backwards from primary. p.342. |
| FBLOCK | phase-6 | Block data, spanning input records (portable sizes). p.343. |
| FRTARGET | phase-6 | From the first record selected by an argument stage through EOF. p.354. |
| HTTPSPLIT | phase-6 | Split an HTTP stream into header records and body (ASCII default). p.387. |
| JEREMY | phase-6 | Write pipeline-set status when an input record arrives. p.398. |
| PARCEL | phase-6 | Parcel a byte stream into records. p.452. |
| QPDECODE / QPENCODE | done | Quoted-printable decode / encode. pp.466–467. See Done table. |
| RANDOM | phase-6 | Generate pseudo-random numbers. p.469. |
| RETAB | done | Replace runs of blanks with tabulate characters (converse of UNTAB). p.473. See Done table. |
| SCM | phase-6 | Align REXX comments. p.483. |
| SNAKE | done | Build a multicolumn page layout. pp.491–492. See Done table. |
| SPACE | done | Space words as REXX `SPACE` does. p.495. See Done table. |
| SPILL | phase-6 | Spill long lines at word boundaries; optional secondary for unsplittable words. p.502. |
| STRxxxxx | subset | Family (p.534). Implemented: STRFIND, STRNFIND, STRFRLABEL, STRTOLABEL, STRWHILELABEL. Missing: STRLITERAL (`PREFACE` / `APPEND` / `CONDitional` / `IFEMPTY`, p.412). `STRASMFIND` / `STRASMNFIND` stay out with assembler. See Done table. |
| STRFIND | done | `STR` form of FIND (delimited string / hex / binary). pp.349, 534. See Done table. |
| STRNFIND | done | `STR` form of NFIND. pp.438, 534. See Done table. |
| STRLITERAL | phase-6 | `STR` form of LITERAL (`PREFACE` / `APPEND` / `CONDitional` / `IFEMPTY`). pp.412, 534. |
| STRTOLABEL | subset | `STR` form of TOLABEL. pp.559, 534. Missing: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. See Done table. |
| STRFRLABEL | done | `STR` form of FRLABEL. Synonym `STRFROMLABEL`. pp.353, 534. See Done table. |
| STRWHILELABEL | subset | `STR` form of WHILELABEL. pp.596, 534. Missing: matching record not left in the pipeline for the CALLPIPE caller to re-read (needs streaming CALLPIPE, Phase 4, postponed), p.559 / p.596. See Done table. |
| SUBSTRING | phase-6 | Write a substring of each record. p.536. |
| THREEWAY | phase-6 | Split each record three ways onto three outputs. p.557. |
| TOKENISE | done | Split a record into tokens. Synonym `TOKENIZE`. p.558. See Done table. |
| TOTARGET | phase-6 | Select through the first record selected by an argument stage. p.560. |
| UNTAB | done | Replace tabulate characters with blanks (converse of RETAB). p.575. See Done table. |
| URLDEBLOCK | phase-6 | Process a universal resource locator into fields. p.577. |
| VCHAR | phase-6 | Recode characters to a different bit length. p.592. |
| VERIFY | phase-6 | Select records that contain only specified characters. p.594. |
| WILDCARD | phase-6 | Select records matching a pattern. p.597. |
| XRANGE | phase-6 | Write a range of characters (the device driver, not the xorc helper). p.609. |
| 64DECODE / 64ENCODE | done | MIME Base-64 decode / encode. pp.613–614. See Done table. |

Track intentional support vs out-of-scope here as the catalog grows.

---

## REXX pipeline commands

These are ADDRESS `PIPELINE` subcommands (Author’s Edition Chapter 21), not stage verbs. Rule 6 does not apply. Status is `done` / `subset` only when `crates/pipe-rexx` implements the command (subset interpreter and/or Regina bridge); otherwise `planned`.

| Command | Status | Notes |
|---|---|---|
| READTO | done | Read or discard one input record into a variable. p.650. |
| OUTPUT | done | Write a line. p.648. |
| SHORT | done | Connect / sever the selected input and output. p.656. |
| CALLPIPE | subset | Run a subroutine pipeline; v1 buffering connectors. p.641. |
| ADDPIPE | subset | Add a specification to the running set; v1 ≈ CALLPIPE. p.639. |
| SELECT | subset | Choose a stream; implemented as `SELECT INPUT/OUTPUT n` only. p.654. |
| COMMIT | subset | Commit to a new level; Regina path only. p.642. |
| PEEKTO | planned | Preview the next input line without consuming it. p.649. |
| ADDSTREAM | planned | Create a stream. p.640. |
| BEGOUTPUT | planned | Enter implied output mode. p.640. |
| EOFREPORT | planned | Enable reporting of stream events. p.643. |
| GETRANGE | planned | Extract part of a record or string. p.644. |
| ISSUEMSG | planned | Issue a message from the repository. p.645. |
| MAXSTREAM | planned | Return the highest-numbered stream. p.646. |
| MESSAGE | planned | Issue a message. p.647. |
| NOCOMMIT | planned | Disable automatic commit on I/O. p.647. |
| RESOLVE | planned | Return the entry point of a built-in program. p.651. |
| REXX | planned | Call a REXX pipeline program from a filter (subcommand, not the REXX stage). p.651. |
| SCANRANGE | planned | Parse an `inputRange`. p.652. |
| SCANSTRING | planned | Parse a `delimitedString`. p.653. |
| SETRC | planned | Set the return code in the stage writing. p.655. |
| SEVER | planned | Break a connection. p.656. |
| STAGENUM | planned | Return the stage’s position in the pipeline. p.657. |
| STREAMNUM | planned | Return a stream number. p.657. |
| STREAMSTATE | planned | Return stream status. p.658. |
| SUSPEND | planned | Allow other stages to run. p.659. |

---

## Out of scope

Permanently skipped even if listed in Appendix A. Do not invent portable approximations under these names.

### Device / platform

| Family | Examples |
|---|---|
| CMS/SFS/MDSK disk drivers | `<MDSK`, `>MDSK`, `>>MDSK`, `<SFS`, `<SFSSLOW`, `>SFS`, `>>SFS`, `>>SFSSLOW`, DISKFAST, DISKSLOW, DISKBACK, DISKRANDOM, DISKUPDATE, MDSK, MDSKBACK, MDSKRANDOM, MDSKSLOW, MDSKUPDATE, MDISKBLK, FILETOKEN, SFSBACK, SFSRANDOM, SFSUPDATE (bare `<` / `>` / `>>` are implemented with a host-path operand) |
| MVS / PDS / QSAM | `<MVS`, `>MVS`, `>>MVS`, LISTPDS, LISTCAT, LISTDSI, LISTISPF, READPDS, WRITEPDS, QSAM, MEMBERS, PDSDIRECT, SYSDSN, SYSOUT |
| OpenExtensions / HFS | `<OE`, `>OE`, `>>OE`, HFS, HFSDIRECTORY, HFSQUERY, HFSREPLACE, HFSSTATE, HFSXECUTE, FILEDESCRIPTOR |
| 3270 / fullscreen | FULLSCR, FULLSCRQ, FULLSCRS, BROWSE, BUILDSCR, 3277BFRA, 3277ENC, XPNDHI |
| Tape / unit record | TAPE, PUNCH, READER, URO, PRINTMC, CARD |
| Track / CKD | TRACKBLOCK, TRACKDEBLOCK, TRACKREAD, TRACKSQUISH, TRACKVERIFY, TRACKWRITE, TRACKXPAND, CKDDEBLOCK; CKD / AWSTAPE / NETDATA / CMS / V / VB / VS / VBS / SF operands of BLOCK/DEBLOCK (portable FIXED / STRING / LINEND / CRLF / C are Phase 6) |

### Host / networking / subsystem

| Family | Examples |
|---|---|
| CP / CMS / TSO command stages | CP, CMS, TSO (full CMS/TSO fidelity) |
| CP services | STARMSG, STARMON, STARSYS, DIAGE4, DEVINFO, STSI, ACIGROUP, TRFREAD |
| TCP/UDP / resolver | TCPCLIENT, TCPDATA, TCPLISTEN, UDP, HOSTBYADDR, HOSTBYNAME, HOSTID, HOSTNAME, IP2SOCKA, SOCKA2IP |
| SQL / DB2 | SQL, SQLSELECT, SQLCODES, ARIRVSTC |
| ISPF / XEDIT / HLASM / assembler | ISPF, XEDIT, XMSG, HLASM, HLASMERR, ASMCONT, ASMFIND, ASMNFIND, ASMXPND, STRASMFIND, STRASMNFIND |
| IUCV / VMCF / storage peek | IUCV, VMC, VMCF, STORAGE, NUCEXT, INSTORE, OUTSTORE, FITTING |
| DFSORT / IEBCOPY / maclib | DFSORT, IEBCOPY, MACLIB |
| FST / state | AFTFST, FMTFST, STATE, STATEW |
| Loader tables | LDRTBLS |
| External attribute buffers | XAB |
| Service / config | HELP, QUERY, CONFIGURE, PAUSE, IMMCMD, FILTERPACK, EMSG |

### Encoding / mainframe-only transforms

EBCDIC as default; APLENCODE/APLDECODE; ASA/CCW carriage stages (ASATOMC, MCTOASA, OPTCDJ); overstrike (OVERSTR, C14TO38); 3270 graphic escapes; CIPHER (CMS CP-assist DES/AES/Blowfish). DIGEST (SHA/MD5) is Phase 6.

---

## Working agreement

1. Feature branch `cursor/builtins-phase-N-…` from `master`.
2. IBM names and primary/secondary semantics; document ambiguities in [`architecture.md`](architecture.md).
3. Prefer one solid stage with tests over five half-ports.
4. REXX examples may prototype behavior, then graduate to a built-in when stable.
5. Do not walk Appendix A A–Z — implement by phase unlock, not alphabet.
6. Never add a roadmap row for a verb that is not in Appendix A / Ch. 19.

---

## Coverage

Every Chapter 19 inventory entry in Author’s Edition 1.1.11 (SL26-0018-05) is now on this roadmap: implemented, planned in a phase, noted as an alias/synonym, or listed out of scope. Counts against the 254 unique Appendix A / Chapter 19 verbs: **73 done or subset** (Done table; DISK remains the portable diskfast stand-in and is not a separate Chapter 19 name; LOOKUP is done in the Phase 4 table; XLATE is a Phase 6 subset; INSERT, PAD, REVERSE, RETAB, UNTAB, 64ENCODE, 64DECODE, QPENCODE, QPDECODE, SNAKE, SPACE, TOKENISE, FIND, NFIND, FRLABEL, CASEI, NOT, COMBINE, OVERLAY, BETWEEN, INSIDE, NOTINSIDE, OUTSIDE, and STRFIND / STRNFIND / STRFRLABEL are done in Phase 6; TOLABEL, WHILELABEL, STRTOLABEL, STRWHILELABEL, and STRxxxxx are Phase 6 subsets), **9 planned in Phase 4**, **15 planned in Phase 5**, **26 planned in Phase 6** (STRLITERAL remains; the STRxxxxx family is subset with STRLITERAL listed as the missing member), **131 out of scope**. REXX pipeline commands (Chapter 21) are tracked separately: 3 done, 4 subset, 19 planned.
