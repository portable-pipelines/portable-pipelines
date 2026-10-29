# Portable Pipelines

Portable Pipelines is a portable implementation of IBM's **CMS Pipelines** (z/VM) for Windows, macOS, and Linux.

Records flow through **stages** connected by `|`. Stages never call each other; they only **read/write via a coroutine dispatcher**, following John Hartmann’s design.

## Status

Of the 254 built-in programs in the IBM CMS Pipelines catalog, **120** work in this port today, including some with documented limits. **133** programs that need CMS, z/VM, or other mainframe facilities are not planned. REXX pipeline commands (inside a REXX stage, counted separately) are **22** complete and **4** limited.

[What Portable Pipelines can do today](docs/STATUS.md) lists every stage you can run, what is still coming, and what this port will not implement.

## Requirements

- [Rust](https://rustup.rs/) 1.75+ (`rustup` installs `cargo` and `rustc`)
- On Windows without Visual Studio Build Tools, use the GNU host toolchain. The MSVC host needs `link.exe`. The release zip always uses GNU (`x86_64-pc-windows-gnu`; see [docs/packaging.md](docs/packaging.md)).

```powershell
# Windows (PowerShell), if you do not have Visual Studio Build Tools:
rustup toolchain install stable-x86_64-pc-windows-gnu
# Optional: make GNU the default host so plain `cargo` uses it:
# rustup set default-host x86_64-pc-windows-gnu
```

## Build and run

```powershell
# Windows (PowerShell), from your clone
# Without Visual Studio, prefix cargo with the GNU toolchain:
#   cargo +stable-x86_64-pc-windows-gnu build --release
# Or set it once: rustup set default-host x86_64-pc-windows-gnu
cargo build --release
cargo run -p pipe-cli -- "literal hello| console"
.\target\release\pipe.exe "literal hello world| console"
.\target\release\pipe.exe "literal c|literal a|literal b | sort | console"
```

```bash
# Linux and macOS, from your clone
cargo build --release
cargo run -p pipe-cli -- "literal hello| console"
./target/release/pipe "literal hello world| console"
./target/release/pipe "literal c|literal a|literal b | sort | console"
```

The binary is named `pipe`. A space before `|` becomes part of the record (a trailing blank), so first-run examples write `literal hello| console` with no space before `|`.

## Windows release zip

From the repo root, after `git pull`:

```powershell
powershell -File .\scripts\package-windows.ps1
```

That builds `pipe.exe` with the GNU host toolchain (`cargo +stable-x86_64-pc-windows-gnu build --release --target x86_64-pc-windows-gnu -p pipe-cli`), so you do not need Visual Studio. The binary is `target\x86_64-pc-windows-gnu\release\pipe.exe`. It copies `examples\` and the `pipelines\` helper package, copies `LICENSE-MIT` and `LICENSE-APACHE`, ships Regina (`regina.dll`, `en.mtb`, `LICENSE-Regina.txt`) when it can be found, and copies MinGW runtime DLLs only if `pipe.exe` actually imports them. Output is `dist\pipe-windows-x64-<gitsha>.zip`. For a release asset, pass `-Version v0.1.2` so the zip is `dist\pipe-windows-x64-v0.1.2.zip` and upload that name. `dist\` is gitignored; do not commit third-party DLLs or the zip.

Overrides: `-NoBuild`, `-ExePath`, `-ReginaDir`, `-MingwBinDir`, `-OutDir`, `-NoRegina`, `-Version`. Auto-discovery matches `pipe-rexx` (env vars, PATH, common install dirs) and gcc/rustup for MinGW.

Linux and macOS tarballs are built by GitHub Actions when a Release is published. They do not include Regina or Python; install those with apt or brew. Details: [docs/packaging.md](docs/packaging.md).

## Quick examples

```text
pipe --version
pipe "literal hello| console"
pipe "< notes.txt | locate /TODO/ | console"
pipe "disk notes.txt | locate /TODO/ | console"
pipe "literal xx | change /xx/ /yy/ | > notes.out"
pipe "literal xx | change /xx/ /yy/ | console"
pipe "command echo hi | console"
pipe "examples/emit.rexx a b c d | take 2 | drop last 1 | strip | console"
pipe "examples/emit.py a b c | examples/upper.py | console"
pipe "disk examples/lines.txt | take 20 | nlocate /x/ | strip | console"
pipe "(ENDCHAR ?) literal ax|literal by|literal az | L: locate /a/ | console ? L: | console"
```

### TRACE (dispatcher debug)

```text
pipe --trace "literal hello| console"
pipe "(TRACE) literal hello| console"
```

TRACE lines go to stderr (`PIPE TRACE: …`) and show stage start/end, READ/WRITE (with a short preview), COMMIT, SHORT, EOF, and CALLPIPE enter/exit.

### Error context

When a stage fails (scan, commit-time stream checks, or runtime), stderr keeps the existing `PIPE: …` primary line and adds the CMS default-MSGLEVEL context lines (Author’s Edition 1.1.11 messages 1 and 3; message 4 when `NAME` is set; message 2 for `CALLPIPE` / `ADDPIPE`):

```text
PIPE: PICK: secondary input stream must not be connected
... Issued from stage 3 of pipeline 1
... Running "PICK 1.1 == /B/"
```

Stages and pipelines are numbered from 1. Label-only references (`a:`) occupy a slot in that pipeline but do not create a new stage; a later `b:` that connects the secondary input of `b: PICK` still reports the **definition** (`b: PICK` as stage 3 of pipeline 1). Spec-wide syntax errors (unclosed `(…)` options, empty specification) have no stage and stay a single line. This port does not print FPL message numbers.

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

More filters (prefix, fields, chop, wrap, CALLPIPE, …) live under [`examples/`](examples/README.md). The harvested developer catalog is [`docs/examples-reference.md`](docs/examples-reference.md).

Supported REXX pipeline commands (IBM names): `READTO`, `PEEKTO`, `OUTPUT`, `CALLPIPE`, `ADDPIPE`, `SHORT`, `SELECT`, `COMMIT`, `ADDSTREAM`, `BEGOUTPUT`, `EOFREPORT`, `GETRANGE`, `ISSUEMSG`, `MAXSTREAM`, `MESSAGE`, `NOCOMMIT`, `RESOLVE`, `REXX`, `SCANRANGE`, `SCANSTRING`, `SETRC`, `SEVER`, `STAGENUM`, `STREAMNUM`, `STREAMSTATE`, `SUSPEND`. Live CALLPIPE streams (loans); a Regina stage still drains first, and ADDPIPE is still a CALLPIPE alias. See [docs/STATUS.md](docs/STATUS.md).

**REXX engines**
1. If you have **Regina** installed (and `regina.dll` / `libregina.so` / `libregina.dylib` is discoverable), `.rexx` / `.rex` stages run through real Regina with ADDRESS `PIPELINE`.
2. Otherwise an embedded **subset** interpreter is used automatically.

Prefer the `.rexx` extension (Regina’s usual default); `.rex` is accepted too.

Discovery: set `PIPE_REGINA` or `REGINA_HOME` to the install/DLL path, or put the library on `PATH`. Debian/Ubuntu also ships `libregina.so.3` under `/usr/lib/<triplet>` (e.g. `x86_64-linux-gnu`); that multiarch dir is searched.

**Installing Regina (development)**

```bash
# Linux (Debian/Ubuntu) — shared library + regina/rexx binaries
sudo apt-get update && sudo apt-get install -y regina-rexx libregina3-dev

# macOS (Homebrew)
brew install regina-rexx
```

On Windows, install Regina or use the packaged `regina.dll` next to `pipe.exe` ([docs/packaging.md](docs/packaging.md)).

### Python stages

Write a `.py` program that reads records from its input and writes records to its output. Use it by naming the file in the pipeline, just as you do with a REXX stage. Each Python stage runs as its own Python 3.10 or later process, and REXX and Python stages can sit side by side in one pipeline.

`pipe` makes the `pipelines` helper module available to each stage for you, so you don't set `PYTHONPATH` yourself. It looks in these places in order: `PIPE_PYTHONPATH` if set, then a `pipelines` folder next to the executable (Windows zip), then `crates/pipe-python/python` from this source tree via a compile-time path when that directory still exists on disk. If none is found, the stage fails and lists where it looked.

```text
pipe "literal hello | examples/upper.py | console"
pipe "examples/emit.py apple banana | examples/upper.py | console"
pipe "examples/emit.rexx a b c | examples/upper.py | console"
pipe "examples\webget.py https://www.gutenberg.org/cache/epub/1342/pg1342.txt | locate /Darcy/ | examples\wordfreq.py 15 | console"
```

Interpreter discovery (first hit wins): `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`. Older than 3.10 fails the stage with a message that names the version found. Set `PIPE_PYTHON` to a virtual environment's `python.exe` to use that interpreter. Packages installed with `pip` are importable from stage scripts.

`examples/webget.py` uses `truststore` when installed so Python trusts the Windows certificate store behind HTTPS-inspecting antivirus or proxies. If `urllib` fails with `CERTIFICATE_VERIFY_FAILED` or "self-signed certificate in the certificate chain":

```text
py -m pip install truststore
```

## Workspace layout

| Crate | Role |
|---|---|
| `pipe-core` | Records, streams, coroutine dispatcher, `StageCtx` |
| `pipe-parser` | PIPE specification scanner/parser |
| `pipe-stages` | Built-in stages + runner |
| `pipe-rexx` | REXX bridge + subset interpreter |
| `pipe-python` | Python stage host (out-of-process interpreter) |
| `pipe-cli` | `pipe` binary |

## Tests

```bash
cargo test --workspace
```

How to require Regina and Python 3.10+ (so a missing install fails instead of falling back or skipping) is in [CONTRIBUTING.md](CONTRIBUTING.md). Compat golden cases live in `tests/compat/`.

## License

MIT OR Apache-2.0, at your option. See [LICENSE](LICENSE), [LICENSE-MIT](LICENSE-MIT), and [LICENSE-APACHE](LICENSE-APACHE).

## Contributing

Build, test, and Cursor notes for people working from a clone are in [CONTRIBUTING.md](CONTRIBUTING.md).

## Trademarks

CMS Pipelines and z/VM are trademarks of IBM. This project is not affiliated with or endorsed by IBM.
