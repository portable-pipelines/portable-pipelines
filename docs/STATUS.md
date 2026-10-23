# What Portable Pipelines can do today

Portable Pipelines is a portable implementation of IBM CMS Pipelines for Windows, macOS, and Linux. If you already use CMS Pipelines, or you are wondering whether this port can do a job you have in mind, this page is the public picture of what works today.

The numbers below match this project's coverage of the Author's Edition catalog of 254 built-in programs. The tables name every stage you can run, grouped by what it helps you do rather than by alphabet. Stage names are the IBM names, written in uppercase.

**How to read this page**

- **Works:** the stage behaves as the Author's Edition describes.
- **Works, with limits:** the stage is present, but some operands or behaviours are still missing. Each row says what is missing.
- **Planned:** work that is coming, in plain words.
- **Not planned:** mainframe-only families that this port will not implement.

Records here are bytes. On a PC or Unix host that usually means ordinary host text, not EBCDIC. File stages take a host path such as `notes.txt` or `/tmp/out`, not a CMS file name of the form `fn ft fm`.

The README shows how to build `pipe` and run a specification.

## Coverage at a glance

Of those 254 catalog entries, **99** built-in programs work here today, including ones with the limits noted below. **15** further stages that map cleanly onto a portable host are planned. **139** programs that depend on CMS, z/VM, or other mainframe facilities are not planned. The remaining catalog entry is a nested-pipeline behaviour (streaming CALLPIPE) that is planned and is described later on this page.

User-written REXX and Python stages sit beside that catalog; they are not extra IBM built-ins. REXX pipeline commands are counted separately: **3** are complete, **4** work with limits, and **19** are still to come.

## Works

These stages behave as the Author's Edition describes. A few accept only numeric stream numbers. Synonyms are noted where they help.

### Reading and writing the console, and producing records

| Stage | What it does | Notes |
|---|---|---|
| CONSOLE | Reads the terminal when it is first, or writes the terminal and copies records through when it is not. | Also TERMINAL. CMS-only fullscreen options are rejected. |
| LITERAL | Writes one argument record, then copies its input. | An omitted argument writes a null record. A cascade writes in reverse order. |
| STRLITERAL | The delimited-string form of LITERAL, with PREFACE, APPEND, or IFEMPTY. | Default is PREFACE. |
| XRANGE | Writes one record that is an inclusive range of characters. | Must be first. With no argument it writes every byte from X'00' through X'FF'. |
| RANDOM | Writes 4-byte random values until the output closes. | Must be first. Uses the host clock when you omit the seed. |

### Selecting records

| Stage | What it does | Notes |
|---|---|---|
| BETWEEN | Keeps records from one label through another, labels included. | |
| INSIDE | Keeps records between two labels, labels excluded. | |
| NOTINSIDE | The converse of INSIDE. | Also NINSIDE. |
| OUTSIDE | The converse of BETWEEN. | |
| PICK | Selects records by a string or numeric comparison of two fields or strings. | |
| FIND | Selects records that match an XEDIT FIND string. | |
| NFIND | The converse of FIND. | Also NOTFIND. |
| FRLABEL | Copies from the first prefix match through the end of the file. | Also FROMLABEL. |
| STRFIND | FIND with a delimited string, hex, or binary argument. | |
| STRNFIND | The converse of STRFIND. | |
| STRFRLABEL | FRLABEL with a delimited string. | Also STRFROMLABEL. |
| ABBREV | Selects records whose first word is an abbreviation of the argument. | |
| VERIFY | Selects records that contain only characters from a given set. | |
| ALL | Selects records that match a boolean combination of substring tests. | |
| WILDCARD | Selects records that match a `*` / `%` pattern. | |
| FRTARGET | Copies from the first record selected by another stage through the end of the file. | Also FROMTARGET. |
| TOTARGET | Copies through the first record selected by another stage. | |
| CASEI | Runs another stage with case-insensitive matching and writes the original records. | |
| NOT | Runs another stage and swaps its primary and secondary outputs. | |

### Changing and reshaping records

