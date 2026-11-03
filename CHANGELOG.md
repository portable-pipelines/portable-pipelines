# Changelog

Notable changes to Portable Pipelines. Version numbers follow the crate
workspace version in `Cargo.toml`.

## [Unreleased]

Added:

- FILEBACK (also DISKBACK) reads a host file last record first. It must be
  first. A missing file is an error, the same way `<` reports one.
- FILERAND (also DISKRAND) reads selected records or ranges from a host
  file (`n`, `n-m`, `n-*`, `*-m`, `*-*`, `n.count`, plus RECNO). BLOCKED
  is rejected. The file is indexed in memory.
- FILEFAST (also DISKFAST) is the same host-path stand-in as DISK.
- FILESLOW (also DISKSLOW) is FILEFAST with optional FROM recno.
- FILEUPDATE (also DISKUPDATE) replaces records whose numbers sit in
  columns 1-10 of each input record. A different-length replacement
  keeps later records.
- CRC writes a 16-bit CRC16I checksum of the concatenated input as two
  big-endian bytes (polynomial 0x8005, initial 0x8000). APPEND and a
  secondary output work as documented. This is a subset: the result has
  not been checked against IBM CRC16I.
- INSTORE holds records until end of file and writes one opaque token.
  OUTSTORE reads that token and writes the stored records (reversed
  after INSTORE REVERSE), then releases the buffer. They work across
  CALLPIPE in the same process. Example
  [examples/reverse_records.txt](examples/reverse_records.txt).

Fixed:

