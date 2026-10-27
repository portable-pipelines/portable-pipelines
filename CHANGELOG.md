# Changelog

Notable changes to Portable Pipelines. Version numbers follow the crate
workspace version in `Cargo.toml`.

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

[0.1.1]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.0...v0.1.1