| Stage | What it does | Notes |
|---|---|---|
| STRIP | Removes leading and trailing characters or strings. | Default is both ends, blank. |
| SPLIT | Breaks records at a target. | |
| JOIN | Joins records back together. | |
| JOINCONT | Joins continuation lines. | |
| CHOP | Truncates records and can send the remainder to a second output. | Also TRUNCATE. |
| DUPLICATE | Writes extra copies of each record. | |
| INSERT | Inserts a string before or after a field. | |
| PAD | Pads short records on the left or right. | |
| REVERSE | Reverses the bytes of each record. | |
| RETAB | Replaces runs of blanks with tab characters. | Converse of UNTAB. |
| UNTAB | Replaces tab characters with blanks. | Converse of RETAB. |
| SNAKE | Lays records into columns, down then across. | |
| SPACE | Collapses delimiter runs the way REXX SPACE does. | |
| TOKENISE | Splits a record into tokens. | Also TOKENIZE. |
| SUBSTRING | Writes a chosen range of each record. | |
| SPILL | Splits long lines at word boundaries. | |

### Sorting, matching, and counting

| Stage | What it does | Notes |
|---|---|---|
| COUNT | Counts characters, words, lines, or the shortest and longest records. | At least one keyword is required. |
| UNIQUE | Discards or keeps adjacent duplicates. | |
| COLLATE | Collates a master stream with a detail stream. | Comparison is host byte order. |
| MERGE | Merges already-ordered streams. | Up to ten inputs. Comparison is host byte order. |
| LOOKUP | Looks up details against a reference by key. | |

### Combining, splitting, and controlling streams

| Stage | What it does | Notes |
|---|---|---|
| BUFFER | Holds records until the end of the file, or repeats null-delimited subfiles. | |
| FANINANY | Reads whichever input has a record ready. | Stream identifiers are numeric. |
| FANINTWO | Two-stream fan-in; the secondary input has priority. | |
| FANOUTWO | Copies each record to both outputs. | End-of-file on the primary output travels backwards. |
| FILLUP | Writes to the next output when the current one fills up. | |
| THREEWAY | Splits each record onto three outputs (prefix, range, rest). | Also 3WAY. |
| DEAL | Sends records to output streams by round-robin, key, or stream id. | Stream identifiers are numeric. |
| GATHER | The converse of DEAL: gathers records from input streams. | Stream identifiers are numeric. |
| HOLE | Reads every connected input and writes nothing. | |
| COPY | Consumes each record, then writes it (one-record delay). | |
| PREFACE | Runs a nested pipeline first, then copies the primary input. | |
| APPEND | Copies the primary input, then runs a nested pipeline. | |
| ELASTIC | Buffers so a pipeline is less likely to stall. | |
| GATE | Passes higher streams until a primary record arrives, then stops. | |
| IF | Runs a selection stage as an if-then or if-then-else. | |
| DAM | Waits for a primary record, then stops every stream pair. | Converse of GATE. |
| JUXTAPOSE | Holds each primary record and prefixes it to each following secondary record. | |
| SYNCHRONISE | Waits for a record on every input, then copies one from each. | Also SYNCHRONIZE. |
| PREDSELECT | Routes the original record after a test on copies. | |
| NOEOFBACK | Passes records while the output is connected; then consumes the rest. | Also NOTEOFBACK. |
| EOFBACK | Copies through, then runs a nested output stage so end-of-file can travel backwards. | |
| AGGRC | Combines numeric return codes (most negative, otherwise the largest). | |
| OVERLAY | Overlays columns from every connected input. | |
| COMBINE | Combines a run of records, bitwise or by FIRST/LAST column. | |
| PARCEL | Cuts a byte stream into records using lengths from a second input. | |
| HTTPSPLIT | Splits an HTTP stream into header records and a body. | ASCII by default. |
| UPDATE | Applies a CMS-format UPDATE file from the secondary input onto a master. | Sequence numbers are host digits. |
| DELAY | Waits until a time of day or for an interval taken from each record. | Uses the host clock. |
| BEAT | Copies records and writes a marker on a second output when input is quiet. | Uses the host clock. |

### Encoding

| Stage | What it does | Notes |
|---|---|---|
| 64ENCODE | MIME Base-64 encode. | ASCII alphabet. |
| 64DECODE | MIME Base-64 decode. | ASCII alphabet. |
| QPENCODE | MIME quoted-printable encode. | ASCII domain. |
| QPDECODE | MIME quoted-printable decode. | ASCII domain. |
| DIGEST | Computes SHA-1, SHA-256, SHA-384, SHA-512, or MD5. | Software implementation. |
| ESCAPE | Prefixes listed characters with an escape character. | |
| URLDEBLOCK | Splits form-urlencoded data into fields. | ASCII by default. |

## Works, with limits

These stages are present and useful. Each Notes cell is the short form of what is still missing.

### Reading and writing files

File stages take a **host path**, not a CMS `fn ft fm`.

