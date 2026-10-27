# What Portable Pipelines can do today

Portable Pipelines is a portable implementation of IBM CMS Pipelines for Windows, macOS, and Linux. If you already use CMS Pipelines, or you are wondering whether this port can do a job you have in mind, this page is the public picture of what works today.

The Author's Edition is John Hartmann's reference for CMS/TSO Pipelines. This page is measured against Author's Edition 1.1.11 (SL26-0018-05), including its catalog of built-in programs (Appendix A / Chapter 19).

The numbers below match this project's coverage of the Author's Edition catalog of 254 built-in programs. Stage names are the IBM names, written in uppercase. Detailed tables are grouped by what a stage helps you do. The glance tables at the top are alphabetical, so you can find a name quickly.

Records here are bytes. On a PC or Unix host that usually means ordinary host text, not EBCDIC. File stages take a host path such as `notes.txt` or `/tmp/out`, not a CMS file name of the form `fn ft fm`.

The README shows how to build `pipe` and run a specification.

## Symbols

The same marks are used in headings, summary tables, glance tables, and the detailed rows.

| Symbol | Status | Meaning |
|---|---|---|
| ✅ | Works | The stage behaves as the Author's Edition describes. |
| 🟡 | Works, with limits | The stage is present. The row names what is still missing. |
| 📅 | Planned | Coming. Nothing with this mark is available to run today. |
| ⛔ | Not planned | Mainframe-only. This port will not implement it. |
| ✍️ | Writing your own | A REXX or Python program you write and name in the pipeline. |
| 💡 | Ideas | A wish-list extension beyond the IBM catalog. Not a plan. |

Of those 254 catalog entries, **111** built-in programs work here today (✅ and 🟡 together). **9** further stages that map cleanly onto a portable host are 📅 planned. **133** programs that depend on CMS, z/VM, or other mainframe facilities are ⛔ not planned. The remaining catalog entry is a nested-pipeline behaviour (streaming CALLPIPE) that is 📅 planned.

User-written REXX and Python stages sit beside that catalog; they are not extra IBM built-ins. REXX pipeline commands are counted separately: **3** are ✅ complete, **4** are 🟡 limited, and **19** are 📅 still to come.

### Status summary

| Status | Count | What it means |
|---|---|---|
| ✅ Works | part of the 111 | Behaves as the Author's Edition describes. |
| 🟡 Works, with limits | part of the 111 | Present and useful. Each detailed row names what is still missing. |
| ✅ and 🟡 together | **111** of 254 | Catalog programs you can run today. |
| 📅 Planned | **9** host stages, plus streaming CALLPIPE | Coming. Nothing in that section runs today. |
| ⛔ Not planned | **133** programs, shown as **16** families | Mainframe-only. This port will not implement them. |
| ✍️ REXX pipeline commands | **3** ✅, **4** 🟡, **19** 📅 | Commands inside a REXX stage. Counted separately from the 254. |

### All stages at a glance

Implemented stages, A to Z. The mark is the status. The name jumps to the detailed row.

