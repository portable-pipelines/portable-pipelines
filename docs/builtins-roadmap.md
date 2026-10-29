# Built-in stage roadmap

Checklist of **IBM CMS/TSO Pipelines** stages (Author’s Edition Appendix A / Ch. 19).
See [`reference/authors-edition-map.md`](reference/authors-edition-map.md).
What this port can run today is in [`STATUS.md`](STATUS.md).

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
| SPEC | subset | pp.496–502 (tutorial Ch.14 pp.153–162; reference Ch.20 pp.615–628). Implemented: `FIELDSEPARATOR`/`FS` / `WORDSEPARATOR`/`WS` (p.196); input ranges `n` / `n-m` / `n.len` / `n;m` / `*-*` / negatives / `WORD`/`FIELD` (elided `w1`) / `SUBSTRING … OF`; output column, range, `NEXT`/`NEXTWORD`/`NEXTFIELD` (and `NWORD`/`NW` / `NFIELD`/`NF`, `NEXT.n`); `LEFT`/`CENTRE`/`CENTER`/`RIGHT`; mixed literal + field; `PAD` xorc; `STRIP`; portable conversions `C2X`/`X2C`/`C2B`/`B2C`/`X2B`/`B2X`; `WRITE`/`READ`/`READSTOP`; `NUMBER`/`RECNO` with `FROM`/`BY`; field identifiers (`a:1`); counters `#0` to `#9`; `IF`/`THEN`/`ELSE`/`ENDIF`; `SET` (`:=` / `+=`); `PRINT`; `PICTURE` (a run of `9` digits); `NOWRITE`. Keyword `SPECS` → `SPEC`. Missing: `SELECT` / `WHEN` / `OTHERWISE`, `ELSEIF`, `EOF`/`BREAK`; `TODCLOCK` (p.498); packed/float/date/TOD conversions `C2D`/`D2C`/`C2F`/`F2C`/`C2P`/`P2C`/`C2V`/`V2C`/`C2I`/`I2C`/`C2T`/`T2C` (pp.624–627); `STOP`/`PRINTONLY`/`OUTSTREAM`/`ID` (pp.618–623); full COBOL pictures; parenthesised output column expressions (p.627). |
| COPY | done | p.305. Consume each input record, then write (one-record delay). No operands. Extra inputs rejected; secondary output unused. |
| REXX | subset | p.475. Verb is a `.rexx` / `.rex` path or `REXX`. Regina preferred; subset interp fallback. STEM/VAR family is Phase 5 (seven done, including VARDROP). |
| TAKE | subset | p.542. `FIRST` (default) / `LAST`, number (default 1), `0`, `*`. `BYTES` rejected. |
| DROP | subset | p.333. Converse of TAKE. `BYTES` rejected. Not an alias of HOLE. |
| NLOCATE | subset | pp.439–440. Inverse of the LOCATE subset. No operand selects null records. Synonym `NOTLOCATE`. |
| STRIP | done | pp.532–533. `BOTH` / `LEADING` / `TRAILING`, blank default, `STRING` / `ANYOF` / xrange, `NOT` / `TO`, `ANYCASE` (min 4; `ANYOF` wins at 3), optional per-side count. |
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
| LOOKUP | done | pp.416–423 (tutorial pp.87–88). `LOOKUP [COUNT] [INCREMENt] [SETCOUNT] [TRACKCOUnt] [COUNTDBG] [NOPAD \| PAD xorc] [ANYcase] [AUTOADD [BEFORE] \| CEILING \| FLOOR] [KEYONLY] [STRICT] [inputRange [inputRange]]` then DETAIL/MASTER/ALLMASTER/BOTH/MATCHING. Details on primary; secondary is the reference (read to EOF first); tertiary adds; quaternary deletes. Matched → primary; unmatched details → secondary; leftover masters → tertiary; deleted → quaternary; duplicate masters → quinary; unmatched deletes → senary. Diagram is all-caps. |
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
| PIPCMD | subset | pp.457–458 (commands Ch.21 pp.638–660). `PIPCMD [ STOP [ ALLEOF \| ANYEOF \| number ] ]`. Issues command records into the running set. Honoured: OUTPUT, CALLPIPE, ADDPIPE (≈ CALLPIPE), COMMIT, SELECT INPUT/OUTPUT/BOTH/ANYINPUT, SHORT, SEVER, RESOLVE, STAGENUM, STREAMNUM, STREAMSTATE (not ALL), MAXSTREAM, SUSPEND, MESSAGE, ADDSTREAM, EOFREPORT, ISSUEMSG (portable stand-in; no FPL repository), REXX subcommand, SETRC. REXX-only verbs are rejected (p.457). CALLPIPE streams for this live `StageCtx`. ADDPIPE is still a CALLPIPE alias. |
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
| TOLABEL | subset | pp.559–560. `TOLABEL [string]`. Through the record before the first prefix match. Remaining including the match go to secondary if connected. A live CALLPIPE caller can re-read the match. A Regina stage still drains input first, so leftover is lost there. |
| WHILELABEL | subset | pp.596–597. `WHILELABEL [string]`. While records begin with the string; rest to secondary. Same leftover limit as TOLABEL. |
| STRFIND | done | pp.349, 534. `STRFIND [ANYcase] delimitedString`. XEDIT FIND with a dstring / hex / binary. |
| STRNFIND | done | pp.438, 534. `STRNFIND [ANYcase] delimitedString`. Converse of STRFIND. |
| STRFRLABEL | done | pp.353, 534. `STRFRLABEL [ANYcase] [EXCLUSIVe \| INCLUSIVe] delimitedString`. Synonym `STRFROMLABEL`. |
| STRTOLABEL | subset | pp.559, 534. `STRTOLABEL [ANYcase] [EXCLUSIVe \| INCLUSIVe] delimitedString`. Same leftover limit as TOLABEL. |
| STRWHILELABEL | subset | pp.596, 534. `STRWHILElabel [ANYcase] delimitedString` (min 8). Same leftover limit as TOLABEL. |
| STRxxxxx | subset | Family p.534. Implemented members: STRFIND, STRNFIND, STRFRLABEL, STRTOLABEL, STRWHILELABEL, STRLITERAL. Missing: `STRASMFIND` / `STRASMNFIND` stay out with assembler. |
| ABBREV | done | p.254. `ABBREV [word [number [ANYCASE]]]`. Leading columns until a blank are an abbreviation of `word` (REXX `ABBREV`); default word is null, default number is 0. Primary/secondary. Diagram is all-caps. See Done notes. |
| SUBSTRING | done | p.536. `SUBSTRing inputRange` (min 6). Writes the specified range of each record (SPEC special case). Missing range writes a null record. |
| VERIFY | done | p.594. `VERIFY [ANYCASE] [inputRange] delimitedString`. Character list (not xrange). Null range or all characters in the set → primary; else secondary. |
| ALL | done | pp.259–260. `ALL expression`. Boolean of LOCATE substring tests: `!` / `\|` or, `&` and, `¬` not (UTF-8 U+00AC; `^` / `\` per PICK p.456). Grouping `(…)`. Primary/secondary. Note 2 `%debug`/`%dump` CMS diagnostics rejected. |
| WILDCARD | done | pp.597–598. `WILDCARD [ANYCase] [BLANK xorc\|OFF \| ANYCHaracter xorc\|OFF \| ANYSTRing xorc\|OFF]… [inputRange] delimitedString`. LISTFILE `*` / `%` / blank; ANYCASE min 4. Primary/secondary. |
| ESCAPE | done | pp.337–338. `ESCAPE [delimitedString]`. Prefix listed characters with the first character. Default `/"\|/` (quote, backslant, bar). Filter. |
| XRANGE | done | pp.609–610. `XRANGE [xrange \| xorc xorc]`. Device driver; must be first. One record: inclusive wrap (`a-i` / `a.9` / `a i`). No argument writes X'00'–X'FF'. Portable ASCII xorc (book `f0 f9` → `0123456789` is EBCDIC). |
| RANDOM | done | pp.469–470. `RANDOM [number \| *] [snumber \| *]`. Device driver; must be first. 4-byte big-endian values until the output severs. First word is the modulus (remainder; `*` placeholder); second is the seed (`*` or omitted = wall clock). Generator is SplitMix64 (book algorithm undocumented). |
| DIGEST | done | pp.326–327. `DIGEST SHA1 \| SHA256 \| SHA384 \| SHA512 \| MD5 [APPEND \| VERIFY]`. Filter. Primary-only: one digest of concatenated records. Secondary: copy PI→SO; digest on PO at SI trigger or PI EOF. `APPEND` is `record \|\| digest`; `VERIFY` strips it (or writes the whole record to secondary). Software RustCrypto (no CMS Message-security Assist). |
| STRLITERAL | done | pp.412–413, 534. `STRLITERAL [PREFACE \| APPEND [CONDitional] \| IFEMPTY] [delimitedString]`. STR form of LITERAL. Default PREFACE. Optional dstring (p.412; omitted is a null record). `CONDitional` min 4 only with APPEND. Diagram is all-caps. |
| SPILL | done | pp.502–504. `SPILL number [ [STRing] dstring \| [NOT] ANYof dstring ] [ANYcase] [KEEP] [OFFSET number\|dstring]`. Split long lines at word boundaries. Optional secondary for unsplittable words. Diagram is all-caps. Portable ASCII blank X'20'. |
| FRTARGET | done | pp.354–355. `FRTARGET word [string]`. From the first record selected by an argument stage through EOF. Synonym `FROMTARGET`. Nested selection hosted like CASEI/NOT. Diagram is all-caps. |
| TOTARGET | done | pp.560–561. `TOTARGET word [string]`. Through the first record selected by an argument stage; trigger and rest to secondary if defined. Diagram is all-caps. |
| FANINTWO | done | pp.340–341. `FANINTWO`. Two-stream fan-in; secondary has priority. No operands. Diagram is all-caps. |
| FANOUTWO | done | pp.342–343 (tutorial p.90). `FANOUTWO`. Copy to both outputs; EOF backwards from the primary. No operands. Diagram is all-caps. |
| FILLUP | done | p.345. `FILLUP`. Pass records to output streams as they fill up. No operands. Diagram is all-caps. |
| THREEWAY | done | p.557. `THREEWAY inputRange`. Split each record onto three outputs (prefix / range / rest). Synonym `3WAY`. Diagram is all-caps. |
| DEBLOCK | subset | pp.317–320. `DEBLOCK n` / `DEBLOCK FIXED n` / bare `DEBLOCK` (FIXED 80), plus `DEBLOCK LINEND [xorc] [TERMinate] [EOF [xorc]]` and `DEBLOCK CRLF [TERMinate] [EOF [xorc]]`. FIXED splits into character groups of width *n* (a leftover shorter group is written last). LINEND / CRLF are a byte stream; ASCII LF / CRLF defaults. Left out: FROMRIGHT, VARIABLE/V/VB/VS/VBS, CMS/CMS4, SF/SF4, RDW, RFC959, ONEBYTE, ADMSF, GDFORDERS, AWSTAPE, DECIMAL, NETDATA, TEXTUNIT, MONITOR, TEXTFILE, C, STRING, CKD. Diagram is all-caps. |
| PARCEL | done | p.452. `PARCEL`. Byte stream parcelled by lengths from secondary input. Diagram is all-caps. |
| HTTPSPLIT | done | p.387. `HTTPSPLIT [EBCDIC]`. ASCII default (CRLF X'0D0A'); headers to primary, body to secondary. Diagram is all-caps. |
| URLDEBLOCK | done | pp.577–578. `URLDEBLOCK [EBCDIC]`. CGI/form-urlencoded: `&` / `;` split, `+` → blank, `%XX` hex byte. ASCII default; `EBCDIC` uses CP037/1047 specials. Null input records discarded. Diagram is all-caps. |
| STEM | done | p.528. When first, reads from a REXX stem and writes records (`name.0` = count). When not first, stores records in `name.1` … and sets `name.0`. Regina only, CALLPIPE from a REXX stage. |
| VAR | done | p.580. When first, splits a REXX variable on newline into records. When not first, joins records with newline. Regina only, CALLPIPE from a REXX stage. |
| VARSET | done | p.590. Sets a REXX variable to a delimited string value; copies primary input through (filter). Regina only, CALLPIPE from a REXX stage. |
| VARFETCH | done | p.585. Writes the value of a REXX variable as a single record; must be first. Regina only, CALLPIPE from a REXX stage. |
| REXXVARS | done | p.477. Must be first. Writes one record per REXX variable in the form `name value`. Regina only, CALLPIPE from a REXX stage. |
| VARLOAD | done | p.587. Reads records in the form `name value` and sets REXX variables; copies through when output is connected. Regina only, CALLPIPE from a REXX stage. |
| VARDROP | done | p.583. Drops named REXX variables (operands, then first word of each input record). A trailing `.` drops a stem. Regina only, CALLPIPE. |
| DATECONVERT | subset | p.311. Portable FROM/TO date conversion (ISO, YMD/ORDERED, DMY/EUROPEAN, MDY/USA, JULIAN, GREG). Not the IBM DATE stage. |
| GETFILES | subset | p.368. Read host files named on input records. Missing file errors, or secondary if connected. |
| GREG2SEC | subset | p.369. Gregorian timestamp to seconds since 1970-01-01 00:00:00 UTC. |
| SEC2GREG | subset | p.485. Seconds since the Unix epoch to `yyyymmddhhmmss` UTC. |
| STACK | subset | p.513. Process-wide FIFO/LIFO deque. Default LIFO. |
| SUBCOM | subset | p.534. COMMAND/CMD/SYSTEM/SH/SHELL run the host shell. |
| SYSVAR | subset | p.541. Host environment and computed OS/ARCH/PID/CWD/HOSTNAME/USER/HOME/PATH/TEMP. |
| TIMESTAMP | subset | p.557. Prefix UTC `YYYY-MM-DD HH:MM:SS` and a separator. |
| COLLATE | done | pp.295–297. `COLLATE [STOP ANYEOF] [NOPAD \| PAD xorc] [ANYcase] [inputRange [inputRange]] [MASTER DETAIL \| MASTER \| DETAIL [MASTER]]`. Masters on primary, details on secondary. Matched → primary; unmatched masters → secondary; unmatched details → tertiary. Diagram is all-caps. Portable ASCII collation (book: System/360 / EBCDIC). |
| MERGE | done | pp.436–437. `MERGE [NOPAD \| PAD xorc] [ANYcase] [Ascending \| Descending \| inputRange [Ascending \| Descending] [NOPAD \| PAD xorc] …]`. Up to 10 already-ordered inputs; equal keys keep the lowest stream first. Diagram is all-caps. `Ascending` / `Descending` min 1. Portable ASCII collation. |
| DEAL | done | pp.313–316. `DEAL [STOP [ALLEOF \| ANYEOF \| number] \| SECONDARY [RELEASE] \| KEY inputRange [STRIP] \| STREAMid inputRange [STRIP]]`. Round-robin, secondary stream-ids, key runs, or STREAMID. `STREAMid` min 7. Named stream identifiers rejected (numeric only). Diagram is all-caps. |
| GATHER | done | pp.366–367. `GATHER [STOP [ALLEOF \| ANYEOF \| number] \| STREAMid inputRange]`. Converse of DEAL. Round-robin or STREAMID. `STREAMid` min 7. Named stream identifiers rejected. Diagram is all-caps. |

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
| Streaming CALLPIPE | subset | Live callers splice nested stages into the parent dispatcher and loan streams. Leftover TOLABEL/WHILELABEL/STR* records are left for that caller. Regina stages still drain input first. Named FANOUT/FANIN/DEAL/GATHER/SELECT/ADDSTREAM identifiers and a real ADDPIPE remain later work. |
| AGGRC | done | Aggregate numeric return codes (most negative, else largest). pp.258–259. See Done table. |
| BEAT | done | Heartbeat: copy records; write a marker on secondary if none arrive in the interval (pp.271–272). See Done table. |
| DAM | done | Wait for a primary record, then short all streams (pp.310–311). See Done table. |
| DELAY | done | Suspend the stream for a time-of-day or interval (pp.321–322). See Done table. |
| EOFBACK | done | Copy through, then run an output driver so EOF can propagate backwards (p.337). See Done table. |
| IF | done | Run a selection stage in an if-then / if-then-else multistream topology. pp.389–390. See Done table. |
| JUXTAPOSE | done | Preface the last primary record with each secondary record (pp.403–405). See Done table. |
| NOEOFBACK | done | Pass records and ignore end-of-file on output (pp.440–441). See Done table. |
| PIPCMD | subset | Issue pipeline commands from input records (pp.457–458). See Done table. CALLPIPE streams. ADDPIPE is still a CALLPIPE alias. STREAMSTATE ALL is REXX-only. |
| PIPESTOP | subset | Stop stages waiting on an external event (pp.458–459). See Done table. Missing: CMS ECB post with completion code X'3F' (p.458); PIPMOD STOP immediate command (p.150, p.322). |
| PREDSELECT | done | Route the original record after a destructive test on copies (pp.459–460). See Done table. |
| RUNPIPE | subset | Issue nested PIPE specifications, intercepting messages (pp.481–483). See Done table. Missing: EVENTS/MASK Appendix G records (p.798); live pause-on-message (p.481); PIPMOD MSGLEVEL bits (p.482). |
| SYNCHRONISE | done | Wait for a record on every input, then copy one from each (pp.536–539). Synonym `SYNCHRONIZE`. See Done table. |
| UPDATE | done | Apply a CMS-format UPDATE file from secondary onto the primary master. pp.576–577. See Done table. |
| LOOKUP | done | See Done table. |
| ELASTIC | done | Full stage per Author’s Edition 1.1.11 p.335 (tutorial Ch.5 pp.88–89; FANIN note 2 p.338). Stall-avoiding buffer; secondary input is a to-do/feedback list. |

---

## Phase 5 — Portable device / host extras (thin)

Only IBM verbs where a portable OS mapping is obvious.

| Stage | Status | Notes |
|---|---|---|
| STEM | done | pp.528. When first stage (device driver), reads from a REXX stem and writes records. When not first (sink), reads records and populates stem (STEM.0 = count, STEM.1, STEM.2, etc.). Regina only, use from REXX program with CALLPIPE. Design: [rexx-variable-pool.md](design/rexx-variable-pool.md). |
| VAR | done | pp.580. When first stage (device driver), reads from a REXX variable and writes records (split on newlines). When not first (sink), reads records and stores in variable (newline-separated). Regina only, use from REXX program with CALLPIPE. |
| VARSET | done | pp.590. Sets a REXX variable to a delimited string value; copies primary input through (filter). Regina only, use from REXX program with CALLPIPE. |
| VARFETCH | done | pp.585. Writes the value of a REXX variable as a single record; must be first stage (device driver). Regina only, use from REXX program with CALLPIPE. |
| REXXVARS | done | pp.477. Must be first stage (device driver). Writes one record per REXX variable in the form `name value`. Regina only, use from REXX program with CALLPIPE. |
| VARLOAD | done | pp.587. Reads records in the form `name value` and sets REXX variables; copies records through (filter). Regina only, use from REXX program with CALLPIPE. |
| VARDROP | done | p.583. Drops named REXX variables (operands, then first word of each input record). A trailing `.` drops a stem. Regina only, CALLPIPE. |
| DATECONVERT | subset | p.311. Portable FROM/TO formats: ISO, YMD/ORDERED, DMY/EUROPEAN, MDY/USA, JULIAN, GREG. Not the IBM DATE stage. |
| GETFILES | subset | p.368. Read host files named on input records (portable `<` mapping). Missing file errors, or secondary if connected. |
| GREG2SEC | subset | p.369. Gregorian `yyyymmddhhmmss` (and separated UTC forms) to seconds since 1970-01-01 00:00:00 UTC. |
| SEC2GREG | subset | p.485. Seconds since the Unix epoch to `yyyymmddhhmmss` UTC. |
| STACK | subset | p.513. Process-wide FIFO/LIFO deque (not the CMS console stack). Default LIFO. |
| SUBCOM | subset | p.534. `COMMAND`/`CMD`/`SYSTEM`/`SH`/`SHELL` run the host shell. CMS/TSO environments are not implemented. |
| SYSVAR | subset | p.541. Host environment dump or named lookup; computed OS/ARCH/PID/CWD/HOSTNAME/USER/HOME/PATH/TEMP. Not CMS sysvars. |
| TIMESTAMP | subset | p.557. Prefix UTC `YYYY-MM-DD HH:MM:SS` and a separator (default blank). |

`DISK` remains the portable diskfast verb. `<` / `>` / `>>` are registered with a host-path operand: the remainder after the verb is one path (`/tmp/x.txt` is not a delimited `/tmp/`), and a blank is required after the verb (`< file.txt`, not `<file.txt`). CMS `fn ft fm`, `<MDSK`, `>SFS`, and the rest of the disk-driver family stay out of scope.

---

## Phase 6 — Polish / on-demand

Niche IBM filters only when a real pipe needs them:

| Stage | Status | Notes |
|---|---|---|
| OVERLAY | done | Overlay data from input streams (p.447). See Done table. |
| XLATE | subset | ASCII/code-page subset. See Done table. Missing: `TO`/`FROM` `[CODEPAGE] number` (p.604); Figure 339 code pages (pp.604–605); `A2E`/`E2A` (Note 1 p.607); live CMS SET INPUT/OUTPUT (p.604). |
| PAD | done | Expand short records (p.451). See Done table. |
| REVERSE | done | Reverse contents of records (p.474). See Done table. |
| INSERT | done | Insert a string in records (p.392). See Done table. |
| FIND / NFIND | done | XEDIT-style selection (pp.349–350, 438–439). Synonym `NOTFIND`. See Done table. |
| FRLABEL / TOLABEL / WHILELABEL | subset | Label runs (pp.353–354, 559–560, 596–597). Synonym `FROMLABEL`. FRLABEL is done. TOLABEL / WHILELABEL leftover works for live CALLPIPE; Regina stages still drain first. See Done table. |
| INSIDE / OUTSIDE / NOTINSIDE | done | Label-range family with BETWEEN (pp.393, 445, 442). See Done table. |
| COLLATE | done | Collate master and detail streams (pp.295–297). See Done table. |
| MERGE | done | Merge already-ordered streams (pp.436–437). See Done table. |
| DEAL | done | Pass records to output streams (pp.313–316). See Done table. |
| GATHER | done | Copy records from input streams (pp.366–367). See Done table. |
| CASEI | done | Case-insensitive wrapper (pp.285–286). See Done table. Same wording as ZONE (p.610): the argument is *assumed* to be a selection stage and *should* behave like one; the diagram takes any `word`. |
| NOT | done | Invert primary/secondary (pp.441–442). See Done table. |
| FILLUP | done | Pass records to output streams (p.345). See Done table. |
| COPY | done | Copy records, allowing a one-record delay (p.305). See Done table. |
| ABBREV | done | Select records whose first word is an abbreviation of the argument. p.254. See Done table. |
| ALL | done | Select records matching a boolean string expression (`&` / `!` / `¬`). pp.259–260. See Done table. |
| BLOCK | out-of-scope | Block to an external format (p.273). Not useful in a portable build (owner decision). |
| COMBINE | done | Combine a run of records into one. pp.298–300. See Done table. |
| DEBLOCK | subset | Inverse of BLOCK (p.317). FIXED n, LINEND, and CRLF; see Done table. |
| DIGEST | done | Compute a message digest (SHA-1/256/384/512 or MD5). pp.326–327. See Done table. |
| ESCAPE | done | Insert escape characters before listed characters. pp.337–338. See Done table. |
| FANINTWO | done | Two-stream fan-in (arcane FANIN). p.340. See Done table. |
| FANOUTWO | done | Copy to both outputs; EOF backwards from primary. p.342. See Done table. |
| FBLOCK | out-of-scope | Block data, spanning input records (p.343). Not useful in a portable build (owner decision). |
| FRTARGET | done | From the first record selected by an argument stage through EOF. p.354. Synonym `FROMTARGET`. See Done table. |
| HTTPSPLIT | done | Split an HTTP stream into header records and body (ASCII default). p.387. See Done table. |
| PARCEL | done | Parcel a byte stream into records. p.452. See Done table. |
| QPDECODE / QPENCODE | done | Quoted-printable decode / encode. pp.466–467. See Done table. |
| RANDOM | done | Generate pseudo-random numbers. pp.469–470. See Done table. |
| RETAB | done | Replace runs of blanks with tabulate characters (converse of UNTAB). p.473. See Done table. |
| SNAKE | done | Build a multicolumn page layout. pp.491–492. See Done table. |
| SPACE | done | Space words as REXX `SPACE` does. p.495. See Done table. |
| SPILL | done | Spill long lines at word boundaries; optional secondary for unsplittable words. p.502. See Done table. |
| STRxxxxx | subset | Family (p.534). Implemented: STRFIND, STRNFIND, STRFRLABEL, STRTOLABEL, STRWHILELABEL, STRLITERAL. Missing: `STRASMFIND` / `STRASMNFIND` stay out with assembler. See Done table. |
| STRFIND | done | `STR` form of FIND (delimited string / hex / binary). pp.349, 534. See Done table. |
| STRNFIND | done | `STR` form of NFIND. pp.438, 534. See Done table. |
| STRLITERAL | done | `STR` form of LITERAL (`PREFACE` / `APPEND` / `CONDitional` / `IFEMPTY`). pp.412, 534. See Done table. |
| STRTOLABEL | subset | `STR` form of TOLABEL. pp.559, 534. Same leftover limit as TOLABEL. See Done table. |
| STRFRLABEL | done | `STR` form of FRLABEL. Synonym `STRFROMLABEL`. pp.353, 534. See Done table. |
| STRWHILELABEL | subset | `STR` form of WHILELABEL. pp.596, 534. Same leftover limit as TOLABEL. See Done table. |
| SUBSTRING | done | Write a substring of each record. p.536. See Done table. |
| THREEWAY | done | Split each record three ways onto three outputs. p.557. Synonym `3WAY`. See Done table. |
| TOKENISE | done | Split a record into tokens. Synonym `TOKENIZE`. p.558. See Done table. |
| TOTARGET | done | Select through the first record selected by an argument stage. p.560. See Done table. |
| UNTAB | done | Replace tabulate characters with blanks (converse of RETAB). p.575. See Done table. |
| URLDEBLOCK | done | Process a universal resource locator into fields. pp.577–578. See Done table. |
| VERIFY | done | Select records that contain only specified characters. p.594. See Done table. |
| WILDCARD | done | Select records matching a pattern. pp.597–598. See Done table. |
| XRANGE | done | Write a range of characters (the device driver, not the xorc helper). pp.609–610. See Done table. |
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
| CALLPIPE | subset | Run a subroutine pipeline. Live callers stream (loans). Regina stages still drain first. p.641. |
| ADDPIPE | subset | Add a specification to the running set; still a CALLPIPE alias. p.639. |
| SELECT | done | Choose a stream: INPUT / OUTPUT / BOTH / a number / ANYINPUT. p.654. Named stream identifiers are not supported. |
| COMMIT | done | Commit to a new level; subset and Regina. p.642. |
| PEEKTO | done | Preview the next input line without consuming it. p.649. After PEEKTO, CALLPIPE `*:` sees that unread record. |
| ADDSTREAM | done | Create a numeric stream. RC is the stream number. p.640. |
| BEGOUTPUT | subset | Enter implied output mode. p.640. Subset SAY writes to OUTPUT. Regina SAY still goes to the terminal. |
| EOFREPORT | done | Enable reporting of stream events (CURRENT / ANY / ALL). Severed I/O is RC 8. p.643. |
| GETRANGE | done | Extract an `inputRange` into a variable. RC 8 when the start is missing. p.644. |
| ISSUEMSG | done | Issue `number [component] [tokens]` as a pipeline message. Portable stand-in (no FPL repository). p.645. |
| MAXSTREAM | done | Return the highest-numbered connected stream. p.646. Shared with PIPCMD. |
| MESSAGE | done | Issue a message. p.647. Shared with PIPCMD. |
| NOCOMMIT | subset | Recorded; dispatcher auto-commit to 0 before the stage body is unchanged. p.647. |
| RESOLVE | done | Return 1 if the name is a registered built-in, else 0. p.651. Shared with PIPCMD. |
| REXX | done | Call a REXX pipeline program from a filter (`*: \| path \| *:`). p.651. Inner Regina is not re-entered. |
| SCANRANGE | done | Parse an `inputRange` from the start of a string. p.652. |
| SCANSTRING | done | Parse a `delimitedString` from the start of a string. p.653. |
| SETRC | done | Set the stage return code. A nonzero EXIT still wins; EXIT 0 keeps SETRC. p.655. |
| SEVER | done | Break INPUT or OUTPUT. p.656. Shared with PIPCMD. |
| STAGENUM | done | Return the stage's 1-based position. p.657. Shared with PIPCMD. |
| STREAMNUM | done | Return a stream number. p.657. Shared with PIPCMD. |
| STREAMSTATE | done | Return stream status; ALL sets a stem. p.658. Shared with PIPCMD (ALL is REXX-only). |
| SUSPEND | done | Allow other stages to run. p.659. No-op in the buffered Regina host. |

---

## Non-IBM extensions

These are **not** Appendix A / Chapter 19 verbs. Rule 6 does not apply. Do not add them to the Coverage IBM-verb counts.

| Extension | Status | Notes |
|---|---|---|
| Python-language stages | subset | Alternative language for user-written stages (same role as REXX filters). A program you write that reads records from its input and writes records to its output, used by naming the `.py` file in the pipeline, just as you do with a REXX stage (not a registry entry, no PIPCMD command). Live `readto` / `peekto` / `output` / `short` / `records()` / `exit`. Single-stream only. Design: [`design/python-stages.md`](design/python-stages.md). |

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
| Track / CKD | TRACKBLOCK, TRACKDEBLOCK, TRACKREAD, TRACKSQUISH, TRACKVERIFY, TRACKWRITE, TRACKXPAND, CKDDEBLOCK; CKD / AWSTAPE / NETDATA / CMS / V / VB / VS / VBS / SF / FIXED / STRING / C operands of BLOCK/DEBLOCK (DEBLOCK LINEND / CRLF are implemented; BLOCK and FBLOCK are out of scope) |

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

EBCDIC as default; APLENCODE/APLDECODE; ASA/CCW carriage stages (ASATOMC, MCTOASA, OPTCDJ); overstrike (OVERSTR, C14TO38); 3270 graphic escapes; CIPHER (CMS CP-assist DES/AES/Blowfish). DIGEST (SHA/MD5) is implemented in software (no CMS Message-security Assist).

### Niche / mainframe record formats (not useful in a portable build)

| Stage | Reason |
|---|---|
| ADDRDW | Prefix a CMS/OS/SF record descriptor word (p.255). Mainframe record format. |
| PACK / UNPACK | Mainframe packed/file formats (pp.449, 574). |
| JEREMY | Write pipeline-set status when an input record arrives (p.398). Niche debugger. |
| SCM | Align REXX comments (p.483). Niche formatter. |
| VCHAR | Recode characters to a different bit length (p.592). Niche; not useful on a portable ASCII host. |
| BLOCK | Block to an external format (p.273). Not useful in a portable build. |
| FBLOCK | Block data, spanning input records (p.343). Not useful in a portable build. |

---

## Working agreement

1. Feature branch `cursor/builtins-phase-N-…` from `master`.
2. IBM names and primary/secondary semantics; document ambiguities next to the code and add a regression test.
3. Prefer one solid stage with tests over five half-ports.
4. REXX examples may prototype behavior, then graduate to a built-in when stable.
5. Do not walk Appendix A A–Z — implement by phase unlock, not alphabet.
6. Never add a roadmap row for a verb that is not in Appendix A / Ch. 19.

---

## Coverage

Every Chapter 19 inventory entry in Author’s Edition 1.1.11 (SL26-0018-05) is now on this roadmap: implemented, planned in a phase, noted as an alias/synonym, or listed out of scope. Counts against the 254 unique Appendix A / Chapter 19 verbs: **120 done or subset** (Done table, including LOOKUP and REXX; DISK remains the portable diskfast stand-in and is not a separate Chapter 19 name; STRxxxxx is a family heading, not a Chapter 19 verb. StageRegistry has **120** primary names, documented synonyms excluded: that set is these **120** Chapter 19 implementations minus REXX, which is invoked as a `.rexx` / `.rex` path rather than a registry entry, plus DISK. XLATE is a Phase 6 subset; INSERT, PAD, REVERSE, RETAB, UNTAB, 64ENCODE, 64DECODE, QPENCODE, QPDECODE, SNAKE, SPACE, TOKENISE, FIND, NFIND, FRLABEL, CASEI, NOT, COMBINE, OVERLAY, BETWEEN, INSIDE, NOTINSIDE, OUTSIDE, STRFIND / STRNFIND / STRFRLABEL, ABBREV, SUBSTRING, VERIFY, ALL, WILDCARD, ESCAPE, XRANGE, RANDOM, DIGEST, STRLITERAL, SPILL, FRTARGET, TOTARGET, FANINTWO, FANOUTWO, FILLUP, THREEWAY, COLLATE, MERGE, DEAL, GATHER, PARCEL, HTTPSPLIT, COPY, and URLDEBLOCK are done in Phase 6; **STEM, VAR, VARSET, VARFETCH, REXXVARS, VARLOAD, and VARDROP are done in Phase 5**; DATECONVERT, GETFILES, GREG2SEC, SEC2GREG, STACK, SUBCOM, SYSVAR, and TIMESTAMP are Phase 5 subsets; DEBLOCK is a Phase 6 subset (LINEND / CRLF only); TOLABEL, WHILELABEL, STRTOLABEL, STRWHILELABEL, and STRxxxxx are Phase 6 subsets), **0 planned in Phase 4** (Streaming CALLPIPE is a subset dispatcher feature, not a Chapter 19 verb), **0 planned in Phase 5**, **0 planned in Phase 6**, **133 out of scope** (ADDRDW, PACK, UNPACK, JEREMY, SCM, VCHAR, BLOCK, and FBLOCK skipped as niche / mainframe record formats). **120 + 133 = 253** Chapter 19 programs placed; the remaining catalog slot was previously counted as streaming CALLPIPE. **120 + 1 + 0 + 0 + 133 = 254**. REXX pipeline commands (Chapter 21) are tracked separately: 22 done, 4 subset.
