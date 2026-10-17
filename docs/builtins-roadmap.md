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
| SPEC | subset | p.496. `WORD` / `FIELD` / `WORDS`, delimited literals, `1-*` / `*`, `start.length`, `NEXT` / `NEXTWORD` placement skip. Keyword `SPECS` → `SPEC`. Full language is Phase 3. |
| COPY | done | p.305. Consume each input record, then write. No operands. |
| REXX | subset | p.475. Verb is a `.rexx` / `.rex` path or `REXX`. Regina preferred; subset interp fallback. STEM/VAR stay Phase 5. |
| TAKE | subset | p.542. `FIRST` (default) / `LAST`, number (default 1), `0`, `*`. `BYTES` rejected. |
| DROP | subset | p.333. Converse of TAKE. `BYTES` rejected. Not an alias of HOLE. |
| NLOCATE | subset | pp.439–440. Inverse of the LOCATE subset. No operand selects null records. Synonym `NOTLOCATE`. |
| STRIP | subset | pp.532–533. `BOTH` / `LEADING` / `TRAILING`, blank default, `STRING` / `ANYOF` / xrange, `NOT` / `TO`, `ANYCASE` (min 4; `ANYOF` wins at 3), optional per-side count. |
| BETWEEN | subset | pp.272–273. Two operands required; match is **begins with** (prefix). Count ≥ 2. dstring is pair / hex / binary (p.194). `ANYCASE` / `CASEANY` / `IGNORECASE` / `CASEIGNORE`. Column ranges/masks rejected. |
| SPLIT | done | Break records at a target. Author's Edition 1.1.11 p.505. Full syntax: `ANYCASE`, `MINIMUM`, `AT` / `[snumber] BEFORE` / `[snumber] AFTER`, `NOT`, `BLANK`, xrange / `STRING` / `ANYOF` (dstring pair / hex / binary), optional count. Copies null input records; does not generate null records. |
| JOIN | done | Reassemble records. Author's Edition 1.1.11 p.399. Full syntax: `COUNT`, append count / `*` / `KEYLENGTH n`, delimited insert (pair / hex / binary), `TERMINATE`, optional maximum length. Default joins pairs; secondary input defaults to unlimited and flushes the buffer. Null records are empty contributors. |
| JOINCONT | done | Join continuation lines. Author's Edition 1.1.11 pp.401–403. Full syntax: `ANYCASE`, `NOT`, `TRAILING` (default) / `RANGE inputRange` / `LEADING`, `DELAY`, `ANYOF`, continuation dstring, `KEEP`, optional insert dstring. |
| CHOP | done | Truncate records. Author's Edition 1.1.11 pp.289–290 (examples pp.6, 12, 58–59). Column form (default 80; synonym `TRUNCATE`) or target form: `ANYCASE`, `[snumber] BEFORE` / `AFTER`, `NOT`, xrange / `STRING` / `ANYOF`. Prefix → primary; remainder → secondary. |
| UNIQUE | done | Discard or retain adjacent duplicates. Author's Edition 1.1.11 pp.572–574 (tutorial Figure 116 p.56). `COUNT`, `NOPAD` / `PAD xorc`, `ANYCASE`, `inputRange` or `( inputRange [PAD|NOPAD] … )`, `LAST` (default) / `FIRST` / `SINGLES` / `MULTIPLE` / `PAIRWISE`. |
| DUPLICATE | done | Copy records. Author's Edition 1.1.11 p.334 (tutorial pp.148–149). `DUPlicate [snumber | *]`. Default additional count 1 (write twice). Number must be −1 or larger; `*` copies the first record until the output severs (cap 2 147 483 647, p.149). `duplicate -1` consumes all input and writes nothing. |
| PICK | done | Relation select. Author's Edition 1.1.11 pp.454–456 (tutorial pp.53–54, 68). `[NOPAD \| PAD xorc] [ANYcase] (inputRange \| dstring) operator (inputRange \| dstring)`. Strict string operators `==` `¬==` `<<` `<<=` `>>` `>>=`; numeric `=` `¬=` `<` `<=` `>` `>=`. Primary/secondary. |

### Removed non-IBM registry aliases

These were registered earlier and have been dropped so the registry matches Appendix A:

| Former alias | Replacement |
|---|---|
| OUTPUT (as console stage) | Use `CONSOLE`. `OUTPUT` remains the REXX pipeline subcommand only |
| FILE | Use `DISK` |
| HOST | Use `COMMAND` |
| DROP → HOLE | `DROP` is its own stage (phase 1). It does not alias `HOLE` |

Kept: `SPECS` → `SPEC` (Appendix A keyword synonym); `CMD` → `COMMAND` (1.1.11 p.300); `TERMINAL` → `CONSOLE` (1.1.11 p.305); `NOTLOCATE` → `NLOCATE`; `TRUNCATE` → `CHOP` (1.1.11 p.289).

---

## Phase 1 — Selection and trimming

Shipped. TAKE, DROP, NLOCATE, and STRIP are in the done table. BETWEEN slipped to phase 2.

Exit criteria: `… | take 20 | nlocate /x/ | strip | console` without REXX.

---

## Phase 2 — Record reshape

BETWEEN, SPLIT, JOIN, JOINCONT, CHOP, UNIQUE, DUPLICATE, and PICK have shipped (see Done table).

