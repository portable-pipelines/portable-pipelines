# The Portable Pipelines GUI

This is a long, friendly guide to `pipe --gui`: the Portable Pipelines
GUI. It is written for a pipelines user who is opening the window for
the first time and wants to *watch records move*.

The GitHub Release includes `pipe-gui` next to `pipe`. From the unpacked
folder:

```text
pipe --gui
pipe --gui somefile.pipe
```

On Windows, from that folder: `pipe --gui`. `pipe --gui` looks for
`pipe-gui` in the same folder as `pipe`; it does not search PATH.

It is a native desktop window. Records start moving when you press
**Run** or **Step one record**. Quit and the window close box abort
the session.

The window looks the same on Windows, macOS, and Linux.

## How this guide is laid out

1. This page: what the window is, how to launch it, a map of the chrome,
   and the one idea that makes everything else make sense.
2. [A first pipeline](gui/first-pipeline.md): a gentle tutorial. You
   paste three fruits, filter with LOCATE, press Step, then Run, and
   watch apple and apricot go through while banana is discarded.
3. [Worked walkthroughs](gui/walkthroughs.md): a multi-stream pipeline
   so secondary connections show; SORT so buffering shows; a Python
   (and REXX) stage so script info and **View script file** show; CSV
   and the marina example so a real spreadsheet of fields shows.
4. [Reference](gui/reference.md): every pane, button, mouse gesture,
   keyboard shortcut, mark, hover peek, and Records grid, in one place.
5. [Glossary](gui/glossary.md): every mark, badge, heading, and status
   word the window paints.
6. [FAQ and troubleshooting](gui/faq.md): “why does this stage show
   zzz?”, renderer / Remote Desktop notes, and the usual first-day
   snags.

If you only have twenty minutes, read this page and the first-pipeline
tutorial. Come back to the walkthroughs when you want secondary streams,
SORT, or a `.py` stage under the microscope.

The same topics in shorter form live in the
[v0.2.0 release notes](release-notes-v0.2.0.md). JSON fields and CSV —
what the spreadsheet columns actually *mean* — are taught in
[JSON rows and CSV](json-and-fields.md).

## The one idea

A pipeline is stages connected by `|`. A record is written by one
stage and read by the next. Delay, filters, SORT, and a secondary
output are all consequences of that connector.

The GUI is a picture of that connector:

- Each box is a stage.
- Each line is one stream connecting a writer to a reader.
- A number on the left of a box (`→ 3`) is how many records that stage
  has **read**.
- A number on the right (`2 →`) is how many it has **sent**.
- An hourglass (<img src="gui/images/marks/waiting.png" alt="hourglass waiting mark" height="16">)
  on a line means a record is sitting there, waiting to be read.
- 💤 on a box means the stage is alive and has nothing to read yet.
- A green check (✓) means that stage has finished.
- A violet stack (<img src="gui/images/marks/holding.png" alt="violet buffering stack mark" height="16">)
  means the stage is **buffering**: it has read records and is holding
  them until it is ready to send (SORT and BUFFER do this).
- A grey-red down-arrow (↓) means a record was discarded instead of sent
  (LOCATE’s non-match, when you left the secondary output unconnected).

Those marks are painted shapes, so they stay readable. The one-line
legend at the bottom of the window is the same vocabulary.

![The empty GUI: Source on the left, an empty graph, Records underneath, Console at the bottom, and a Marks legend.](gui/images/empty.png)

*Just after `pipe --gui`, before any specification. The graph says
“Paste a specification or open a file. The graph is drawn from source
without running it.”*

## Launch

Unpack the GitHub Release so `pipe` and `pipe-gui` sit in the same
folder, then:

```text
pipe --gui
pipe --gui examples/marina/to-csv.pipe
```

Windows (cmd):

```bat
pipe --gui
pipe --gui examples\marina\to-csv.pipe
pipe --gui "docs\my file.pipe"
```

`--gui` is parsed before the rest of the argv is joined, so a quoted
path with spaces is one file. Working directory is the folder you
launched from. Relative paths in the spec
(`examples\marina\vessels.jsonl`) resolve from that folder unless you
change **working directory** in the window and press Run again.

