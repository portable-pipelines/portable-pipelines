# Desktop editor and debugger

This is a native Rust window (`egui` / `eframe`). The default renderer is wgpu (Direct3D 12, Vulkan, Metal, or GL, with automatic GPU fallback). If that cannot start, the same process tries OpenGL 2.0 via glow. It is not a browser, not a local web server, and not a webview.

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

`pipe.exe` prints `PIPE: starting …` before it waits for the window. If the window fails to appear, that same console prints the GUI exit status and a plain-language renderer error. `pipe-gui.exe` attaches to this console so its own errors are not swallowed by the Windows subsystem.

The default is `--renderer auto`: try wgpu first, then glow. Force one backend with `--renderer wgpu`, `--renderer glow`, or `PIPE_GUI_RENDERER` (`auto`, `wgpu`, or `glow`). The flag wins over the environment variable. `pipe --gui --renderer glow` is forwarded to `pipe-gui`; it is not treated as pipeline text.

Noninteractive children owned by the GUI (Python/REXX helpers, interpreter probes, COMMAND/SUBCOM `cmd /C` capture) set `CREATE_NO_WINDOW` so they do not flash a cmd window. Stdin/stdout/stderr capture is unchanged. A GUI program started from COMMAND still shows its own window.

## Linux and macOS

```bash
cargo build --release -p pipe-cli -p pipe-gui
./target/release/pipe --gui
./target/release/pipe --gui examples/marina/to-csv.pipe
```

## What Run does

Opening a file, pasting text, and drawing the graph do not execute the pipeline. Press **Run** (or **Step**) to start a real debug session on a worker thread. The first **Step** click starts the session and takes one record cycle: the selected stage if it can still run, otherwise a stage that has a record waiting. It never keeps targeting an already-ended stage. If nothing can move, the console names the stage that is waiting (for example **Waiting for tuck.py to read**). Pause and Stop stay usable; pause takes effect at the next stage read, write, or short. CONSOLE output appears in the window. CONSOLE stdin is refused (treated as end of file) so the window does not wait on a hidden terminal. A new Run after the session has ended starts fresh and clears the console; Run while paused continues the same session.

`.py` / `.rexx` stages show as the file name plus language and arguments (for example `tuck.py` / `Python  owner`). Topology, hover, and graph drawing do not open or run those files. **View script file** (records pane or right-click) reads the path from the working directory you chose; it is a file view, not interpreter locals or stepping inside Python/REXX.

The left source pane is the specification. Selecting a stage still highlights that span and keeps the bytes. The pane wraps to its width so a long one-line pipe stays readable; interacting with the graph does not delete source.

**Records** is a resizable, collapsible pane under the graph. With nothing selected it shows a run summary (status, steps taken, per-stage marks, where a record is waiting) and **Select a stage or connection to see its records**. **About this pipeline** (collapsed) names scripts and files in plain language. Hover or click a stage or connection for the same scoped data: a compact spreadsheet grid (header row, row numbers, cell borders, stripes, sticky header, thin solid ~6px scroll bars reserved beside the cells so they never cover text). JSON objects (CSV output) use the real field names as columns. Plain text such as `apple` is one column named **value**. A raw CSV header line is not split into columns. Long cells ellipsize; the full value is on hover. Nested JSON objects use the same grid. A connection shows only that edge’s waiting record and the writer’s outgoing events — not the writer’s input. A single **Waiting for LOOKUP to read** line (amber hourglass) is the live pipe; **Recently sent from** (or **Recently seen at** a stage) keeps only the last few records, not a replay. If the waiting record is also in that short list, it is tagged **still waiting**. Marks are drawn shapes, not font glyphs: hourglass = waiting, zzz = idle (waiting for input), arrow = sent, X = discarded, stack = buffering (records kept until the stage is ready to send), check = done/finished. There is no separate “read and passed on” mark — a sender’s arrow count plus an hourglass on anything still waiting covers that. SORT (and BUFFER) wait for all input before sending; the box shows a stack plus the known count (`received − sent − discarded`). If that count cannot be known, no number is shown. The graph shows an amber hourglass on a waiting connection, zzz on an idle live stage, and a green check on a finished stage. Stage boxes show a compact **in** count at the upper left (records read; omitted on sources with no input) and an **out** count at the upper right (records sent), plus buffering/discarded marks. Hover a count for a plain-language tooltip; multi-input stages name each source. A one-line legend sits at the bottom of the window. **More details** is optional. Hide the pane to give the graph the space.