| Stage | What it does | Limits |
|---|---|---|
| DISK | First stage reads a file (a missing file is empty). Later it appends or creates the file and copies records through. | Host path only. CMS minidisk and SFS forms are not planned. |
| `<` | Must be first. Reads a host file. | Host path only. A missing file is an error. |
| `>` | Must not be first. Replaces or creates a host file and copies records through. | Host path only. |
| `>>` | Must not be first. Appends to or creates a host file and copies records through. | Host path only. |

### Selecting records

| Stage | What it does | Limits |
|---|---|---|
| LOCATE | Keeps records that contain a string (or any character, with ANYOF). | MIXED, ONES, ZEROS, and column ranges are not supported yet. |
| NLOCATE | The converse of LOCATE. | Same limits as LOCATE. Also NOTLOCATE. |
| TAKE | Keeps the first or last *n* records. | Byte counts are not supported yet. |
| DROP | Discards the first or last *n* records. | Byte counts are not supported yet. DROP is not another name for HOLE. |
| TOLABEL | Copies through the record before the first prefix match. | The matching record is not left in the pipeline for a nested pipeline to read again. |
| WHILELABEL | Copies while records begin with the string. | Same leftover-record limit as TOLABEL. |
| STRTOLABEL | TOLABEL with a delimited string. | Same leftover-record limit as TOLABEL. |
| STRWHILELABEL | WHILELABEL with a delimited string. | Same leftover-record limit as TOLABEL. |
| ZONE | Runs another stage on a column subset and writes the original record. | Buffers until the end of the file instead of streaming one record at a time. Some end-of-file behaviour of the nested stage is not matched yet. |

### Changing and reshaping records

| Stage | What it does | Limits |
|---|---|---|
| CHANGE | Replaces text in each record. | Column ranges are not supported yet. |
| SPEC | Builds output records from fields, words, literals, and placements. | The expression language (counters, IF/THEN, SELECT, SET, PRINT, PICTURE) is not supported yet. Neither are TODCLOCK, packed/float/date conversions, or writing to extra streams. Portable conversions C2X, X2C, C2B, B2C, X2B, and B2X work. Also SPECS. |

### Sorting and counting

| Stage | What it does | Limits |
|---|---|---|
| SORT | Sorts records. Default is ascending, whole record. | Field ranges and PAD / NOPAD are not supported yet. UNIQUE and COUNT work. |

### Combining and splitting streams

| Stage | What it does | Limits |
|---|---|---|
| FANOUT | Copies each record to every connected output. | A list of named stream identifiers is not supported yet. STOP ALLEOF, ANYEOF, and a count work. |
| FANIN | Reads the primary input to the end, then the next input, and so on. | Named stream identifiers are not supported. Numeric stream numbers work. |

### Encoding

| Stage | What it does | Limits |
|---|---|---|
| XLATE | Translates characters, including upper/lower case and from-to pairs. | Also TRANSLATE. Code-page numbers, A2E / E2A, and live CMS input/output tables are not supported yet. Case tables are ASCII. |
| DEBLOCK | Splits a byte stream into records at line ends. | Only LINEND and CRLF. Other IBM record formats are not supported. |

### Running commands and nested pipelines

| Stage | What it does | Limits |
|---|---|---|
| COMMAND | Runs a host process and writes its output as records. | Also CMD. This is a portable process runner, not a CMS command. There is no secondary stream for the return code. |
| PIPCMD | Reads command records and issues them into the running pipeline. | ADDSTREAM, EOFREPORT, ISSUEMSG, the REXX command, and SETRC are not implemented yet. |
| PIPESTOP | When a record arrives, wakes stages that are waiting on a timer, then copies the record through. | The CMS-only stop signal is not implemented. DELAY and BEAT are the stages that wake. |
| RUNPIPE | Runs each input record as a new pipeline and intercepts messages. | Event traces, pause-on-message, and CMS message-level bits are not implemented yet. |

## Writing your own stages

A user-written stage is a program you write that reads records from its input and writes records to its output. You use it in a pipeline by naming the file. REXX and Python are the two languages supported today. They fill the same role: a filter you own, sitting in the specification the same way a built-in stage does.

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

**REXX pipeline commands today**

Complete:

- READTO reads or discards one input record into a variable.
- OUTPUT writes a line.
- SHORT connects or severs the selected input and output.

Present, with limits:

- CALLPIPE runs a subroutine pipeline. It currently reads all of its input, runs to completion, then writes all of its output.
- ADDPIPE adds a specification to the running set. It currently behaves like CALLPIPE.
- SELECT chooses a stream. Only `SELECT INPUT n` and `SELECT OUTPUT n` work.
- COMMIT commits to a new level. It is available when Regina is in use.

