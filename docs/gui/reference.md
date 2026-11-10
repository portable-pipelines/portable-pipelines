# GUI reference

This page is the Portable Pipelines GUI, control by control. The
[tutorial](first-pipeline.md) and [walkthroughs](walkthroughs.md) are
the guided tour. If you are hunting for one checkbox, start from the
list below.

1. [Launch and files](#launch-and-files)
2. [The Source pane](#the-source-pane)
3. [Run, Step, Pause, Stop, Quit](#run-step-pause-stop-quit)
4. [The graph](#the-graph)
5. [Marks and badges](#marks-and-badges)
6. [Hover cards](#hover-cards)
7. [The Records pane](#the-records-pane)
8. [The spreadsheet](#the-spreadsheet)
9. [Console and messages](#console-and-messages)
10. [Python, REXX, and COMMAND](#python-rexx-and-command)
11. [Keyboard](#keyboard)
12. [Renderer](#renderer)
13. [Limits](#limits)

The [glossary](glossary.md) is the same vocabulary as a table. The
[FAQ](faq.md) is symptoms and causes.

## Launch and files

`pipe --gui` starts sibling `pipe-gui` and waits until that window
closes. `pipe-gui` can also be started on its own. One optional file
argument is the specification to open:

```bat
pipe --gui
pipe --gui examples\marina\to-csv.pipe
pipe --gui "docs\my file.pipe"
```

```bash
pipe --gui
pipe --gui examples/marina/to-csv.pipe
```

`--gui` is parsed before the rest of argv is joined, so a quoted path
with spaces stays one file. Working directory is the directory you
launched from. Relative paths in the spec resolve from that folder
until you press **Change** next to **working directory** and then Stop
and Run.

**Open** / **Save** / **Save as** use a small picker (Pipeline filter:
`.pipe` and `.txt`). **newlines LF** or **newlines CRLF** follows the
file. **UTF-8 BOM** appears when a BOM is present.

`untitled  • unsaved` means the Source text has no file yet, or
differs from disk. Quit and the window close box ask **Save the
Source text before closing?** — Save / Discard / Cancel. Discard
takes effect. Cancel and a failed Save leave Source open.

Edits to Source wait for Stop and Run to apply. The Run row warns:
*Source changed. Stop and Run to apply. The live session still uses
the previous spec.*

The GitHub Release includes `pipe-gui` next to `pipe`. `pipe --gui`
looks for it in the same folder as `pipe`, not on PATH. Building from
a clone is in [CONTRIBUTING.md](../../CONTRIBUTING.md).

## The Source pane

Source is labelled **authoritative**. The graph is drawn from these
bytes. Selecting a stage highlights that stage's text in Source. A
long one-line pipe wraps to the pane width so it stays readable.

Paste, type, or Open a file. Parser diagnostics and unknown verbs show
up in Records and in **Console and messages**. An unknown verb is
still drawn (often in a red box) so you can see where it sits; Run
fails until it is a built-in or a `.py` / `.rexx` / `.rex` path.

## Run, Step, Pause, Stop, Quit

**Run** starts a real session on a worker thread. If a session is
already paused, Run continues it. A new Run after the session has
ended starts fresh and clears the console.

**Step one record** starts the session if needed, then takes one record
cycle: the selected stage if it can still run, otherwise a stage that
has a record waiting. It skips a stage that has already ended. If
nothing can move, the console names the stage that is waiting.

The first Step click therefore does two jobs: it prepares the session,
and it takes a cycle. The console says `Session ready. Idle: nothing
has run yet.` and then names the stage that moved. See
[Why Step prepares first](first-pipeline.md#step-once-the-session-prepares-then-one-record-moves)
in the tutorial.

**Pause** takes effect at the next stage read, write, or short.

**Stop** ends the session. Already-written host files stay written.

**Quit** (and the window close box) abort the session. A host file
syscall already in flight is given a few seconds; then the process
exits so `pipe --gui` can return. Python/REXX helpers park between
protocol frames so Stop/Quit can terminate the helper child.

Status words, painted next to the Run row:

| Status | Meaning |
|---|---|
| Idle | No live session, or a session that has not started work. |
| Running | The worker is in a record cycle. |
| Paused | Waiting for Run or Step. |
| Ended | The pipeline finished. `rc 0` is success; `Ended (rc)` otherwise. |
| Stopped | You pressed Stop. |
| Failed | The session aborted with a message. |

Records repeats the same facts as **Ended rc 0**, **Stopped  rc N**,
**Failed: …**, or **Idle — nothing has run yet**.

## The graph

Each box is a stage. The title is the verb (or the script file name).
The subtitle is the rest of the clause (`literal apple`, `locate /ap/`,
`Python`, `csv write`). A labelled stage shows `L: LOCATE`. A CALLPIPE
connector is drawn as a connector.

Grab empty canvas with the primary mouse button to pan. Drag a stage to
reposition it; connections follow. That offset is view-only. Click (no
drag) still selects the stage and syncs the Source highlight. Selected
or hovered stages highlight their input/output paths and dim the rest.

**Fit** recenters the camera on the current layout (including any
drags) without shrinking titles unreadably.

**Reset layout** clears view-only node offsets and fits again.

**Zoom −** / **Zoom +** clamp between 45% and 280%. The percent is
shown next to the buttons.

**Right-angle edges** (default on) bends connections in the column
gutter. Off, connections are straight. Stream 1 is labelled
**secondary** and drawn in a second colour. Stream 2 and up say
**stream N**.

Right-click a stage: **Step this stage**, **View script file** (when
the verb is a `.py` / `.rexx` / `.rex` path), **Reset this node
position**. Right-click empty canvas: **Fit graph**.

## Marks and badges

Marks are painted shapes. Idle is a painted zzz (💤). Done is a painted
green check (✓). Discarded is a painted down-arrow (↓).

![LOCATE after a full run: in-count 3, out-count 2, discard 1, green checks on every stage.](images/locate-badges.png)

*In-counts sit on the upper left and point right (`→ 3`). Out-counts
sit on the upper right and point right (`2 →`). Both arrows point
with the flow, left to right.*

| Mark | Where you see it | Meaning |
|---|---|---|
| Amber hourglass <img src="images/marks/waiting.png" alt="hourglass waiting mark" height="16"> | On a connection; in Records | A record is sitting at a stage’s input, not yet read. |
| Cyan zzz 💤 | On a live stage or connection; in Records | Idle, waiting for input. |
| Cyan in-arrow <img src="images/marks/in.png" alt="cyan in-arrow mark" height="16"> | Badge `→ N` on the upper left of a box | Records this stage has read. Omitted on sources with no input. |
| Accent out-arrow <img src="images/marks/out.png" alt="blue out-arrow mark" height="16"> | Badge `N →` on the upper right | Records this stage has sent. |
| Grey-red down-arrow ↓ | Next to the out side; in sheets | The record was discarded instead of sent. |
| Violet stack <img src="images/marks/holding.png" alt="violet buffering stack mark" height="16"> | On a buffering box; in Records | Records kept inside until the stage is ready to send. |
| Green check ✓ | On a finished box; on a drained connection | This stage (or writer) has finished. |

A sender’s arrow count plus an hourglass on anything still waiting
covers “read and passed on.”

The buffering count is `received − sent − discarded` when that number
can be known. SORT and BUFFER wait for all input before sending; the
box shows the stack plus that count. If the count cannot be known, the
stack still appears and the number is omitted.

The in-badge is omitted on a source that has no pipeline input (a
LITERAL that only produces, a `<` that only reads a host file). Those
boxes still show an out-count once they have sent. Hover a count for a
plain-language tooltip; multi-input stages name each source.

The legend at the bottom of the window is this same list. **Show
words** / **Hide words** toggles the labels. Hover a legend icon for
the tooltip.

A full table, including heading text (**→ Received**, **Sent →**,
**still waiting**), is in the [glossary](glossary.md).

## Hover cards

Hover a stage or a connection. After a short delay a compact record
preview opens against the box (no gap). It stays while the pointer is
on the stage, the popup, the bridge between them, or the scroll bars,
and it keeps a short grace (~360 ms) so Remote Desktop jitter is
easier to live with.

Click the stage or the popup to pin it. A **Pinned** label appears.
**Close**, Esc, or a click elsewhere dismisses it. The popup stays
bounded so the graph remains visible around it.

The card and the Records pane share one grid. Pinning the card keeps
the popup on screen.

## The Records pane

**Records** is a resizable, collapsible pane under the graph. **Hide**
/ **Show** toggle it.

With nothing selected it is a run summary: status, steps taken,
per-stage marks, and where a record is waiting. If every stage has
ended it says **All stages finished.** If the session is live and
nothing is waiting it says **Idle, waiting for input**. Otherwise each
waiting connection gets an hourglass line such as **Waiting for
LITERAL to read**. At the bottom: *Select a stage or connection to see
its records.*

With a stage or connection selected it shows the same scoped data as
the hover card, with more room: small counts, holding sentence,
waiting record, **→ Received** / **Sent →** / **↓ Discarded** sheets,
then optional **More details**.

**More details** is a checkbox. For a stage it adds live name, last
step, connections, recent history. For a connection it reminds you
that the connection shows records sent on it.

**About this pipeline** (collapsed) names, in plain language:

- How Step works: *Each Step moves one record through one stage. A
  stage can have more than one input or output.*
- Python and REXX files, and that they run when you Step or Run.
- Files the spec reads (`<`) and writes (`>`, `>>`).
- CONSOLE: output appears in this window; stdin is end of file.
- COMMAND / PIPECMD / SUBCOM: may run a program on this computer when
  you Step or Run.

## The spreadsheet

One layout engine, two homes (hover card and Records). It auto-picks
from the bytes:

| The record looks like | The sheet shows |
|---|---|
| JSON object | One column per field name (`customer`, `amount`, …). |
| Plain text | One column named **value**. |
| Binary | Hex, with a small **binary** label. |
| Empty | *empty record*. |
| Nested JSON in a cell | A nested summary; the Records pane can inspect it. The hover card stays compact. |

A raw CSV header line stays in one **value** column. Parse it with
`csv` first; the **Sent →** sheet then uses the field names. See the
[CSV walkthrough](walkthroughs.md#spreadsheet-columns-csv-in-json-objects-out).

Header row, row numbers, cell borders, stripes, sticky header. Long
cells ellipsize; the full value is on hover. Thin solid ~6 px scroll
bars sit beside the cells so they never cover text.

The sheet keeps the last 1000 records. If a stage sent 2500, the
heading reads **Sent (showing last 1000 of 2500)** and the first row
number is 1501. The live pipe — the record still waiting to be read —
is a separate hourglass row. If that waiting record is also in the
short recent list, it is tagged **still waiting**.

If you expected field columns and see **value**, you are looking at
raw text before a CSV/JSON stage, or at records that are still plain
text.

## Console and messages

**Console and messages** captures two streams:

- Log lines from the GUI itself: specification loaded (not executed),
  session ready, Run requested, side-effect notice, stage notes,
  `run finished: Ended { rc: 0 }`, parser errors.
- CONSOLE stage output, as `pipe` would have printed it.

CONSOLE **stdin is refused** (treated as end of file) so the window
cannot block on a hidden terminal. Feed records with LITERAL, `<`,
or a producer stage. The pane’s own subtitle says this.

A new Run after the session has ended clears the console.

## Python, REXX, and COMMAND

A stage whose verb is a path ending in `.py`, `.rexx`, or `.rex` is a
language stage. The box title is the file name (`upper.py`). The
subtitle is the language (`Python`, `REXX`). Arguments after the path
show in Records as **arguments:**.

**View script file** (Records pane or right-click) reads the path
resolved from the working directory you chose. The modal says *Read
from disk. Working directory:* and *Not interpreter locals, and not a
live step inside the script.* Close the modal and open it again to
re-read after an external edit.

On Windows, GUI-owned helper children (Python, REXX, interpreter
probes, COMMAND/SUBCOM `cmd /C` capture) set `CREATE_NO_WINDOW` so
they skip a cmd flash. Stdin/stdout/stderr capture is unchanged. A GUI
program started from COMMAND still shows its own window.

COMMAND / PIPECMD / SUBCOM may run a host program when you Step or
Run. **About this pipeline** says so.

Write the file path as the verb (`examples/upper.py`). The
[Python walkthrough](walkthroughs.md#python-and-rexx-script-info-and-source-view)
shows the Records pane and **View script file**.

Python needs 3.10+ on the machine (`PIPE_PYTHON`, then `py -3` on
Windows, then `python3`, then `python`). REXX on Windows uses bundled
Regina in the release zip; on Linux and macOS install Regina for full
behaviour, otherwise the embedded subset runs. See
[v0.2.0 requirements](../release-notes-v0.2.0.md#requirements).

## Keyboard

| Key | Action |
|---|---|
| F5 | Run |
| Ctrl+Enter (⌘+Enter) | Run |
| F10 | Step one record |
| Shift+F5 | Stop |
| Ctrl+S (⌘+S) | Save |
| Ctrl+O (⌘+O) | Open |
| Esc | Dismiss the hover card |

Mouse:

| Gesture | Action |
|---|---|
| Drag empty canvas | Pan |
| Drag a stage | Move it (view only) |
| Click a stage or connection | Select; pin the hover card; highlight Source |
| Hover a stage or connection | Preview card after a short delay |
| Hover an in/out badge | Tooltip |
| Right-click a stage | Stage menu |
| Right-click empty canvas | Fit graph |
| Scroll bars on a sheet | Scroll; they keep hover open |

## Renderer

The default is `--renderer auto`: try wgpu first (Direct3D 12,
Vulkan, Metal, or GL, with automatic GPU fallback), then OpenGL 2.0
via glow in the same process. Forced backends stay on that backend.

```bat
pipe --gui
pipe --gui --renderer wgpu
pipe --gui --renderer glow
set PIPE_GUI_RENDERER=glow
pipe --gui
```

```bash
pipe --gui
pipe --gui --renderer wgpu
pipe --gui --renderer glow
PIPE_GUI_RENDERER=glow pipe --gui
```

The flag wins over `PIPE_GUI_RENDERER`. `pipe --gui --renderer glow`
is forwarded to `pipe-gui`; it is a launch flag.

On some Windows laptops, OpenGL 2.0 is missing when you sit at the
machine (even with a healthy NVIDIA adapter and Microsoft Basic
Display Adapter in Device Manager) while the same binary opens over
Remote Desktop. wgpu prefers Direct3D 12 in that situation. If both
backends fail, the `pipe --gui` console names each cause and how to
override. Walk the flags once at the laptop and once over RDP.

`--renderer wgpu` is the check that Direct3D/Vulkan/Metal/GL-via-wgpu
alone can open the window. `--renderer glow` is the OpenGL path.

## Limits

- **View script file** is bytes on disk.
- CONSOLE stdin is end of file. Feed records with LITERAL, `<`, or a
  producer.
- Quit aborts the session.
- CSV field columns appear after a `csv` stage has parsed the header.
- Every specification in this guide is a legal `pipe "…"` pipeline.
  The window is how you watch.

The [FAQ](faq.md) is the shorter form of those sentences, as questions.