| Stage | Stage | Stage |
|---|---|---|
| 🟡 [`<`](#s-lt) | 🟡 [`>`](#s-gt) | 🟡 [`>>`](#s-gtgt) |
| ✅ [64DECODE](#s-64decode) | ✅ [64ENCODE](#s-64encode) | ✅ [ABBREV](#s-abbrev) |
| ✅ [AGGRC](#s-aggrc) | ✅ [ALL](#s-all) | ✅ [APPEND](#s-append) |
| ✅ [BEAT](#s-beat) | ✅ [BETWEEN](#s-between) | ✅ [BUFFER](#s-buffer) |
| ✅ [CASEI](#s-casei) | 🟡 [CHANGE](#s-change) | ✅ [CHOP](#s-chop) |
| ✅ [COLLATE](#s-collate) | ✅ [COMBINE](#s-combine) | 🟡 [COMMAND](#s-command) |
| ✅ [CONSOLE](#s-console) | ✅ [COPY](#s-copy) | ✅ [COUNT](#s-count) |
| ✅ [DAM](#s-dam) | ✅ [DEAL](#s-deal) | 🟡 [DEBLOCK](#s-deblock) |
| ✅ [DELAY](#s-delay) | ✅ [DIGEST](#s-digest) | 🟡 [DISK](#s-disk) |
| 🟡 [DROP](#s-drop) | ✅ [DUPLICATE](#s-duplicate) | ✅ [ELASTIC](#s-elastic) |
| ✅ [EOFBACK](#s-eofback) | ✅ [ESCAPE](#s-escape) | 🟡 [FANIN](#s-fanin) |
| ✅ [FANINANY](#s-faninany) | ✅ [FANINTWO](#s-fanintwo) | 🟡 [FANOUT](#s-fanout) |
| ✅ [FANOUTWO](#s-fanoutwo) | ✅ [FILLUP](#s-fillup) | ✅ [FIND](#s-find) |
| ✅ [FRLABEL](#s-frlabel) | ✅ [FRTARGET](#s-frtarget) | ✅ [GATE](#s-gate) |
| ✅ [GATHER](#s-gather) | ✅ [HOLE](#s-hole) | ✅ [HTTPSPLIT](#s-httpsplit) |
| ✅ [IF](#s-if) | ✅ [INSERT](#s-insert) | ✅ [INSIDE](#s-inside) |
| ✅ [JOIN](#s-join) | ✅ [JOINCONT](#s-joincont) | ✅ [JUXTAPOSE](#s-juxtapose) |
| ✅ [LITERAL](#s-literal) | 🟡 [LOCATE](#s-locate) | ✅ [LOOKUP](#s-lookup) |
| ✅ [MERGE](#s-merge) | ✅ [NFIND](#s-nfind) | 🟡 [NLOCATE](#s-nlocate) |
| ✅ [NOEOFBACK](#s-noeofback) | ✅ [NOT](#s-not) | ✅ [NOTINSIDE](#s-notinside) |
| ✅ [OUTSIDE](#s-outside) | ✅ [OVERLAY](#s-overlay) | ✅ [PAD](#s-pad) |
| ✅ [PARCEL](#s-parcel) | ✅ [PICK](#s-pick) | 🟡 [PIPCMD](#s-pipcmd) |
| 🟡 [PIPESTOP](#s-pipestop) | ✅ [PREDSELECT](#s-predselect) | ✅ [PREFACE](#s-preface) |
| ✅ [QPDECODE](#s-qpdecode) | ✅ [QPENCODE](#s-qpencode) | ✅ [RANDOM](#s-random) |
| ✅ [RETAB](#s-retab) | ✅ [REVERSE](#s-reverse) | 🟡 [RUNPIPE](#s-runpipe) |
| ✅ [SNAKE](#s-snake) | 🟡 [SORT](#s-sort) | ✅ [SPACE](#s-space) |
| 🟡 [SPEC](#s-spec) | ✅ [SPILL](#s-spill) | ✅ [SPLIT](#s-split) |
| ✅ [STRFIND](#s-strfind) | ✅ [STRFRLABEL](#s-strfrlabel) | ✅ [STRIP](#s-strip) |
| ✅ [STRLITERAL](#s-strliteral) | ✅ [STRNFIND](#s-strnfind) | 🟡 [STRTOLABEL](#s-strtolabel) |
| 🟡 [STRWHILELABEL](#s-strwhilelabel) | ✅ [SUBSTRING](#s-substring) | ✅ [SYNCHRONISE](#s-synchronise) |
| 🟡 [TAKE](#s-take) | ✅ [THREEWAY](#s-threeway) | ✅ [TOKENISE](#s-tokenise) |
| 🟡 [TOLABEL](#s-tolabel) | ✅ [TOTARGET](#s-totarget) | ✅ [UNIQUE](#s-unique) |
| ✅ [UNTAB](#s-untab) | ✅ [UPDATE](#s-update) | ✅ [URLDEBLOCK](#s-urldeblock) |
| ✅ [VERIFY](#s-verify) | 🟡 [WHILELABEL](#s-whilelabel) | ✅ [WILDCARD](#s-wildcard) |
| 🟡 [XLATE](#s-xlate) | ✅ [XRANGE](#s-xrange) | 🟡 [ZONE](#s-zone) |

📅 Planned host stages (the 9 remaining portable extras):

| Stage | Stage | Stage |
|---|---|---|
| 📅 [DATECONVERT](#s-dateconvert) | 📅 [GETFILES](#s-getfiles) | 📅 [GREG2SEC](#s-greg2sec) |
| 📅 [REXXVARS](#s-rexxvars) | 📅 [SEC2GREG](#s-sec2greg) | 📅 [STACK](#s-stack) |
| 📅 [STEM](#s-stem) | 📅 [SUBCOM](#s-subcom) | 📅 [SYSVAR](#s-sysvar) |
| 📅 [TIMESTAMP](#s-timestamp) | 📅 [VAR](#s-var) | 📅 [VARDROP](#s-vardrop) |
| 📅 [VARFETCH](#s-varfetch) | 📅 [VARLOAD](#s-varload) | 📅 [VARSET](#s-varset) |

Also planned: 📅 [streaming CALLPIPE](#streaming-callpipe), 📅 [multi-stream Python](#python-multistream), and the 📅 REXX commands in the next table.

✍️ REXX pipeline commands (inside a REXX stage, not built-in programs):

| Command | Command | Command |
|---|---|---|
| 🟡 [ADDPIPE](#rexx-commands) | 📅 [ADDSTREAM](#rexx-commands) | 📅 [BEGOUTPUT](#rexx-commands) |
| 🟡 [CALLPIPE](#rexx-commands) | 🟡 [COMMIT](#rexx-commands) | 📅 [EOFREPORT](#rexx-commands) |
| 📅 [GETRANGE](#rexx-commands) | 📅 [ISSUEMSG](#rexx-commands) | 📅 [MAXSTREAM](#rexx-commands) |
| 📅 [MESSAGE](#rexx-commands) | 📅 [NOCOMMIT](#rexx-commands) | ✅ [OUTPUT](#rexx-commands) |
| 📅 [PEEKTO](#rexx-commands) | ✅ [READTO](#rexx-commands) | 📅 [RESOLVE](#rexx-commands) |
| 📅 [REXX command](#rexx-commands) | 📅 [SCANRANGE](#rexx-commands) | 📅 [SCANSTRING](#rexx-commands) |
| 🟡 [SELECT](#rexx-commands) | 📅 [SETRC](#rexx-commands) | 📅 [SEVER](#rexx-commands) |
| ✅ [SHORT](#rexx-commands) | 📅 [STAGENUM](#rexx-commands) | 📅 [STREAMNUM](#rexx-commands) |
| 📅 [STREAMSTATE](#rexx-commands) | 📅 [SUSPEND](#rexx-commands) | |

User-written programs: ✍️ [REXX stages](#rexx-stages), ✍️ [Python stages](#python-stages).

⛔ Mainframe-only families (133 programs, not listed one by one):

| Family | Family | Family |
|---|---|---|
| ⛔ [CMS minidisk and SFS](#f-cms-minidisk-and-sfs) | ⛔ [MVS datasets and PDS](#f-mvs-datasets-and-pds) | ⛔ [OpenExtensions / HFS](#f-openextensions-hfs) |
| ⛔ [3270 and fullscreen](#f-3270-and-fullscreen) | ⛔ [Tape and unit record](#f-tape-and-unit-record) | ⛔ [Track and CKD](#f-track-and-ckd) |
| ⛔ [CP, CMS, and TSO command stages](#f-cp-cms-and-tso-command-stages) | ⛔ [CP services](#f-cp-services) | ⛔ [TCP / UDP and the resolver](#f-tcp-udp-and-the-resolver) |
| ⛔ [SQL / DB2](#f-sql-db2) | ⛔ [ISPF, XEDIT, and HLASM](#f-ispf-xedit-and-hlasm) | ⛔ [IUCV, VMCF, and storage peek](#f-iucv-vmcf-and-storage-peek) |
| ⛔ [DFSORT, IEBCOPY, and maclib](#f-dfsort-iebcopy-and-maclib) | ⛔ [File-status, loader tables, and service commands](#f-file-status-loader-tables-and-service-commands) | ⛔ [EBCDIC transforms](#f-ebcdic-transforms) |
| ⛔ [Niche mainframe record formats](#f-niche-mainframe-record-formats) | | |

---

<a id="works"></a>
## ✅ Works

These stages behave as the Author's Edition describes.

### Console and generated records

| Stage | What it does | Notes |
|---|---|---|
| ✅ <a id="s-console"></a>CONSOLE | Reads the terminal when it is first, or writes the terminal and copies records through when it is not. | Also TERMINAL. CMS-only fullscreen options are rejected. |
| ✅ <a id="s-literal"></a>LITERAL | Writes one argument record, then copies its input. | An omitted argument writes a null record. A cascade writes in reverse order. |
| ✅ <a id="s-strliteral"></a>STRLITERAL | The delimited-string form of LITERAL, with PREFACE, APPEND, or IFEMPTY. | Default is PREFACE. |
| ✅ <a id="s-xrange"></a>XRANGE | Writes one record that is an inclusive range of characters. | Must be first. With no argument it writes every byte from X'00' through X'FF'. |
| ✅ <a id="s-random"></a>RANDOM | Writes 4-byte random values until the output closes. | Must be first. Uses the host clock when you omit the seed. |

### Selecting records

| Stage | What it does | Notes |
|---|---|---|
| ✅ <a id="s-between"></a>BETWEEN | Keeps records from one label through another, labels included. | |
| ✅ <a id="s-inside"></a>INSIDE | Keeps records between two labels, labels excluded. | |
| ✅ <a id="s-notinside"></a>NOTINSIDE | The converse of INSIDE. | Also NINSIDE. |
| ✅ <a id="s-outside"></a>OUTSIDE | The converse of BETWEEN. | |
| ✅ <a id="s-pick"></a>PICK | Selects records by a string or numeric comparison of two fields or strings. | |
| ✅ <a id="s-find"></a>FIND | Selects records that match an XEDIT FIND string. | |
| ✅ <a id="s-nfind"></a>NFIND | The converse of FIND. | Also NOTFIND. |
| ✅ <a id="s-frlabel"></a>FRLABEL | Copies from the first prefix match through the end of the file. | Also FROMLABEL. |
| ✅ <a id="s-strfind"></a>STRFIND | FIND with a delimited string, hex, or binary argument. | |
| ✅ <a id="s-strnfind"></a>STRNFIND | The converse of STRFIND. | |
| ✅ <a id="s-strfrlabel"></a>STRFRLABEL | FRLABEL with a delimited string. | Also STRFROMLABEL. |
| ✅ <a id="s-abbrev"></a>ABBREV | Selects records whose first word is an abbreviation of the argument. | |
| ✅ <a id="s-verify"></a>VERIFY | Selects records that contain only characters from a given set. | |
| ✅ <a id="s-all"></a>ALL | Selects records that match a boolean combination of substring tests. | |
| ✅ <a id="s-wildcard"></a>WILDCARD | Selects records that match a `*` / `%` pattern. | |
| ✅ <a id="s-frtarget"></a>FRTARGET | Copies from the first record selected by another stage through the end of the file. | Also FROMTARGET. |
| ✅ <a id="s-totarget"></a>TOTARGET | Copies through the first record selected by another stage. | |
| ✅ <a id="s-casei"></a>CASEI | Runs another stage with case-insensitive matching and writes the original records. | |
| ✅ <a id="s-not"></a>NOT | Runs another stage and swaps its primary and secondary outputs. | |

### Changing and reshaping records

| Stage | What it does | Notes |
|---|---|---|
| ✅ <a id="s-strip"></a>STRIP | Removes leading and trailing characters or strings. | Default is both ends, blank. |
| ✅ <a id="s-split"></a>SPLIT | Breaks records at a target. | |
| ✅ <a id="s-join"></a>JOIN | Joins records back together. | |
| ✅ <a id="s-joincont"></a>JOINCONT | Joins continuation lines. | |
| ✅ <a id="s-chop"></a>CHOP | Truncates records and can send the remainder to a second output. | Also TRUNCATE. |
| ✅ <a id="s-duplicate"></a>DUPLICATE | Writes extra copies of each record. | |
| ✅ <a id="s-insert"></a>INSERT | Inserts a string before or after a field. | |
| ✅ <a id="s-pad"></a>PAD | Pads short records on the left or right. | |
| ✅ <a id="s-reverse"></a>REVERSE | Reverses the bytes of each record. | |
| ✅ <a id="s-retab"></a>RETAB | Replaces runs of blanks with tab characters. | Converse of UNTAB. |
| ✅ <a id="s-untab"></a>UNTAB | Replaces tab characters with blanks. | Converse of RETAB. |
| ✅ <a id="s-snake"></a>SNAKE | Lays records into columns, down then across. | |
| ✅ <a id="s-space"></a>SPACE | Collapses delimiter runs the way REXX SPACE does. | |
| ✅ <a id="s-tokenise"></a>TOKENISE | Splits a record into tokens. | Also TOKENIZE. |
| ✅ <a id="s-substring"></a>SUBSTRING | Writes a chosen range of each record. | |
| ✅ <a id="s-spill"></a>SPILL | Splits long lines at word boundaries. | |

### Sorting, matching, and counting

| Stage | What it does | Notes |
|---|---|---|
| ✅ <a id="s-count"></a>COUNT | Counts characters, words, lines, or the shortest and longest records. | At least one keyword is required. |
| ✅ <a id="s-unique"></a>UNIQUE | Discards or keeps adjacent duplicates. | |
| ✅ <a id="s-collate"></a>COLLATE | Collates a master stream with a detail stream. | Comparison is host byte order. |
| ✅ <a id="s-merge"></a>MERGE | Merges already-ordered streams. | Up to ten inputs. Comparison is host byte order. |
| ✅ <a id="s-lookup"></a>LOOKUP | Looks up details against a reference by key. | |

### Combining, splitting, and controlling streams

| Stage | What it does | Notes |
|---|---|---|
| ✅ <a id="s-buffer"></a>BUFFER | Holds records until the end of the file, or repeats null-delimited subfiles. | |
| ✅ <a id="s-faninany"></a>FANINANY | Reads whichever input has a record ready. | Stream identifiers are numeric. |
| ✅ <a id="s-fanintwo"></a>FANINTWO | Two-stream fan-in; the secondary input has priority. | |
| ✅ <a id="s-fanoutwo"></a>FANOUTWO | Copies each record to both outputs. | End-of-file on the primary output travels backwards. |
| ✅ <a id="s-fillup"></a>FILLUP | Writes to the next output when the current one fills up. | |
| ✅ <a id="s-threeway"></a>THREEWAY | Splits each record onto three outputs (prefix, range, rest). | Also 3WAY. |
| ✅ <a id="s-deal"></a>DEAL | Sends records to output streams by round-robin, key, or stream id. | Stream identifiers are numeric. |
| ✅ <a id="s-gather"></a>GATHER | The converse of DEAL: gathers records from input streams. | Stream identifiers are numeric. |
| ✅ <a id="s-hole"></a>HOLE | Reads every connected input and writes nothing. | |
| ✅ <a id="s-copy"></a>COPY | Consumes each record, then writes it (one-record delay). | |
| ✅ <a id="s-preface"></a>PREFACE | Runs a nested pipeline first, then copies the primary input. | |
| ✅ <a id="s-append"></a>APPEND | Copies the primary input, then runs a nested pipeline. | |
| ✅ <a id="s-elastic"></a>ELASTIC | Buffers so a pipeline is less likely to stall. | |
| ✅ <a id="s-gate"></a>GATE | Passes higher streams until a primary record arrives, then stops. | |
| ✅ <a id="s-if"></a>IF | Runs a selection stage as an if-then or if-then-else. | |
| ✅ <a id="s-dam"></a>DAM | Waits for a primary record, then stops every stream pair. | Converse of GATE. |
| ✅ <a id="s-juxtapose"></a>JUXTAPOSE | Holds each primary record and prefixes it to each following secondary record. | |
| ✅ <a id="s-synchronise"></a>SYNCHRONISE | Waits for a record on every input, then copies one from each. | Also SYNCHRONIZE. |
| ✅ <a id="s-predselect"></a>PREDSELECT | Routes the original record after a test on copies. | |
| ✅ <a id="s-noeofback"></a>NOEOFBACK | Passes records while the output is connected; then consumes the rest. | Also NOTEOFBACK. |
| ✅ <a id="s-eofback"></a>EOFBACK | Copies through, then runs a nested output stage so end-of-file can travel backwards. | |
| ✅ <a id="s-aggrc"></a>AGGRC | Combines numeric return codes (most negative, otherwise the largest). | |
| ✅ <a id="s-overlay"></a>OVERLAY | Overlays columns from every connected input. | |
| ✅ <a id="s-combine"></a>COMBINE | Combines a run of records, bitwise or by FIRST/LAST column. | |
| ✅ <a id="s-parcel"></a>PARCEL | Cuts a byte stream into records using lengths from a second input. | |
| ✅ <a id="s-httpsplit"></a>HTTPSPLIT | Splits an HTTP stream into header records and a body. | ASCII by default. |
| ✅ <a id="s-update"></a>UPDATE | Applies a CMS-format UPDATE file from the secondary input onto a master. | Sequence numbers are host digits. |
| ✅ <a id="s-delay"></a>DELAY | Waits until a time of day or for an interval taken from each record. | Uses the host clock. |
| ✅ <a id="s-beat"></a>BEAT | Copies records and writes a marker on a second output when input is quiet. | Uses the host clock. |

### Encoding

| Stage | What it does | Notes |
|---|---|---|
| ✅ <a id="s-64encode"></a>64ENCODE | MIME Base-64 encode. | ASCII alphabet. |
| ✅ <a id="s-64decode"></a>64DECODE | MIME Base-64 decode. | ASCII alphabet. |
| ✅ <a id="s-qpencode"></a>QPENCODE | MIME quoted-printable encode. | ASCII domain. |
| ✅ <a id="s-qpdecode"></a>QPDECODE | MIME quoted-printable decode. | ASCII domain. |
| ✅ <a id="s-digest"></a>DIGEST | Computes SHA-1, SHA-256, SHA-384, SHA-512, or MD5. | Software implementation. |
| ✅ <a id="s-escape"></a>ESCAPE | Prefixes listed characters with an escape character. | |
| ✅ <a id="s-urldeblock"></a>URLDEBLOCK | Splits form-urlencoded data into fields. | ASCII by default. |

---

<a id="works-with-limits"></a>
## 🟡 Works, with limits

These stages are present and useful, with the missing piece named in each Limits cell.

### Reading and writing files

File stages take a **host path**, not a CMS `fn ft fm`.

| Stage | What it does | Limits |
|---|---|---|
| 🟡 <a id="s-disk"></a>DISK | First stage reads a file (a missing file is empty). Later it appends or creates the file and copies records through. | Host path only. CMS minidisk and SFS forms are not planned. |
| 🟡 <a id="s-lt"></a>`<` | Must be first. Reads a host file. | Host path only. A missing file is an error. |
| 🟡 <a id="s-gt"></a>`>` | Must not be first. Replaces or creates a host file and copies records through. | Host path only. |
| 🟡 <a id="s-gtgt"></a>`>>` | Must not be first. Appends to or creates a host file and copies records through. | Host path only. |

### Selecting records, with limits

| Stage | What it does | Limits |
|---|---|---|
| 🟡 <a id="s-locate"></a>LOCATE | Keeps records that contain a string (or any character, with ANYOF). | MIXED, ONES, ZEROS, and column ranges are not supported yet. |
| 🟡 <a id="s-nlocate"></a>NLOCATE | The converse of LOCATE. | Same limits as LOCATE. Also NOTLOCATE. |
| 🟡 <a id="s-take"></a>TAKE | Keeps the first or last *n* records. | Byte counts are not supported yet. |
| 🟡 <a id="s-drop"></a>DROP | Discards the first or last *n* records. | Byte counts are not supported yet. DROP is not another name for HOLE. |
| 🟡 <a id="s-tolabel"></a>TOLABEL | Copies through the record before the first prefix match. | The matching record is not left in the pipeline for a nested pipeline to read again. |
| 🟡 <a id="s-whilelabel"></a>WHILELABEL | Copies while records begin with the string. | Same leftover-record limit as TOLABEL. |
| 🟡 <a id="s-strtolabel"></a>STRTOLABEL | TOLABEL with a delimited string. | Same leftover-record limit as TOLABEL. |
| 🟡 <a id="s-strwhilelabel"></a>STRWHILELABEL | WHILELABEL with a delimited string. | Same leftover-record limit as TOLABEL. |
| 🟡 <a id="s-zone"></a>ZONE | Runs another stage on a column subset and writes the original record. | Buffers until the end of the file instead of streaming one record at a time. Some end-of-file behaviour of the nested stage is not matched yet. |

### Changing and reshaping records, with limits

| Stage | What it does | Limits |
|---|---|---|
| 🟡 <a id="s-change"></a>CHANGE | Replaces text in each record. | Column ranges are not supported yet. |
| 🟡 <a id="s-spec"></a>SPEC | Builds output records from fields, words, literals, and placements. | The expression language (counters, IF/THEN, SELECT, SET, PRINT, PICTURE) is not supported yet. Neither are TODCLOCK, packed/float/date conversions, or writing to extra streams. Portable conversions C2X, X2C, C2B, B2C, X2B, and B2X work. Also SPECS. |

### Sorting and counting, with limits

| Stage | What it does | Limits |
|---|---|---|
| 🟡 <a id="s-sort"></a>SORT | Sorts records. Default is ascending, whole record. | Field ranges and PAD / NOPAD are not supported yet. UNIQUE and COUNT work. |

### Combining and splitting streams, with limits

| Stage | What it does | Limits |
|---|---|---|
| 🟡 <a id="s-fanout"></a>FANOUT | Copies each record to every connected output. | A list of named stream identifiers is not supported yet. STOP ALLEOF, ANYEOF, and a count work. |
| 🟡 <a id="s-fanin"></a>FANIN | Reads the primary input to the end, then the next input, and so on. | Named stream identifiers are not supported. Numeric stream numbers work. |

### Encoding, with limits

| Stage | What it does | Limits |
|---|---|---|
| 🟡 <a id="s-xlate"></a>XLATE | Translates characters, including upper/lower case and from-to pairs. | Also TRANSLATE. Code-page numbers, A2E / E2A, and live CMS input/output tables are not supported yet. Case tables are ASCII. |
| 🟡 <a id="s-deblock"></a>DEBLOCK | Splits a byte stream into records at line ends. | Only LINEND and CRLF. Other IBM record formats are not supported. |

### Running commands and nested pipelines

| Stage | What it does | Limits |
|---|---|---|
| 🟡 <a id="s-command"></a>COMMAND | Runs a host process and writes its output as records. | Also CMD. This is a portable process runner, not a CMS command. There is no secondary stream for the return code. |
| 🟡 <a id="s-pipcmd"></a>PIPCMD | Reads command records and issues them into the running pipeline. | ADDSTREAM, EOFREPORT, ISSUEMSG, the REXX command, and SETRC are not implemented yet. |
| 🟡 <a id="s-pipestop"></a>PIPESTOP | When a record arrives, wakes stages that are waiting on a timer, then copies the record through. | The CMS-only stop signal is not implemented. DELAY and BEAT are the stages that wake. |
| 🟡 <a id="s-runpipe"></a>RUNPIPE | Runs each input record as a new pipeline and intercepts messages. | Event traces, pause-on-message, and CMS message-level bits are not implemented yet. |

---

<a id="writing-your-own"></a>
## ✍️ Writing your own stages

A user-written stage is a program you write that reads records from its input and writes records to its output, used by naming the file in the pipeline.

REXX and Python are the two languages supported today. They fill the same role: a filter you own, sitting in the specification the same way a built-in stage does.

<a id="rexx-stages"></a>
### REXX stages

Write a `.rexx` or `.rex` program that uses pipeline commands. Put the file path in the specification, or write `REXX` and then the path:

```text
pipe "literal hello | examples/upper.rexx | console"
```

A typical filter reads one record, writes one record, and stops at end of file:

```rexx
/* upper.rexx */
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  'OUTPUT' TRANSLATE(LINE)
END
```

When Regina is installed, the program runs on Regina with the PIPELINE environment. When it is not, a small built-in interpreter still runs the common pipeline commands.

<a id="rexx-commands"></a>
**REXX pipeline commands today**

Complete (✅):

- READTO reads or discards one input record into a variable.
- OUTPUT writes a line.
- SHORT connects or severs the selected input and output.

Present, with limits (🟡):

- CALLPIPE runs a subroutine pipeline. It currently reads all of its input, runs to completion, then writes all of its output.
- ADDPIPE adds a specification to the running set. It currently behaves like CALLPIPE.
- SELECT chooses a stream. Only `SELECT INPUT n` and `SELECT OUTPUT n` work.
- COMMIT commits to a new level. It is available when Regina is in use.

Still to come (📅): PEEKTO, ADDSTREAM, BEGOUTPUT, EOFREPORT, GETRANGE, ISSUEMSG, MAXSTREAM, MESSAGE, NOCOMMIT, RESOLVE, the REXX command from a filter, SCANRANGE, SCANSTRING, SETRC, SEVER, STAGENUM, STREAMNUM, STREAMSTATE, and SUSPEND.

Sharing REXX or CLIST variables with the pipeline (STEM, VAR, and the rest of that family) is 📅 planned as built-in stages, not as extra REXX commands. See [📅 Planned](#planned) below.

<a id="python-stages"></a>
### Python stages

Write a `.py` program that reads records from its input and writes records to its output. Put the file path in the specification, the same way you name a REXX program:

```text
pipe "literal hello | examples/upper.py | console"
```

```python
import pipelines as p

for rec in p.records():
    p.output(rec.upper())
```

Each Python stage runs as its own Python 3.10 or later process. You can mix REXX and Python stages in one pipeline. The helper module offers `readto`, `peekto`, `output`, `short`, `records()`, and `exit`.

<a id="python-multistream"></a>
Python stages are single-stream for now: one input and one output. Selecting extra streams, running a nested pipeline from Python, and related calls are 📅 still to come.

---

<a id="planned"></a>
## 📅 Planned

These items are coming, and nothing in this section is available to run today.

<a id="streaming-callpipe"></a>
**Streaming CALLPIPE.** Nested pipelines currently read all of their input, run to completion, then write all of their output. A streaming form, where records can flow through a nested pipeline one at a time, is planned. Until it arrives, TOLABEL, WHILELABEL, STRTOLABEL, and STRWHILELABEL cannot leave their matching record for the caller to read again.

**Portable device and host extras.** These IBM stages have a clear mapping onto Windows, macOS, or Linux, and are planned as a thin host layer:

| Stage | What it is for |
|---|---|
| ✅ <a id="s-stem"></a>STEM | REXX stem variables. When first stage, reads from stem and writes records. When not first, reads records into stem.1, stem.2, etc. (sets stem.0 to count). Regina only, use from REXX program with CALLPIPE. |
| ✅ <a id="s-var"></a>VAR | A single variable in a REXX pool. When first stage, reads from variable and writes records (split on newlines). When not first, reads records and stores in variable (newline-separated). Regina only, use from REXX program with CALLPIPE. |
| ✅ <a id="s-varset"></a>VARSET | Set a variable in the REXX pool. Filter stage. Regina only, use from REXX program with CALLPIPE. |
| ✅ <a id="s-varfetch"></a>VARFETCH | Fetch a variable from the REXX pool. Must be first stage. Regina only, use from REXX program with CALLPIPE. |
| ✅ <a id="s-rexxvars"></a>REXXVARS | Retrieve all variables from the REXX pool as records (name value). Must be first stage. Regina only, use from REXX program with CALLPIPE. |
| ✅ <a id="s-varload"></a>VARLOAD | Set variables from records in the form "name value". Filter stage. Regina only, use from REXX program with CALLPIPE. |
| 📅 <a id="s-vardrop"></a>VARDROP | Drop variables in a REXX pool. |
| 📅 <a id="s-dateconvert"></a>DATECONVERT | Convert dates, if a useful portable form can be defined (there will not be an invented DATE stage). |
| 📅 <a id="s-getfiles"></a>GETFILES | Read host files whose names arrive on the input. |
| 📅 <a id="s-greg2sec"></a>GREG2SEC | Convert a Gregorian timestamp to seconds since 1970. |
| 📅 <a id="s-sec2greg"></a>SEC2GREG | Convert seconds since 1970 to a Gregorian timestamp. |
| 📅 <a id="s-stack"></a>STACK | A FIFO or LIFO program stack. |
| 📅 <a id="s-subcom"></a>SUBCOM | Issue commands to a named subcommand environment. |
| 📅 <a id="s-sysvar"></a>SYSVAR | Write host or system variables into the pipeline. |
| 📅 <a id="s-timestamp"></a>TIMESTAMP | Prefix a date-and-time stamp to each record. |

**Multi-stream Python stages.** Python filters today have one input and one output. Selecting extra streams and running a nested pipeline from Python are planned. See [Python stages](#python-stages).

**Remaining REXX pipeline commands.** The 📅 commands listed as still to come under [REXX pipeline commands](#rexx-commands).

Deeper SPEC work (the expression language, extra conversions, and extra output streams) is part of finishing SPEC, not a new stage. It is noted under [🟡 Works, with limits](#works-with-limits).

---

<a id="not-planned"></a>
## ⛔ Not planned

These families need CMS, z/VM, MVS, a 3270, a tape drive, or another mainframe facility, and this port will not implement them or invent a look-alike under the same name.

File stages you *can* use (`DISK`, `<`, `>`, `>>`) take host paths instead of CMS `fn ft fm`.

| Family | Why it stays out |
|---|---|
| <a id="f-cms-minidisk-and-sfs"></a>⛔ CMS minidisk and SFS | Those drivers address CMS disks and the Shared File System. This port reads and writes host paths. |
| <a id="f-mvs-datasets-and-pds"></a>⛔ MVS datasets and PDS | Those drivers address z/OS datasets, PDS members, and SYSOUT. |
| <a id="f-openextensions-hfs"></a>⛔ OpenExtensions / HFS | Those drivers address z/OS UNIX files and descriptors. |
| <a id="f-3270-and-fullscreen"></a>⛔ 3270 and fullscreen | Those programs drive a 3270 screen. There is no fullscreen terminal here. |
| <a id="f-tape-and-unit-record"></a>⛔ Tape and unit record | Tape, punch, reader, and printer devices are not present on this host. |
| <a id="f-track-and-ckd"></a>⛔ Track and CKD | Those programs speak mainframe disk geometry and related record formats. |
| <a id="f-cp-cms-and-tso-command-stages"></a>⛔ CP, CMS, and TSO command stages | Full CP, CMS, and TSO command fidelity is a mainframe host, not this process runner. |
| <a id="f-cp-services"></a>⛔ CP services | Messages, device info, and related CP queries need z/VM. |
| <a id="f-tcp-udp-and-the-resolver"></a>⛔ TCP / UDP and the resolver | Network and hostname stages are not in this port. |
| <a id="f-sql-db2"></a>⛔ SQL / DB2 | Those stages talk to a mainframe database. |
| <a id="f-ispf-xedit-and-hlasm"></a>⛔ ISPF, XEDIT, and HLASM | Those stages need ISPF, XEDIT, or the assembler. |
| <a id="f-iucv-vmcf-and-storage-peek"></a>⛔ IUCV, VMCF, and storage peek | Those stages reach VM interconnects and guest storage. |
| <a id="f-dfsort-iebcopy-and-maclib"></a>⛔ DFSORT, IEBCOPY, and maclib | Those stages wrap mainframe utilities and libraries. |
| <a id="f-file-status-loader-tables-and-service-commands"></a>⛔ File-status, loader tables, and service commands | STATE, HELP, QUERY, and similar programs are CMS or z/OS services. |
| <a id="f-ebcdic-transforms"></a>⛔ EBCDIC transforms | This port works in host bytes. APL encode/decode, ASA carriage, overstrike, 3270 graphics, and CMS cipher assists stay with an EBCDIC host. DIGEST (SHA and MD5) is the exception; it is implemented in software. |
| <a id="f-niche-mainframe-record-formats"></a>⛔ Niche mainframe record formats | ADDRDW, PACK, UNPACK, BLOCK, FBLOCK, and a few similar programs format CMS or OS records that this host does not use. |

---

<a id="ideas"></a>
## 💡 Ideas for future stages

This section is a wish list of extensions beyond the IBM catalog: ideas and suggestions, not commitments, not a schedule, and not a plan. They are described by what they do, and they would not be given invented IBM-looking names.

Many of these can already be approximated today with a ✍️ Python stage: a small `.py` program that reads records and writes records. If you want to try one now, that is the way. Suggestions are welcome. Open an issue, or mention an idea on a pull request.

### Data formats

- **CSV with proper quoting.** Read and write comma-separated files so that commas, quotes, and newlines inside a field stay one record.
- **JSON and JSON Lines.** Turn JSON objects into records, and write records as JSON or as one object per line.
- **XML and YAML.** Pull fields out of XML or YAML, and write records back in those formats.
- **TOML.** The same idea for TOML config files, which show up often next to JSON and YAML.
- **Parquet and Arrow.** Read and write columnar analytics files, one pipeline record per row.
- **Compressed archives.** Read and write gzip, zip, and tar so a pipeline can consume or produce compressed data without a separate COMMAND.

### Databases

- **SQLite.** Run a query against a local SQLite file, or insert records into one.
- **PostgreSQL.** The same idea against a network Postgres database.
- **ODBC.** A general way to talk to databases the host already has a driver for.
- **SQL over files.** Run a SQL query over CSV or Parquet files that sit on disk (the kind of work DuckDB does well).

### Documents

- **Excel workbooks.** Read and write `.xlsx` sheets as records (rows and columns).
- **PDF text extraction.** Pull text out of a PDF into records.
- **HTML tables.** Turn an HTML table into records, and write records as a simple HTML table.

### Network and cloud

- **HTTP and REST.** GET or POST records to an ordinary host HTTP API and write the response as records. (IBM TCP and UDP stages are ⛔ not planned. This would be a host HTTP client, not those programs.)
- **Cloud object storage.** Read and write objects in S3 or Azure Blob Storage.
- **Clipboard.** Read the host clipboard as records, or write records onto it.

### Text processing

- **Regular-expression selection.** Keep or change records using a modern regular expression, which LOCATE and CHANGE do not offer.
- **Unicode case and normalization.** Fold case and normalize Unicode beyond the ASCII tables in XLATE.
- **Whole-file distinct.** Drop duplicate records wherever they occur, not only when they sit next to each other (UNIQUE today is adjacent).
- **Templates.** Fill a template from fields in each record (the kind of work people do with mustache or format strings).
- **Directory listings.** Write host paths, sizes, and timestamps as records, so a pipeline can pick which files to read.
- **Diff of two streams.** Compare two inputs and write the added, removed, and changed records.

---

## Trademarks

CMS Pipelines and z/VM are trademarks of IBM. This project is not affiliated with or endorsed by IBM.
