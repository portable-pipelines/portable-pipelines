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

`pipe.exe` prints `PIPE: starting …` before it waits for the window. If the window fails to appear, that same console prints the GUI exit status. `pipe-gui.exe` attaches to this console so its own errors are not swallowed by the Windows subsystem.

Noninteractive children owned by the GUI (Python/REXX helpers, interpreter probes, COMMAND/SUBCOM `cmd /C` capture) set `CREATE_NO_WINDOW` so they do not flash a cmd window. Stdin/stdout/stderr capture is unchanged. A GUI program started from COMMAND still shows its own window.

## Linux and macOS

```bash
cargo build --release -p pipe-cli -p pipe-gui
./target/release/pipe --gui
./target/release/pipe --gui examples/marina/to-csv.pipe
```

## What Run does

Opening a file, pasting text, and drawing the graph do not execute the pipeline. Press **Run** (or **Step**) to start a real debug session on a worker thread. The first **Step** click starts the session and takes the first record cycle on the selected stage (or the first stage if none is selected). Pause and Stop stay usable; pause takes effect at the next stage read, write, or short. CONSOLE output appears in the window. CONSOLE stdin is refused (treated as end of file) so the window does not wait on a hidden terminal. A new Run after the session has ended starts fresh and clears the console; Run while paused continues the same session.

`.py` / `.rexx` stages are external scripts, not unknown builtins. Topology inspection does not open those files or run them.

Grab empty canvas with the primary mouse button to pan. **Right-angle edges** toggles orthogonal vs direct connections. Fit keeps stage titles readable and does not shrink a long graph to unreadably small text.

**Quit** (and the window close box) abort the session. They do not run a paused pipeline to completion. Already-written host files stay written. A host file syscall already in flight is not interrupted; Quit waits a few seconds for that write, then the process exits so `pipe --gui` can return. Python/REXX helpers park between protocol frames so Stop/Quit can terminate the helper child.

Edits to the source do not change a session that is already live. Stop and Run to apply a new spec.

Quit and the window close box ask Save / Discard / Cancel when the buffer is unsaved. Discard and Cancel must take effect (Discard does not reopen the prompt). A cancelled or failed Save leaves the buffer open. Shutdown is still abort-not-run-to-completion.

Some Windows machines fail to open the window locally with the glow OpenGL renderer while the same build works under Remote Desktop. NVIDIA and Microsoft Basic Display Adapter have both been reported healthy in that situation. Cause is not proven; this is not a renderer migration.

## Rust version

The CLI and core crates still declare Rust 1.75. The GUI crate declares 1.85 because `egui` needs a newer compiler. `cargo test` at the repo root (no `--workspace`) uses `default-members` and does not build `pipe-gui`. `cargo test --workspace` builds it and needs a current stable toolchain.