- A REXX stage that ends without reading the rest of its input now
  severs those connections. An upstream writer gets RC 12 and the
  pipeline finishes instead of stalling. ADDPIPE insert-between and the
  subset interpreter follow the same rule (issue #97).

## [0.1.5] - 2026-11-02

Live CALLPIPE from a Regina helper (including STEM and VAR), named
stream identifiers, a real ADDPIPE, and GitHub Actions CI on Linux,
macOS, and Windows.

Added:

- CALLPIPE issued from a Regina helper loans the stage's live streams
  in the parent (the same path as subset REXX and PIPCMD). An infinite
  producer plus downstream TAKE N stops early. SELECT, COMMIT, SEVER,
  ADDSTREAM, and STREAMSTATE go through the helper protocol, and I/O
  honours stream 1+. Example `examples/callpipe_stream.rexx` (PR #91).
- Reverse SHV (`0x1B` request, `0x85` reply) so STEM, VAR, VARSET,
  VARFETCH, REXXVARS, VARLOAD, and VARDROP in a parent-loaned CALLPIPE
  reach the helper's Regina pool, including while CALLPIPE, SELECT,
  COMMIT, or SEVER is in flight. Example `examples/callpipe_stem.rexx`
  (PR #93).
- Named stream identifiers. PRIMARY, SECONDARY, and TERTIARY are 0, 1,
  and 2. User names are case-insensitive and live on one stage. FANOUT
  and FANIN take identifier lists (`FANOUT LEFT RIGHT`). DEAL, GATHER,
  SELECT, ADDSTREAM, and STREAMNUM accept a name as well as a number.
  Design notes:
  [docs/design/named-stream-identifiers.md](docs/design/named-stream-identifiers.md)
  (PR #94).
- ADDPIPE is a real add to the running set. Nested stages join the
  caller's commit group and return at once (no wait, no restore).
  Input and output connectors insert the nested pipeline on the path
  so the caller keeps reading and writing. PIPCMD, subset REXX, and
  the Regina helper share that path (helper protocol `0x1C`).
  Connectors accept a caller stream name (`*.input.FOO:`). Example
  `examples/addpipe_demo.rexx` (PR #95).
- GitHub Actions CI (`.github/workflows/ci.yml`) runs the workspace
  test suite on `ubuntu-latest`, `macos-latest` (Apple silicon), and
  `windows-latest` for every pull request and every push to master.
  Linux and macOS require Regina and Python 3.10+. Windows uses the
  MSVC toolchain and the official Regina zip already used by the
  release job. The workflow does not sign binaries (PR #92).

Fixed:

- The dispatcher sleeps at most 1ms while only DELAY/BEAT waiters
  remain, then re-checks the nearest deadline. A full
  `thread::sleep(remaining)` on macOS can overshoot a 15ms wait by
  100ms, so BEAT missed 10ms intervals (PR #92).
- When one host sleep expires several DELAY/BEAT waiters, the
  dispatcher resumes the earliest deadline first (PR #92).
- Fixture specs that write `/tmp/...` are rewritten at run time to a
  harness temp directory, so harvested README lines stay `/tmp` and
  still run on Windows (PR #92).
- A CALLPIPE that names STEM or VAR no longer stays in the helper
  with buffered I/O. The TOLABEL leftover on that path is restored
  for the caller (PR #93).
- VARSET now shorts its input when the output severs, so a live
  CALLPIPE plus TAKE can stop an infinite producer the same way
  XLATE does (PR #93).

Limits that stay:

- `PIPE_REGINA_INPROC=1` still drains the whole stage, then runs, then
  flushes, including CALLPIPE and ADDPIPE.
- `SUSPEND` cannot yield the parent while Regina is in bytecode.
- Python stages stay single-stream and have no ADDPIPE.
- FANINANY has no identifier list.
- PRIMARY, SECONDARY, and TERTIARY cannot be renamed.
- A nested SELECT FOO does not see the caller's FOO. Names stay on the
  stage that defined them.

## [0.1.4] - 2026-11-01

A signed Windows `pipe.exe`, live Regina stages through a helper process,
and a one-line streaming CALLPIPE demo over host files.

Added:

- The Windows `pipe.exe` in the GitHub Release zip is Authenticode-signed
  by John Lynn via Microsoft Artifact Signing. A signed `pipe.exe` usually
  means fewer antivirus and Defender scan delays. The bundled `regina.dll`
  is third-party Regina and is not signed. Windows SmartScreen or Defender
  may still do a one-time cloud check the first time you run the program.
  The release workflow signs after the MSVC build and before the zip, using
  GitHub OIDC (no stored secrets). `workflow_dispatch` builds and signs
  without publishing a GitHub Release. Pull requests skip signing and stay
  a packaging dry run (PR #89).
- Real-Regina `.rexx` stages run in a helper process (the same `pipe`
  binary relaunched with a hidden flag). READTO, PEEKTO, OUTPUT, and
  SHORT go live through the dispatcher, so an infinite producer plus
  downstream TAKE N completes, and two Regina stages can sit in one
  pipeline. Framing is shared with Python stages; every I/O message
  carries a stream number (only stream 0 works today). CALLPIPE issued
  from the helper still buffers remaining input. `PIPE_REGINA_INPROC=1`
  keeps the old in-process drain path. Design notes:
  [docs/design/regina-out-of-process.md](docs/design/regina-out-of-process.md)
  (PR #88).
- Example, generator, and tests for streaming CALLPIPE over a set of host
  files. The headline is one `pipe` line: SPEC builds `callpipe < path |
  xlate | *:` from a path list and PIPCMD runs it (streams on every host,
  including a Windows zip with bundled Regina). A REXX stage is also
  here. Demos cover many small files, a few large files (watch RSS), and
  `take 10` stopping early (PR #87).
- XLATE and SPEC short their input when a downstream TAKE (or other
  SHORT) closes the output, so a nested `< file | xlate | *:` CALLPIPE
  and a SPEC-to-PIPCMD path list can finish instead of stalling
  (PR #87).
- A Regina stage that has already buffered its output stops flushing
  when the downstream stream is severed, instead of failing the
  pipeline with "stream severed". That is the in-process drain path
  (`PIPE_REGINA_INPROC=1`). The helper maps a severed write to OUTPUT
  RC 12 and does not fail the pipeline (PR #87).

Fixed:

- Windows GNU and MSVC compile of the Regina helper: `CreateFileW` needs
  the `Win32_Security` feature (its signature uses `SECURITY_ATTRIBUTES`),
  and `GENERIC_READ` is imported from `Win32::Foundation` in windows-sys
  0.59 (PR #88).

Limits that stay:

- CALLPIPE issued from a Regina helper still buffers remaining input.
- Only stream 0 is live on the Regina helper path.
- ADDPIPE is still a CALLPIPE alias, not a true add to the running set.
- Named stream identifiers for FANOUT, FANIN, DEAL, GATHER, SELECT, and
  ADDSTREAM are still planned.
- The bundled `regina.dll` in the Windows zip is third-party and unsigned.
  Windows SmartScreen or Defender may still do a one-time cloud check
  the first time you run `pipe.exe`.

## [0.1.3] - 2026-10-29

Streaming CALLPIPE for live callers, nine portable host stages, a
Regina drain fix so TAKE and DROP LAST work with REXX, and a
GitHub-built Windows zip with checksums and a build attestation.

Added:

- Nine portable host stages: VARDROP, DATECONVERT, GETFILES, GREG2SEC,
  SEC2GREG, STACK, SUBCOM, SYSVAR, and TIMESTAMP. 120 of the 254
  built-in programs in the Author's Edition catalog run today. The
  public picture is [docs/STATUS.md](docs/STATUS.md) (PR #83).
- VARDROP drops REXX variables (and stems ending in `.`) through
  CALLPIPE when Regina is loaded (PR #83).
- DATECONVERT converts the first word of each record between ISO,
  YMD/ORDERED, DMY/EUROPEAN, MDY/USA, JULIAN, and GREG. This is not
  an invented DATE stage (PR #83).
- GETFILES reads host files named on the input. A missing file is an
  error, or the path goes to the secondary output when that stream is
  connected (PR #83).
- GREG2SEC and SEC2GREG convert between `yyyymmddhhmmss` (and a few
  separated UTC forms) and whole seconds since 1970-01-01 00:00:00 UTC
  (PR #83).
- TIMESTAMP prefixes each record with a UTC `YYYY-MM-DD HH:MM:SS` stamp
  (PR #83).
- STACK is a process-wide FIFO or LIFO deque. SUBCOM COMMAND (and
  CMD/SYSTEM/SH/SHELL) runs the host shell. SYSVAR writes host
  environment values and a few computed names (OS, ARCH, PID, and
  friends) (PR #83).
- Streaming CALLPIPE for live callers. Nested stages join the parent
  dispatcher and borrow the caller's streams, so records move one at a
  time and memory stays bounded. `*:`, `*.input:`, `*.output:`,
  `*.input.n:`, `*.output.n:`, and PRIMARY / SECONDARY / TERTIARY
  connectors are honoured. TOLABEL, WHILELABEL, STRTOLABEL, and
  STRWHILELABEL leave their matching record for a live CALLPIPE caller
  to read again. PREFACE and APPEND use the same loan path (PR #82).
- The GitHub Release workflow now builds the Windows zip on a
  GitHub-hosted Windows runner with the MSVC toolchain, writes a
  `.sha256` checksum, attaches both to the release, and records a
  GitHub artifact attestation. Unix tarballs get the same attestation.
  `scripts/package-windows.ps1` still defaults to GNU for local use
  without Visual Studio; pass `-Target msvc` to match the release zip
  (PR #84).

Fixed:

- COMMAND and SUBCOM on Windows pass the command text to `cmd /C` with
  `raw_arg`, so a command that contains spaces is not wrapped as one
  extra quoted argv token. Nested quotes (PowerShell `-File`, `type`
  with a quoted path) then reach cmd the way a user would type them.
  Unix still uses `sh -c` (PR #83).
- The SUBCOM echo example is `literal echo hello|` with no space before
  `|`. A space there is a trailing blank in LITERAL, and Windows `ECHO`
  prints that blank (`hello `). Unix `echo` drops it, so the old line
  only looked portable (PR #83).
- On Windows, `tests::old_python_via_pipe_python_env` writes a `.cmd`
  stub that prints `3.9.18` instead of a `#!/bin/sh` script, which
  Windows cannot execute (os error 193). `query_python_version`
  accepts that stub (PR #83).
- A REXX stage running under Regina now treats an upstream SHORT (TAKE
  after its count, DROP LAST, and other stages that sever the output) as
  end of file. Previously the stage failed with RC 8 "stream severed"
  before the first READTO. The subset interpreter, Python stages,
  PIPCMD, and BUFFER/SORT already kept the records they had read
  (PR #81).

Limits that stay:

- DATECONVERT, GREG2SEC, SEC2GREG, and TIMESTAMP use UTC civil time.
  STACK is in-process, not the CMS console stack. SUBCOM does not
  implement CMS or TSO environments. SYSVAR does not implement CMS
  system variables.
- A Regina stage still drains its input, runs, then writes buffered
  output. Nested CALLPIPE from that host is not a live stream.
- ADDPIPE is still a CALLPIPE alias, not a true add to the running set.
- Named stream identifiers for FANOUT, FANIN, DEAL, GATHER, SELECT, and
  ADDSTREAM are still planned.
- The Windows `pipe.exe` in the GitHub Release zip is not
  Authenticode-signed yet.

## [0.1.2] - 2026-10-28

Old CMS script compatibility, the rest of the REXX pipeline commands
from Author's Edition Chapter 21, and tests that found two bugs.

Added:

- Labels may sit against the stage verb (`o:fanout`, `v:verify`,
  `short: locate 1`) and a label may stand alone as a stage (`| j: |`,
  `| i:|`, `? v: |`, `?short:`). Windows drives, URLs, and `x::y` stay
  one token (PR #77).
- DEBLOCK FIXED *n*, including a bare number (`deblock 2`) and bare
  DEBLOCK (FIXED 80). Input records are concatenated as a byte stream
  and then split every *n* characters; a leftover shorter group is
  written last (PR #77).
- SPEC field identifiers (`a:1`), IF/THEN/ELSE/ENDIF, SET (`:=` / `+=`),
  PRINT, PICTURE (a run of `9` digits), NOWRITE, counters `#0` to `#9`,
  and the operators `==`, `^=` / `/=` / `\=`, `//`, `+`, `-`, `*`, `/`
  (PR #77).
- ADDRESS PIPELINE commands that were missing: PEEKTO, ADDSTREAM,
  BEGOUTPUT, EOFREPORT, GETRANGE, ISSUEMSG, MAXSTREAM, MESSAGE,
  NOCOMMIT, RESOLVE, REXX (the command), SCANRANGE, SCANSTRING, SETRC,
  SEVER, STAGENUM, STREAMNUM, STREAMSTATE, and SUSPEND. Shared verbs
  (SEVER, RESOLVE, STAGENUM, STREAMNUM, STREAMSTATE, MAXSTREAM,
  SUSPEND, MESSAGE, and now ADDSTREAM, EOFREPORT, ISSUEMSG, SETRC, and
  REXX) use the same implementation as PIPCMD. REXX pipeline commands
  are now 22 complete and 4 limited (PR #78).
- PEEKTO looks at the next input record without consuming it. CALLPIPE
  `*:` then sees that unread record (PR #78).
- SELECT also accepts BOTH, a bare number, ANYINPUT, and the IBM
  abbreviations. COMMIT runs in the subset interpreter as well as
  Regina (PR #78).
- Example `examples/luhn.rexx`: an original Luhn mod-10 check that uses
  glued labels, DEBLOCK n, SPEC conditionals, and PEEKTO plus CALLPIPE
  `*:` (PRs #77 and #78).
- Dedicated tests per REXX pipeline command, plus matching depth for
  glued labels, DEBLOCK FIXED, SPEC conditionals, CMS not-equal rewrite,
  and the stall message. Named tests, Regina helpers, and `.pipefix`
  cases (PR #79).

Fixed:

- Subset `SETRC 7` then `EXIT 0` now keeps stage RC 7, matching the
  Regina runner (nonzero EXIT still wins) (PR #79).
- `SEVER OUTPUT` waits for an already written record to be consumed
  before severing, so that record is not dropped. IBM SHORT still
  drops the in-flight slot (PR #79).
- REXX stages rewrite CMS `/=` and `^=` to Regina `\=` outside quoted
  strings and `/* */` comments, so old CMS filters run unmodified
  (PR #77).
- A stalled pipeline now prints blocked stages in plain words and a
  short hint. The old debug dump is emitted only with `--trace`. Return
  code 8 is unchanged (PR #77).
- The Windows zip from `scripts/package-windows.ps1` drops `__pycache__`
  and `*.pyc`. The Unix release tarball already did (PR #77).

Limits that stay:

- CALLPIPE and ADDPIPE are still buffered (not streaming). After
  PEEKTO, `*:` feeds every remaining unread record in one nested run,
  so a Luhn check of several cards in one stream would still need one
  LITERAL injection per card until streaming CALLPIPE exists.
- BEGOUTPUT redirects SAY only in the subset interpreter. Regina SAY
  still goes to the terminal; use OUTPUT.
- NOCOMMIT is recorded but cannot undo the dispatcher auto-commit to
  level 0 before the stage body.
- ISSUEMSG has no IBM FPL message repository; it writes the number,
  optional component, and tokens as one pipeline message.
- SUSPEND is a no-op in the buffered Regina host (input was already
  drained).
- SPEC SELECT / ELSEIF / TODCLOCK, extra SPEC streams, full COBOL
  pictures, and named FANIN / FANOUT stream lists are still missing.

Not in this release: streaming CALLPIPE.

## [0.1.1] - 2026-10-27

REXX variable pool stages, so a REXX program running as a pipeline stage
can share variables with a nested pipeline through CALLPIPE.

Added:

- Six REXX variable pool stages: STEM, VAR, VARSET, VARFETCH, REXXVARS,
  and VARLOAD (PR #75). STEM and VAR act as a source when first in a
  pipeline and as a sink otherwise; VARSET and VARLOAD are filters;
  VARFETCH and REXXVARS must be first. They work in a REXX program
  running as a pipeline stage that uses CALLPIPE, and they need Regina.
  111 of the 254 built-in programs in the Author's Edition catalog run
  today. The public picture is [docs/STATUS.md](docs/STATUS.md).
- Examples `examples/photo_workflow.rexx` and `examples/varpool_demo.rexx`,
  and the design note [docs/design/rexx-variable-pool.md](docs/design/rexx-variable-pool.md).

Fixed:

- CALLPIPE from REXX now supports multi-stream pipelines with labels and
  `?`. It used to drop everything after the first `?`, so LOOKUP did not
  work from CALLPIPE.
- STEM as a sink no longer loses buffered records when its input stream
  is severed early (for example by LOOKUP).

Known limits: CALLPIPE is still buffered (not streaming). VARDROP is not
implemented yet.

## 0.1.0

First public release. A portable `pipe` command for Windows, macOS, and Linux,
measured against CMS/TSO Pipelines Author's Edition 1.1.11 (SL26-0018-05).

What ships:

- 105 of the 254 built-in programs in the Author's Edition catalog run today,
  including some with documented limits. The public picture is [docs/STATUS.md](docs/STATUS.md).
- 15 further portable host stages are planned. 133 programs that need CMS, z/VM,
  or other mainframe facilities are not planned.
- User-written REXX stages (Regina when installed, otherwise an embedded subset
  interpreter) and user-written Python 3.10+ stages.
- A Windows zip built on a maintainer laptop (`scripts/package-windows.ps1`).
- Linux x86_64 and macOS (arm64 and x86_64) tarballs from the GitHub Actions
  release workflow. Those tarballs do not bundle Regina or Python.
- Version option (`pipe --version`, `-v`, or `-V`) prints the version, e.g. `pipe 0.1.0`.
- New JUXTAPOSE example showing how to add the owning USER to each MDISK line. The pipeline uses STRFIND, SPEC, and JUXTAPOSE to pair USER lines with their following MDISK lines. See `examples/users.txt` and the guided tour scripts.

This release does not publish crates to crates.io. The workspace crates set
`publish = false` (the `pipe-cli` crate name is already taken there).

[0.1.5]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.0...v0.1.1
