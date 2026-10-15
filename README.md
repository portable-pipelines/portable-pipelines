# CMS Pipelines (portable Rust)

A cross-platform reimplementation of **CMS Pipelines** (z/VM) for Windows, macOS, and Linux.

Records flow through **stages** connected by `|`. Stages never call each other; they only **read/write via a coroutine dispatcher**, following John Hartmann’s design.

## Requirements

- [Rust](https://rustup.rs/) 1.75+ (`rustup` installs `cargo` and `rustc`)
- On Windows, the `x86_64-pc-windows-gnu` toolchain works out of the box with the MinGW linker rustup provides; MSVC is also fine if Visual Studio Build Tools are installed.

```powershell
# Windows (PowerShell) — if needed:
rustup default stable-x86_64-pc-windows-gnu
```

## Build and run

```powershell
cd cms-pipelines
cargo build --release
cargo run -p pipe-cli -- "literal hello | console"
```

The binary is named `pipe`:

```powershell
.\target\release\pipe.exe "literal hello world | console"
.\target\release\pipe.exe "literal c|literal a|literal b | sort | console"
```

## Quick examples

```text
pipe "literal hello | console"
pipe "file notes.txt | locate /TODO/ | console"
pipe "literal xx | change /xx/ /yy/ | console"
pipe "host echo hi | console"
pipe "(ENDCHAR ?) literal ax|literal by|literal az | L: locate /a/ | console ? L: | console"
```

### REXX stages

Write a `.rex` filter that uses pipeline subcommands:

```rexx
/* upper.rex — uppercase each record */
DO FOREVER
  READTO LINE
  IF RC = 12 THEN LEAVE
  WRITETO TRANSLATE(LINE)
END
```

```text
pipe "literal hello | upper.rex | console"
```

Supported REXX pipeline commands: `READTO`, `WRITETO` / `OUTPUT`, `CALLPIPE`, `ADDPIPE`, `SHORT`, `SELECT INPUT n`, `SELECT OUTPUT n`.

An embedded Regina-compatible **subset** interpreter runs these scripts without installing Regina. See [docs/architecture.md](docs/architecture.md) for plugging in real Regina later.

## Using Cursor (first time)

1. Open the `cms-pipelines` folder in Cursor.
2. Ask the Agent to run tests: `cargo test`.
3. Change a stage under `crates/pipe-stages`, re-run tests, and try `cargo run -p pipe-cli -- "…"`.

## Workspace layout

| Crate | Role |
|---|---|
| `pipe-core` | Records, streams, coroutine dispatcher, `StageCtx` |
| `pipe-parser` | PIPE specification scanner/parser |
| `pipe-stages` | Built-in stages + runner |
| `pipe-rexx` | REXX bridge + subset interpreter |
| `pipe-cli` | `pipe` binary |

## Tests

```powershell
cargo test
```

Compat golden cases live in `tests/compat/`.

## License

MIT OR Apache-2.0
