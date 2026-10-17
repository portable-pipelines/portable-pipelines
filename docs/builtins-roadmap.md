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

| Stage | Status | Notes |
|---|---|---|
| LITERAL | done | |
| CONSOLE | done | |
| DISK | subset | Portable line-oriented stand-in for CMS `disk` / `diskfast` |
| COMMAND | subset | IBM issues CMS/TSO commands; our impl is a portable process runner |
| LOCATE | done | Primary/secondary |
| CHANGE | done | First occurrence |
| COUNT | done | |
| SORT | done | |
| BUFFER | done | |
| FANOUT | done | |
| FANIN | done | |
| FANINANY | subset | Approximates FANIN today; fidelity in phase 4 |
| HOLE | done | Discard all records |
| SPEC | subset | WORD, literals, `1-*`; deepen in phase 3. Keyword `SPECS` → `SPEC` |
| COPY | done | |
| REXX | subset | Regina preferred; subset interp fallback; `*.rexx` / `*.rex` paths |
| TAKE | subset | `TAKE` / `TAKE FIRST n` / `TAKE LAST n` / `*` / `0`. Record counts. `BYTES` rejected |
| DROP | subset | Converse of TAKE. Record counts. `BYTES` rejected. Not an alias of HOLE |
| NLOCATE | done | Inverse of the LOCATE contains-subset. No operand selects empty records. Synonym `NOTLOCATE` |
| STRIP | subset | `BOTH` / `LEADING` / `TRAILING`, blank, a character or delimited set, `ANYOF`, `NOT` / `TO`, `ANYCASE`, optional count. `STRING` and xrange rejected |

### Removed non-IBM registry aliases

These were registered earlier and have been dropped so the registry matches Appendix A:

| Former alias | Replacement |
|---|---|
| OUTPUT (as console stage) | Use `CONSOLE`. `OUTPUT` remains the REXX pipeline subcommand only |
| FILE | Use `DISK` |
| HOST / CMD | Use `COMMAND` |
| DROP → HOLE | `DROP` is its own stage (phase 1). It does not alias `HOLE` |

Kept: `SPECS` → `SPEC` (Appendix A keyword synonym).

---

## Phase 1 — Selection and trimming

Shipped. TAKE, DROP, NLOCATE, and STRIP are in the done table. BETWEEN slipped to phase 2.

Exit criteria: `… | take 20 | nlocate /x/ | strip | console` without REXX.

---

## Phase 2 — Record reshape

| Stage | Status | Notes |
|---|---|---|
| BETWEEN | phase-2 | Select between labels. Slipped from phase 1. Complements OUTSIDE / NOTINSIDE / INSIDE stay in phase 6 |
| SPLIT | phase-2 | Break on delimiter / width |
| JOIN | phase-2 | Reassemble records |
| JOINCONT | phase-2 | Continuation-line join |
| CHOP | phase-2 | Truncate (prototype: `examples/chop.rexx`) |
| UNIQUE | phase-2 | Discard or retain duplicate lines |
| DUPLICATE | phase-2 | Copy records (repeat) |
| PICK | phase-2 | Relation select without full SPEC |
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
| FANINANY fidelity | phase-4 | Real any-ready semantics |
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

`DISK` is the portable disk verb today; fuller `diskfast` / `<` / `>` fidelity stays out of scope unless a clear portable subset is justified.

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
| CMS/SFS/MDSK disk drivers | `<`, `>`, `>>`, `<MDSK`, `>SFS`, DISKFAST, DISKSLOW, MDSK*, SFS* |
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
