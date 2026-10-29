# Changelog

Notable changes to Portable Pipelines. Version numbers follow the crate
workspace version in `Cargo.toml`.

## [Unreleased]

Added:

- Nine portable host stages: VARDROP, DATECONVERT, GETFILES, GREG2SEC,
  SEC2GREG, STACK, SUBCOM, SYSVAR, and TIMESTAMP. 120 of the 254
  built-in programs in the Author's Edition catalog run today. The
  public picture is [docs/STATUS.md](docs/STATUS.md).
- VARDROP drops REXX variables (and stems ending in `.`) through
  CALLPIPE when Regina is loaded.
- DATECONVERT converts the first word of each record between ISO,
  YMD/ORDERED, DMY/EUROPEAN, MDY/USA, JULIAN, and GREG. This is not
  an invented DATE stage.
- GETFILES reads host files named on the input. A missing file is an
  error, or the path goes to the secondary output when that stream is
  connected.
- GREG2SEC and SEC2GREG convert between `yyyymmddhhmmss` (and a few
  separated UTC forms) and whole seconds since 1970-01-01 00:00:00 UTC.
- TIMESTAMP prefixes each record with a UTC `YYYY-MM-DD HH:MM:SS` stamp.
- STACK is a process-wide FIFO or LIFO deque. SUBCOM COMMAND (and
  CMD/SYSTEM/SH/SHELL) runs the host shell. SYSVAR writes host
  environment values and a few computed names (OS, ARCH, PID, and
  friends).
- Streaming CALLPIPE for live callers. Nested stages join the parent
  dispatcher and borrow the caller's streams, so records move one at a
  time and memory stays bounded. `*:`, `*.input:`, `*.output:`,
  `*.input.n:`, `*.output.n:`, and PRIMARY / SECONDARY / TERTIARY
  connectors are honoured. TOLABEL, WHILELABEL, STRTOLABEL, and
  STRWHILELABEL leave their matching record for a live CALLPIPE caller
  to read again. PREFACE and APPEND use the same loan path.
- The GitHub Release workflow now builds the Windows zip on a
  GitHub-hosted Windows runner with the MSVC toolchain, writes a
  `.sha256` checksum, attaches both to the release, and records a
  GitHub artifact attestation. `scripts/package-windows.ps1` still
  defaults to GNU for local use without Visual Studio; pass
  `-Target msvc` to match the release zip.

Fixed:

- COMMAND and SUBCOM on Windows pass the command text to `cmd /C` with
  `raw_arg`, so a command that contains spaces is not wrapped as one
  extra quoted argv token. Nested quotes (PowerShell `-File`, `type`
  with a quoted path) then reach cmd the way a user would type them.
  Unix still uses `sh -c`.
- The SUBCOM echo example is `literal echo hello|` with no space before
  `|`. A space there is a trailing blank in LITERAL, and Windows `ECHO`
  prints that blank (`hello `). Unix `echo` drops it, so the old line
  only looked portable.
- On Windows, `tests::old_python_via_pipe_python_env` writes a `.cmd`
  stub that prints `3.9.18` instead of a `#!/bin/sh` script, which
  Windows cannot execute (os error 193). `query_python_version`
  accepts that stub.
- A REXX stage running under Regina now treats an upstream SHORT (TAKE
  after its count, DROP LAST, and other stages that sever the output) as
  end of file. Previously the stage failed with RC 8 "stream severed"
  before the first READTO. The subset interpreter, Python stages,
  PIPCMD, and BUFFER/SORT already kept the records they had read.

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

[0.1.2]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.0...v0.1.1