Grab empty canvas with the primary mouse button to pan. Drag a stage to reposition it; edges follow. That offset is view-only: it does not change wiring or the specification. Click (no drag) still selects the stage and syncs the source highlight. Right-click a stage for Step, View script file, and Reset this node position. Hover a stage or edge: after a short delay a compact record preview opens against the box (no gap), stays while the pointer is on the stage, the popup, the bridge between them, or the scroll bars, and keeps a ~360ms grace so Remote Desktop jitter does not dismiss it. Click the stage or the popup to pin it; Esc, the Close button, or a click elsewhere dismisses it. The popup stays bounded so it never covers the whole graph. Hover does not run or open files. Selected or hovered stages highlight their input/output paths and dim the rest.

**Right-angle edges** toggles orthogonal vs direct connections. Orthogonal elbows stay in the column gutter instead of crossing through node boxes. Parallel connections that would sit on top of each other get a few pixels of lane offset so you can tell them apart; hover still picks the line under the pointer. **Fit** recenters the camera on the current layout (including any drags) without shrinking titles unreadably. **Reset layout** clears view-only node offsets and fits again.

**Quit** (and the window close box) abort the session. They do not run a paused pipeline to completion. Already-written host files stay written. A host file syscall already in flight is not interrupted; Quit waits a few seconds for that write, then the process exits so `pipe --gui` can return. Python/REXX helpers park between protocol frames so Stop/Quit can terminate the helper child.

Edits to the source do not change a session that is already live. Stop and Run to apply a new spec.

Quit and the window close box ask Save / Discard / Cancel when the buffer is unsaved. Discard and Cancel must take effect (Discard does not reopen the prompt). A cancelled or failed Save leaves the buffer open. Shutdown is still abort-not-run-to-completion.

## Troubleshooting: the window does not open on Windows

On some Windows laptops `pipe --gui` failed at glow init with “OpenGL 2.0 is not available” when launched at the machine, while the same binary opened over Remote Desktop. Device Manager can still show a healthy NVIDIA adapter (for example Quadro M2000M) and Microsoft Basic Display Adapter. That is a hybrid-graphics / WGL OpenGL problem, not a dead GPU.

The default is now wgpu, which on Windows prefers Direct3D 12, then Vulkan, then GL. If wgpu cannot start, the same process tries glow and prints why. If both fail, the console names each cause and how to override.

What to try:

```powershell
.\target\release\pipe.exe --gui
.\target\release\pipe.exe --gui --renderer wgpu
.\target\release\pipe.exe --gui --renderer glow
$env:PIPE_GUI_RENDERER = "glow"
.\target\release\pipe.exe --gui
```

Do that once sitting at the laptop and once over Remote Desktop. `--renderer wgpu` does not fall back to OpenGL, so it is the check that Direct3D/Vulkan alone can open the window. `--renderer glow` is the old OpenGL path.

Quit (and the window close box) still abort the debug session. They do not run a paused pipeline to completion. Helper children still use `CREATE_NO_WINDOW` so they do not flash a cmd window. Those behaviors are independent of the renderer.

## Rust version

The CLI and core crates still declare Rust 1.75. The GUI crate declares 1.85 because `egui` needs a newer compiler. `cargo test` at the repo root (no `--workspace`) uses `default-members` and does not build `pipe-gui`. `cargo test --workspace` builds it and needs a current stable toolchain.