Still to come: PEEKTO, ADDSTREAM, BEGOUTPUT, EOFREPORT, GETRANGE, ISSUEMSG, MAXSTREAM, MESSAGE, NOCOMMIT, RESOLVE, the REXX command from a filter, SCANRANGE, SCANSTRING, SETRC, SEVER, STAGENUM, STREAMNUM, STREAMSTATE, and SUSPEND.

Sharing REXX or CLIST variables with the pipeline (STEM, VAR, and the rest of that family) is planned as built-in stages, not as extra REXX commands. See Planned below.

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

Python stages are single-stream for now: one input and one output. Selecting extra streams, running a nested pipeline from Python, and related calls are still to come.

## Planned

These items are coming. Nothing here is available to run today.

**Streaming CALLPIPE.** Nested pipelines currently read all of their input, run to completion, then write all of their output. A streaming form, where records can flow through a nested pipeline one at a time, is planned. Until it arrives, TOLABEL, WHILELABEL, STRTOLABEL, and STRWHILELABEL cannot leave their matching record for the caller to read again.

**Portable device and host extras.** These IBM stages have a clear mapping onto Windows, macOS, or Linux, and are planned as a thin host layer:

- STEM, VAR, VARSET, VARFETCH, REXXVARS, VARLOAD, and VARDROP, for reading and writing REXX or CLIST variables.
- DATECONVERT, if a useful portable form can be defined (there will not be an invented DATE stage).
- GETFILES, to read host files whose names arrive on the input.
- GREG2SEC and SEC2GREG, to convert between a Gregorian timestamp and seconds since 1970.
- STACK, a FIFO or LIFO program stack.
- SUBCOM, to issue commands to a named subcommand environment.
- SYSVAR, to write host or system variables into the pipeline.
- TIMESTAMP, to prefix a date-and-time stamp to each record.

**Multi-stream Python stages.** Python filters today have one input and one output. Selecting extra streams and running a nested pipeline from Python are planned.

**Remaining REXX pipeline commands.** The commands listed as still to come in the REXX section above.

Deeper SPEC work (the expression language, extra conversions, and extra output streams) is part of finishing SPEC, not a new stage. It is noted under Works, with limits.

## Not planned

These families need CMS, z/VM, MVS, a 3270, a tape drive, or another mainframe facility. This port will not implement them, and it will not invent a look-alike under the same name. File stages you *can* use (`DISK`, `<`, `>`, `>>`) take host paths instead of CMS `fn ft fm`.

| Family | Why it stays out |
|---|---|
| CMS minidisk and SFS | Those drivers address CMS disks and the Shared File System. This port reads and writes host paths. |
| MVS datasets and PDS | Those drivers address z/OS datasets, PDS members, and SYSOUT. |
| OpenExtensions / HFS | Those drivers address z/OS UNIX files and descriptors. |
| 3270 and fullscreen | Those programs drive a 3270 screen. There is no fullscreen terminal here. |
| Tape and unit record | Tape, punch, reader, and printer devices are not present on this host. |
| Track and CKD | Those programs speak mainframe disk geometry and related record formats. |
| CP, CMS, and TSO command stages | Full CP, CMS, and TSO command fidelity is a mainframe host, not this process runner. |
| CP services | Messages, device info, and related CP queries need z/VM. |
| TCP / UDP and the resolver | Network and hostname stages are not in this port. |
| SQL / DB2 | Those stages talk to a mainframe database. |
| ISPF, XEDIT, and HLASM | Those stages need ISPF, XEDIT, or the assembler. |
| IUCV, VMCF, and storage peek | Those stages reach VM interconnects and guest storage. |
| DFSORT, IEBCOPY, and maclib | Those stages wrap mainframe utilities and libraries. |
| File-status, loader tables, and service commands | STATE, HELP, QUERY, and similar programs are CMS or z/OS services. |
| EBCDIC transforms | This port works in host bytes. APL encode/decode, ASA carriage, overstrike, 3270 graphics, and CMS cipher assists stay with an EBCDIC host. DIGEST (SHA and MD5) is the exception; it is implemented in software. |
| Niche mainframe record formats | ADDRDW, PACK, UNPACK, BLOCK, FBLOCK, and a few similar programs format CMS or OS records that this host does not use. |

## Trademarks

CMS Pipelines and z/VM are trademarks of IBM. This project is not affiliated with or endorsed by IBM.
