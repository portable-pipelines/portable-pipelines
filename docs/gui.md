# Desktop editor and debugger

This is a native Rust window (`egui` / `eframe` with the glow renderer). It is not a browser, not a local web server, and not a webview.

The signed GitHub Release zip still ships `pipe` only. `--gui` is a local-build preview until a later release includes `pipe-gui` on purpose.

## Windows (PowerShell)

From a clone. GNU host if you do not have Visual Studio Build Tools:

```powershell
# Without Visual Studio:
#   cargo +stable-x86_64-pc-windows-gnu build --release -p pipe-cli -p pipe-gui
cargo build --release -p pipe-cli -p pipe-gui
.\target\release\pipe.exe --gui
.\target\release\pipe.exe --gui examples\marina\to-csv.pipe
```

`pipe.exe` stays a console program. `pipe-gui.exe` is a Windows-subsystem window next to it. `--gui` is parsed before the rest of the argv is joined, so a path with spaces is one file argument when you quote it:

```powershell
.\target\release\pipe.exe --gui ".\docs\my file.pipe"
```

Working directory is the directory you launched from. Relative paths in the spec (`examples\marina\vessels.jsonl`) resolve from that folder unless you change Working directory in the window and press Run again.

## Linux and macOS

```bash
cargo build --release -p pipe-cli -p pipe-gui
./target/release/pipe --gui
./target/release/pipe --gui examples/marina/to-csv.pipe
```

## What Run does

Opening a file, pasting text, and drawing the graph do not execute the pipeline. Press **Run** (or **Step**) to start a real debug session on a worker thread. Pause and Stop stay usable; pause takes effect at the next stage read, write, or short. CONSOLE output appears in the window. CONSOLE stdin is refused (treated as end of file) so the window does not wait on a hidden terminal.

Edits to the source do not change a session that is already live. Stop and Run to apply a new spec.

## Rust version

The CLI and core crates still declare Rust 1.75. The GUI crate declares 1.85 because `egui` needs a newer compiler. `cargo test` at the repo root (no `--workspace`) uses `default-members` and does not build `pipe-gui`. `cargo test --workspace` builds it and needs a current stable toolchain.