| Stage | Status | Notes |
|---|---|---|
| ZONE | phase-2 | Run selection on a column subset |

Exit criteria: split → sort → unique → join style pipes work.

---

## Phase 3 — Deepen SPEC

One stage, large surface — expand `builtins::spec` for IBM **SPEC**, not new verbs.

| Item | Status | Notes |
|---|---|---|
| Field separators | phase-3 | |
| Ranges `n-m` | phase-3 | |
| Insertion / next-word | phase-3 | |
| Mixed literal + field builds | phase-3 | Enough to replace many CHANGE/PICK uses |

Cite Author’s Edition SPEC chapters when expanding; add dedicated tests.

---

## Phase 4 — Multistream and flow control

| Stage / feature | Status | Notes |
|---|---|---|
| FANINANY fidelity | done | Any-ready via `read_any` (1.1.11 p.339). Named stream identifiers still omitted. |
| ELASTIC | phase-4 | Stall-avoiding buffer |
| GATE | phase-4 | Pass until stopped |
| PREFACE | phase-4 | Inject before primary input |
| APPEND | phase-4 | Inject after device driver output |
| Streaming CALLPIPE | phase-4 | Replace buffering bridges in dispatcher |

---

## Phase 5 — Portable device / host extras (thin)

Only IBM verbs where a portable OS mapping is obvious.

| Stage | Status | Notes |
|---|---|---|
| STEM | phase-5 | REXX/Regina stem variables |
| VAR | phase-5 | Single variable in a REXX/CLIST pool |
| VARSET / VARFETCH | phase-5 | Set/fetch helpers in the same family |
| DATECONVERT | phase-5 | Only if a portable subset is useful; no invented `DATE` stage |

`DISK` remains the portable diskfast verb. `<` / `>` / `>>` are registered with a host-path operand (see architecture.md). CMS `fn ft fm`, `<MDSK`, `>SFS`, and the rest of the disk-driver family stay out of scope.

---

## Phase 6 — Polish / on-demand

Niche IBM filters only when a real pipe needs them:

| Stage | Status | Notes |
|---|---|---|
| OVERLAY | phase-6 | |
| XLATE | phase-6 | ASCII/code-page subset only |
| PACK / UNPACK | phase-6 | On demand |
| PAD | phase-6 | |
| REVERSE | phase-6 | |
| INSERT | phase-6 | |
| FIND / NFIND | phase-6 | XEDIT-style; lower priority than LOCATE |
| FRLABEL / TOLABEL / WHILELABEL | phase-6 | Label runs |
| INSIDE / OUTSIDE / NOTINSIDE | phase-6 | Label-range family with BETWEEN |
| COLLATE / MERGE / DEAL / GATHER | phase-6 | Multistream helpers as needed |
| CASEI | phase-6 | Case-insensitive wrapper |
| NOT | phase-6 | Invert primary/secondary |
| FILLUP | phase-6 | |
| COPY | phase-6 | One-record delay fidelity if still short |

Track intentional support vs out-of-scope here as the catalog grows.

---

## Out of scope

Permanently skipped even if listed in Appendix A. Do not invent portable approximations under these names.

### Device / platform

| Family | Examples |
|---|---|
| CMS/SFS/MDSK disk drivers | `<MDSK`, `>SFS`, DISKFAST, DISKSLOW, MDSK*, SFS* (bare `<` / `>` / `>>` are implemented with a host-path operand) |
| MVS / PDS / QSAM | `<MVS`, `>MVS`, LISTPDS, READPDS, WRITEPDS, QSAM, MEMBERS |
| OpenExtensions / HFS | `<OE`, `>OE`, HFS*, FILEDESCRIPTOR |
| 3270 / fullscreen | FULLSCR*, BROWSE, BUILDSCR |
| Tape / unit record | TAPE, PUNCH, READER, URO, CARD |
| Track / CKD | TRACK*, CKDDEBLOCK, BLOCK (external CKD formats) |

### Host / networking / subsystem

| Family | Examples |
|---|---|
| CP / CMS / TSO command stages | CP, CMS, TSO (full CMS/TSO fidelity) |
| TCP/UDP / resolver | TCPCLIENT, TCPDATA, TCPLISTEN, UDP, HOSTBY*, HOSTID, HOSTNAME |
| SQL / DB2 | SQL, SQLSELECT, SQLCODES, ARIRVSTC |
| ISPF / XEDIT / HLASM | ISPF, XEDIT, XMSG, HLASM* |
| IUCV / VMCF / storage peek | IUCV, VMCF, STORAGE, NUCEXT |
| DFSORT / IEBCOPY / maclib | DFSORT, IEBCOPY, MACLIB |

### Encoding / mainframe-only transforms

EBCDIC as default, APLENCODE/APLDECODE, ASA/CCW carriage stages, overstrike/3270 graphic escapes, cipher/digest stages tied to CMS services.

---

## Working agreement

1. Feature branch `cursor/builtins-phase-N-…` from `master`.
2. IBM names and primary/secondary semantics; document ambiguities in [`architecture.md`](architecture.md).
3. Prefer one solid stage with tests over five half-ports.
4. REXX examples may prototype behavior, then graduate to a built-in when stable.
5. Do not walk Appendix A A–Z — implement by phase unlock, not alphabet.
6. Never add a roadmap row for a verb that is not in Appendix A / Ch. 19.
