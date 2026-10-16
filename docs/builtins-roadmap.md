# Built-in stage roadmap

Checklist of IBM CMS/TSO Pipelines stages we care about for this portable port.
Behavioral authority: Author’s Edition Appendix A / Ch. 19 (see [`reference/authors-edition-map.md`](reference/authors-edition-map.md)).
Engine decisions stay in [`architecture.md`](architecture.md).

Statuses:

| Status | Meaning |
|---|---|
| `done` | Registered in `pipe_stages::StageRegistry` and usable |
| `phase-N` | Planned in that phase (not implemented yet) |
| `subset` | Present, but not full Author’s Edition surface |
| `out-of-scope` | Intentionally skipped (CMS/z-specific or non-portable) |

Rule: every new stage gets a compat or unit test under `tests/compat/` or `crates/pipe-stages/tests/`.

---

## Done (foundation)

| Stage | Status | Notes |
|---|---|---|
| LITERAL | done | |
| CONSOLE | done | |
| OUTPUT | done | Alias of CONSOLE (device write) |
| FILE | done | Line-oriented portable disk |
| DISK | done | Portable alias of FILE |
| HOST | done | Portable process runner |
| COMMAND / CMD | done | Aliases of HOST |
| LOCATE | done | Primary/secondary |
| CHANGE | done | First occurrence |
| COUNT | done | |
| SORT | done | |
| BUFFER | done | |
| FANOUT | done | |
| FANIN | done | |
| FANINANY | subset | Approximates FANIN today; fidelity in phase 4 |
| HOLE | done | Discard all records |
| DROP | subset | **Today aliases HOLE** — wrong vs IBM; fix in phase 1 |
| SPECS / SPEC | subset | WORD, literals, `1-*`; deepen in phase 3 |
| COPY | done | |
| REXX / `*.rexx` | subset | Regina preferred; subset interp fallback |

---

## Phase 1 — Selection and trimming

Highest leverage everyday filters.

| Stage | Status | Notes |
|---|---|---|
| TAKE | phase-1 | First N records (prototype: `examples/take.rexx`) |
| TAKELAST | phase-1 | Last N records |
| DROP | phase-1 | First N discard (split from HOLE alias) |
| DROPLAST | phase-1 | Last N discard |
| NLOCATE / NOTLOCATE | phase-1 | Inverse of LOCATE |
| STRIP | phase-1 | Leading/trailing pad/whitespace |
| BETWEEN | phase-1 | Range/window selection (or slip to phase 2) |
| NOTBETWEEN | phase-1 | Complement of BETWEEN |

Exit criteria: `file … \| take 20 \| nlocate /x/ \| strip \| console` without REXX.

---

## Phase 2 — Record reshape

| Stage | Status | Notes |
|---|---|---|
| SPLIT | phase-2 | Break on delimiter / width |
| JOIN | phase-2 | Reassemble records |
| JOINCONT | phase-2 | Continuation-line join |
| CHOP | phase-2 | Truncate/pad (prototype: `examples/chop.rexx`) |
| UNIQUE | phase-2 | Discard adjacent duplicates |
| DUPLICATE / DUPLICATES | phase-2 | Emit or select duplicates |
| PICK | phase-2 | Relation/column select without full SPECS |
| ZONE | phase-2 | Run selection on a column subset |

Exit criteria: split → sort → unique → join style pipes work.

---

## Phase 3 — Deepen SPECS

One stage, large surface — expand `builtins::specs`, not many tiny verbs.

| Item | Status | Notes |
|---|---|---|
| Field separators | phase-3 | |
| Ranges `n-m` | phase-3 | |
| Insertion / next-word | phase-3 | |
| Mixed literal + field builds | phase-3 | Enough to replace many CHANGE/PICK uses |

Cite Author’s Edition SPECS chapters when expanding; add dedicated tests.

---

## Phase 4 — Multistream and flow control

| Stage / feature | Status | Notes |
|---|---|---|
| FANINANY fidelity | phase-4 | Real any-ready semantics |
| ELASTIC | phase-4 | Stall-avoiding buffer |
| ASTRICT | phase-4 | If needed for stall cases |
| GATE | phase-4 | Pass until stopped |
| PREFACE | phase-4 | Inject before primary input |
| APPEND | phase-4 | Inject after device driver output |
| Streaming CALLPIPE | phase-4 | Replace buffering bridges in dispatcher |

---

## Phase 5 — Portable device / host extras (thin)

Only where the OS mapping is obvious.

| Stage | Status | Notes |
|---|---|---|
| STEM | phase-5 | REXX/Regina stem variables |
| VAR / VARSET / VARFETCH | phase-5 | Single-variable pool access |
| DATECONVERT (or thin DATE emitter) | phase-5 | Synthetic records only — not CMS QUERY |
| FILE synonyms | phase-5 | Add IBM disk verb synonyms only if useful |

Keep **FILE** as the primary portable disk verb.

---

## Phase 6 — Polish / on-demand

Niche filters only when a real pipe needs them:

| Stage | Status | Notes |
|---|---|---|
| OVERLAY | phase-6 | |
| XLATE | phase-6 | ASCII subset only |
| PACK / UNPACK | phase-6 | On demand |
| PAD | phase-6 | |
| REVERSE | phase-6 | |
| INSERT | phase-6 | |
| FIND / NFIND | phase-6 | XEDIT-style; lower priority than LOCATE |
| FRLABEL / TOLABEL / WHILELABEL | phase-6 | Label runs |
| INSIDE / OUTSIDE / NOTINSIDE | phase-6 | If not covered by BETWEEN |
| COLLATE / MERGE / DEAL / GATHER | phase-6 | Multistream helpers as needed |
| CASEI | phase-6 | Case-insensitive wrapper |
| NOT | phase-6 | Invert primary/secondary |
| FILLUP | phase-6 | |
| COPY delay semantics | phase-6 | One-record delay fidelity |

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
| CP / CMS / TSO command stages | CP, CMS, TSO, COMMANDS (CMS-specific) |
| TCP/UDP / resolver | TCPCLIENT, TCPDATA, TCPLISTEN, UDP, HOSTBY* |
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
