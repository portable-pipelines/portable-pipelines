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

## Windows release zip

From the repo root, after `git pull`:

```powershell
powershell -File .\scripts\package-windows.ps1
```

That builds `pipe.exe` with the pinned `stable-x86_64-pc-windows-gnu` toolchain, copies `examples\`, ships Regina (`regina.dll`, `en.mtb`, `LICENSE-Regina.txt`) when it can be found, and copies MinGW runtime DLLs only if `pipe.exe` actually imports them. Output is `dist\pipe-windows-x64-<gitsha>.zip`. `dist\` is gitignored; do not commit third-party DLLs or the zip.

Overrides: `-NoBuild`, `-ExePath`, `-ReginaDir`, `-MingwBinDir`, `-OutDir`, `-NoRegina`. Auto-discovery matches `pipe-rexx` (env vars, PATH, common install dirs) and gcc/rustup for MinGW. Details: [docs/packaging.md](docs/packaging.md).

## Quick examples

```text
pipe "literal hello | console"
pipe "< notes.txt | locate /TODO/ | console"
pipe "disk notes.txt | locate /TODO/ | console"
pipe "literal xx | change /xx/ /yy/ | > notes.out"
pipe "literal xx | change /xx/ /yy/ | console"
pipe "command echo hi | console"
pipe "examples/emit.rexx a b c d | take 2 | drop last 1 | strip | console"
pipe "disk examples/lines.txt | take 20 | nlocate /x/ | strip | console"
pipe "(ENDCHAR ?) literal ax|literal by|literal az | L: locate /a/ | console ? L: | console"
```

### TRACE (dispatcher debug)

```text
pipe --trace "literal hello | console"
pipe "(TRACE) literal hello | console"
```

TRACE lines go to stderr (`PIPE TRACE: …`) and show stage start/end, READ/WRITE (with a short preview), COMMIT, SHORT, EOF, and CALLPIPE enter/exit.

### REXX stages

Write a `.rexx` filter that uses pipeline subcommands:

```rexx
/* upper.rexx — uppercase each record (quote subcommands for Regina/IBM) */
DO FOREVER
  'READTO LINE'
  IF RC = 12 THEN LEAVE
  'OUTPUT' TRANSLATE(LINE)
END
```

```text
pipe "literal hello | examples/upper.rexx | console"
pipe "examples/emit.rexx apple banana apricot | examples/filter.rexx ap | console"
pipe "examples/emit.rexx a b c d | examples/take.rexx 2 | console"
```

More filters (prefix, fields, chop, wrap, CALLPIPE, …) live under [`examples/`](examples/README.md).

Supported REXX pipeline commands (IBM names): `READTO`, `OUTPUT`, `CALLPIPE`, `ADDPIPE`, `SHORT`, `SELECT INPUT n`, `SELECT OUTPUT n`.

**REXX engines**
1. If you have **Regina** installed (and `regina.dll` / `libregina.so` / `libregina.dylib` is discoverable), `.rexx` / `.rex` stages run through real Regina with ADDRESS `PIPELINE`.
2. Otherwise an embedded **subset** interpreter is used automatically.

Prefer the `.rexx` extension (Regina’s usual default); `.rex` is accepted too.

Discovery: set `PIPE_REGINA` or `REGINA_HOME` to the install/DLL path, or put the library on `PATH`. Debian/Ubuntu also ships `libregina.so.3` under `/usr/lib/<triplet>` (e.g. `x86_64-linux-gnu`); that multiarch dir is searched. See [docs/architecture.md](docs/architecture.md).

**Installing Regina (development)**

```bash
# Linux (Debian/Ubuntu) — shared library + regina/rexx binaries
sudo apt-get update && sudo apt-get install -y regina-rexx libregina3-dev

# macOS (Homebrew)
brew install regina-rexx
```

On Windows, install Regina or use the packaged `regina.dll` next to `pipe.exe` ([docs/packaging.md](docs/packaging.md)).

**`PIPE_REQUIRE_REGINA`** — when set to `1`, tests fail if Regina is not discovered/loaded (no silent subset fallback). Cloud agents should export it before `cargo test`. Leave it unset on machines without Regina so the suite still passes.

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

To require the real Regina bridge (fails if the library is missing):

```bash
PIPE_REQUIRE_REGINA=1 cargo test
```

Compat golden cases live in `tests/compat/`.

## License

MIT OR Apache-2.0
