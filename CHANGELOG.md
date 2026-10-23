# Changelog

Notable changes to Portable Pipelines. Version numbers follow the crate
workspace version in `Cargo.toml`.

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

This release does not publish crates to crates.io. The workspace crates set
`publish = false` (the `pipe-cli` crate name is already taken there).