The window title and taskbar use the Portable Pipelines site logo
(the same mark as https://portable-pipelines.github.io).

`pipe` prints `PIPE: starting …` before it waits for the window. If the
window fails to appear, that same console prints the GUI exit status
and a plain-language renderer error.

The default is `--renderer auto`: wgpu first (Direct3D 12, Vulkan,
Metal, or GL), then OpenGL via glow. `--renderer wgpu`, `--renderer
glow`, or `PIPE_GUI_RENDERER` (`auto`, `wgpu`, or `glow`) force one
backend. The flag wins over the environment variable.
`pipe --gui --renderer glow` is forwarded to `pipe-gui`; it is not
treated as pipeline text.

Renderer flags also live in the [reference](gui/reference.md#renderer)
and the
[FAQ](gui/faq.md#troubleshooting-the-window-does-not-open). Building
from a clone is in [CONTRIBUTING.md](../CONTRIBUTING.md).

If the window does not open, go to
[FAQ and troubleshooting](gui/faq.md#troubleshooting-the-window-does-not-open).

## A map of the window

From the top:

**Title row.** “Portable Pipelines”. On the right:
**Open**, **Save**, **Save as**, **Quit**.

**File row.** The path, or `untitled`, with `• unsaved` when the
text differs from disk. Then **working directory** and **Change**.
Then `newlines LF` or `newlines CRLF`, and `UTF-8 BOM` when a BOM is
present.

**Run row.**

![The Run row: Run, Pause, Step one record, Stop, Fit, Reset layout, Zoom, Right-angle edges, Show records, and the Idle status.](gui/images/toolbar.png)

- **Run** starts (or continues) a live session. After Finished or Stop
  it reads **Run again** and starts over from the top.
- **Pause** pauses at the next stage read, write, or short.
- **Step one record** starts the session if needed, then takes one
  record cycle. During a live Run it pauses at the next record. After
  Finished or Stop it reads **Step again** and starts over with one
  record.
- **Stop** ends the session. Already-written host files stay written.
- **Restart** starts a new session from the top without running it.
  Source and layout stay; marks, counts, and Records reset.
- **Fit** recenters the camera.
- **Reset layout** clears view-only node drags and fits again.
- **Zoom −** / **Zoom +** and a percent.
- **Right-angle edges** (on by default): connections bend in the column
  gutter.
- **Show records**: **In Records pane** (default) or **On the graph**.
  The choice is remembered the next time you open the window.
- A status word on the right: Idle, Running, Paused, Ended, Stopped, or
  Failed.

A hint sits next to the checkbox: *Drag a stage to move it (view only).
Drag empty canvas to pan.*

**Source** (left). The specification is authoritative. The graph is
drawn from it. Selecting a stage highlights that stage's text in
Source. A long one-line pipe wraps to the pane width.

**Graph** (center). Stage boxes, connections, in/out badges, and marks.

**Records** (under the graph). With **Show records** set to **In Records
pane** (the default), this is the home for record grids. A run summary
when nothing is selected; the spreadsheet for a stage or connection
when you click it. Hover is a short peek (name, full clause, status,
counts) with the hint *Click to see records below.* **On the graph**
puts those same grids on the hover card as well; click still fills
Records. **Hide** closes the Records pane so the graph has more room.
**About this pipeline** (collapsed) names scripts and files in plain
language.

**Console and messages** (bottom). CONSOLE output is captured here.
CONSOLE stdin is refused (treated as end of file) so the window cannot
block on a hidden terminal.

**Marks** (very bottom). The legend. **Show words** / **Hide words**
toggles the labels next to the icons.

![The Marks legend: waiting, idle, in, out, discarded, buffering, done.](gui/images/legend.png)

*The always-visible legend. Hover a mark for a one-line tooltip.*

## What to do first

1. Unpack the download so `pipe` and `pipe-gui` sit in the same folder.
2. Open the window with `pipe --gui`.
3. Follow [A first pipeline](gui/first-pipeline.md) with three fruits
   and LOCATE. You will press **Step one record** once, then **Run**,
   and read the in/out badges (`→ N` / `N →`) and the discard down-arrow (↓).
4. When you want secondary streams, SORT, or a `.py` stage, use the
   [walkthroughs](gui/walkthroughs.md).

You can also open a file:

```bat
pipe --gui examples\marina\to-csv.pipe
```

```bash
pipe --gui examples/marina/to-csv.pipe
```

The graph will fill in. Press **Run** when you are ready to write
`examples/marina/out/*.csv`.

Python and REXX stages are files you write (`examples\upper.py`). The
box shows the file name and the language. **View script file** reads
the file from the working directory.

File stages take a host path (`notes.txt`, `examples\orders.csv`).
CONSOLE in this window captures output.

## Where the rest of the docs live

| Page | What it is |
|---|---|
| [A first pipeline](gui/first-pipeline.md) | Tutorial. Three fruits and LOCATE. |
| [Walkthroughs](gui/walkthroughs.md) | Secondary streams, SORT, Python/REXX, CSV, marina. |
| [Reference](gui/reference.md) | Every control, in one place. |
| [Glossary](gui/glossary.md) | Every mark, badge, and label. |
| [FAQ and troubleshooting](gui/faq.md) | Why zzz, why Step prepares, renderer notes. |
| [JSON rows and CSV](json-and-fields.md) | What `$amount` and the spreadsheet columns mean. |
| [STATUS](STATUS.md) | Which built-in stages work today. |
| [examples/README](../examples/README.md) | Copy-paste pipelines from the command line. |
