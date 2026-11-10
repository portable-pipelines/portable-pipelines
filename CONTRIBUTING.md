# Contributing

Thank you for wanting to help with Portable Pipelines. This page is for people building and testing from a clone.

## Using Cursor (first time)

1. Open your clone in Cursor.
2. Ask the Agent to run tests: `cargo test`.
3. Change a stage under `crates/pipe-stages`, re-run tests, and try `cargo run -p pipe-cli -- "…"`.

Cursor Cloud VMs install Regina and Python from `.cursor/environment.json`. That file cannot set environment variables, so export the two flags below in the shell before `cargo test` when you want a missing install to fail instead of falling back or skipping.

## Tests

From the repo root:

```bash
cargo test --workspace
```

`cargo test` at the repo root (no `--workspace`) uses `default-members` and does not compile `pipe-gui`. That crate needs Rust 1.85+ (`egui`). CI runs both the default crates and `pipe-gui`.

To run the native window from a clone:

```bash
cargo build --release -p pipe-cli -p pipe-gui
./target/release/pipe --gui
```

```powershell
cargo build --release -p pipe-cli -p pipe-gui
.\target\release\pipe.exe --gui
```

`pipe --gui` looks for `pipe-gui` in the same folder as `pipe`, not on PATH. The GitHub Release archives already include both binaries; this build step is for contributors.

On Windows PowerShell:

```powershell
cargo test --workspace
```

`rust-toolchain.toml` pins the `stable` channel, so Linux and macOS clones work with plain `cargo`. You do not need `cargo +stable` there.

On Windows, rustup often defaults to the MSVC host (`x86_64-pc-windows-msvc`), which needs Visual Studio's `link.exe`. If you do not have Visual Studio, install the GNU toolchain and either prefix cargo or change the default host:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu test --workspace
# Or once: rustup set default-host x86_64-pc-windows-gnu
```

The packaging script (`scripts/package-windows.ps1`) defaults to that GNU host toolchain so a local zip does not need Visual Studio. Pass `-Target msvc` to match the GitHub Release zip (needs `link.exe`). Leave `rust-toolchain.toml` on `channel = "stable"` so Linux and macOS clones keep working.

### Requiring Regina and Python

REXX stages prefer a user-installed Regina library and otherwise use the embedded subset interpreter. Python stages need Python 3.10 or later; without it, those cases skip.

- `PIPE_REQUIRE_REGINA=1` makes tests fail if Regina is not discovered or loaded, instead of silently using the subset interpreter.
- `PIPE_REQUIRE_PYTHON=1` makes tests fail if Python 3.10+ is not discovered, instead of skipping.

Contributors who have both installed (and Cursor Cloud agents) should export them so a missing install cannot hide a real bug:

```bash
PIPE_REQUIRE_REGINA=1 PIPE_REQUIRE_PYTHON=1 cargo test --workspace
```

If you do not have Regina or Python, leave the matching variable unset so the suite still passes on the subset path or by skipping those cases.

Install notes are in the [README](README.md).

## Continuous integration

Every pull request, and every push to `master`, runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml) on Ubuntu, macOS (Apple silicon), and Windows. Each OS is its own matrix job (`fail-fast: false`), so one failure does not hide the others. The workflow only needs `contents: read`. It does not sign binaries and does not use secrets.

The Linux job matches the command we use here: install `regina-rexx`, `libregina3-dev`, and `python3`, then

```bash
PIPE_REQUIRE_REGINA=1 PIPE_REQUIRE_PYTHON=1 cargo test --workspace --no-fail-fast -- --test-threads=1
```

macOS installs Regina with Homebrew (`brew install regina-rexx`). Windows uses the MSVC toolchain and the same official Regina zip the release job fetches (`scripts/fetch-regina-windows.ps1`). A green badge on the README tracks this workflow.

CI also runs the Python release-helper tests and `actionlint` on Ubuntu
(`.github/workflows/ci.yml`, job `release helpers`). Those checks never
create a tag or publish a GitHub Release.

## Releasing

Maintainers: [docs/releasing.md](docs/releasing.md) is the full story.
Merging ordinary pull requests does not publish. Merging the automation
setup does not publish. After John has reviewed the version and the notes
and has said to go, merging a dedicated release-intent pull request is
what publishes.

## Reference PDF

Behavioral authority is CMS/TSO Pipelines Author's Edition 1.1.11 (document SL26-0018-05). Keep your local copy of that PDF; it is not included in this repo (IBM copyright). Do not commit it. See [docs/reference/README.md](docs/reference/README.md).
