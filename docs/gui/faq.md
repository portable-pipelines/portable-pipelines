# FAQ and troubleshooting

Short questions first, then the window-won’t-open notes. The
[tutorial](first-pipeline.md) is the slow version of several of these
answers. The [glossary](glossary.md) is the name of every mark.

## FAQ

### Why does a stage show zzz?

zzz (💤) means **idle, waiting for input**. The stage would run if a
record arrived. It is the usual look of a graph you have not run yet,
and of a downstream stage whose producer has not written.

A green check (✓) means the stage has **finished**. A HOLE after a
buffering SORT is zzz until SORT sends; then both become checks.

See [Why zzz, and why a check](glossary.md#zzz--idle-waiting-for-input)
and the [SORT walkthrough](walkthroughs.md#sort-holds-records-until-input-ends).

### What does buffering mean? Why is SORT not sending?

SORT (and BUFFER) wait until they have seen every input record before
they send the first output record. The GUI paints a violet stack on
the box and a sentence such as **Buffering 1 record (SORT waits for
all input before sending)**.

Those records are inside the stage. Press **Run** or keep stepping
until input ends; SORT will drain in order and the stack will
disappear.

### Why does Run say “0 steps taken”?

**N steps taken** counts **Step one record** clicks. A session you
only **Run** to completion shows **0 steps taken** and still lists
every stage as finished. Step once, then Run, and it will say **1
step taken**.

### Why did the first Step not filter anything?

The first **Step one record** has to **prepare** a session and then
take one record cycle. The console says `Session ready. Idle: nothing
has run yet.` and then names the stage that moved, often a LITERAL
producing its argument.

A record cycle is one stage doing one unit of work. Press Step again,
or press **Run**.

See [Why Step prepares first](first-pipeline.md#step-once-the-session-prepares-then-one-record-moves).

### Why did banana get a down-arrow on LOCATE, and then not get one later?

With the secondary output unconnected, LOCATE discards non-matches.
That is the down-arrow. Connect stream 1 (`L: locate /ap/ | hole ? L: | hole`)
and banana travels the purple **secondary connection**. The secondary
HOLE’s in-count becomes 1.

Same records, different topology. Walkthrough:
[Secondary streams](walkthroughs.md#secondary-streams-so-connections-show).

### Why do in and out both point right?

Records flow left to right. `→ 3` is “three flowed in” and `2 →` is
“two flowed out.” Both badges point with the flow.

### Why is apricot listed before apple?

Chained LITERAL stages emit right-to-left: each writes its own
argument, then copies whatever arrived from the left. That is how
LITERAL is specified in the Author’s Edition. The GUI is showing the
order the stages actually wrote. The same `pipe "…"` command prints
the same order on the terminal.

### Hover is only a short peek.

That is the default (**Show records** → **In Records pane**). The peek
names the stage or connection, shows status and counts, and says
*Click to see records below.* Record grids live in the Records pane,
which stays after you click. If a shaky Remote Desktop mouse makes
the peek flicker, click the stage: the spreadsheet does not depend on
hover.

Choose **On the graph** if you want the record grids on the hover
card itself. That card stays open when you move onto it, so you can
scroll and select. The Records pane then shows a short hint instead of
grids. Switch back to **In Records pane** to see them below again. The
setting is remembered; Restart does not reset it.

### LOOKUP succeeded, then tuck.py failed. Where do I look?

A missing `$field` is an empty key, so LOOKUP can still pair records.
The stage that used the field warns:
`LOOKUP (stage 3): field $owner_i not found in input record; it has owner_id, name, ...`.
Click the LOOKUP box. Records and Console show that line. Source and
the Properties operands line squiggle `$owner_i` once records have
flowed (`no field owner_i in the input; did you mean owner_id?`).

### How does the spreadsheet pick columns?

The sheet looks at the bytes:

- JSON object → field-name columns
- plain text → one column **value**
- binary → hex
- nested JSON → a nested cell in Records

A raw CSV header stays in **value** until a `csv` stage parses it. If
you expected columns and see **value**, you are looking at the input
side of CSV, or at records that are still plain text.

### Can I change column widths?

Yes. Drag the right border of a column header to widen or narrow it.
The whole grid grows as the column grows. Double-click the border to
fit the column to its contents. Drag the grid’s right or bottom edge
to resize the frame. Widths stay while the run continues, and are
remembered per stage or connection (Received, Sent, and ↓ Discarded
each keep their own). When the grid is wider than Records or the hover
card, that pane or card scrolls.

### The sheet says “showing last 1000 of 2500.”

History is capped at 1000 records per direction. Row numbers keep the
original sequence, so the first visible row might be 1501. The live
waiting record is a separate hourglass row.

### CONSOLE does not wait for me to type.

CONSOLE stdin is refused (end of file) so the window cannot block on a
hidden terminal. Output is captured in **Console and messages**. Feed
the pipeline with LITERAL, `<`, or a producer stage.

### I changed Source while a run was going.

The run stops. The next **Run** or **Step one record** starts from the
beginning. Pipes run only when you press Run or Step.

### The graph did not update when I typed.

The text does not parse yet. The graph keeps the last valid layout. A
wavy underline marks the error in Source, with the message next to it.
Fix the text and the graph updates.

### Relative paths are wrong (`examples/upper.py` not found).

Working directory is the directory you launched from, shown on the
file row. Press **Change**, pick the clone root, then Stop and Run.
**View script file** shows **resolved from working directory:** so you
can see what the helper will open.

### The box says unknown verb / unknown stage PYTHON.

The verb needs to be a built-in or a `.py` / `.rexx` / `.rex` path.
A common trip: writing `python examples/upper.py` as if PYTHON were a
stage. Use the path as the verb: `examples/upper.py`. See the
[Python walkthrough](walkthroughs.md#python-and-rexx-script-info-and-source-view).

### Does View script file step through my Python?

It reads the file from disk. The modal says *Not interpreter locals,
and not a live step inside the script.* The Records grid is the
pipeline records that went in and out of the helper.

### Will COMMAND flash a cmd window on Windows?

GUI-owned helpers set `CREATE_NO_WINDOW`, so a captured `cmd /C`
skips a flash. A GUI program *you* start from COMMAND still shows its
own window.

### Does Quit finish the pipeline?

Quit and the close box abort. Already-written host files stay written.
A write already in flight is given a few seconds, then the process
exits so `pipe --gui` can return.

### I closed the window and it asked to save.

The specification in Source is unsaved (`• unsaved`). Save / Discard /
Cancel. Discard takes effect. This is about the Source text.

### Is `pipe-gui` in the GitHub Release zip?

Yes. The Windows zip and the Unix tarballs ship `pipe-gui` next to
`pipe`. From the unpacked folder:

```text
pipe --gui
```

`pipe --gui` looks for `pipe-gui` in the same folder as `pipe`; it does
not search PATH. If you moved only `pipe`, copy `pipe-gui` next to it.
Building from a clone is in [CONTRIBUTING.md](../../CONTRIBUTING.md).

### Can I use this instead of `pipe` on the command line?

The specification language is the same. Every walkthrough in this
guide is a legal `pipe "…"` pipeline. The window is how you *watch*.
Automation, harvest tests, and one-liners stay on `pipe`.

### Why Rust 1.85 for the GUI and 1.75 for `pipe`?

`egui` needs a current compiler. The CLI and core crates still declare
1.75. Contributors: see [CONTRIBUTING.md](../../CONTRIBUTING.md).

## Troubleshooting: the window does not open

### Windows

On some Windows laptops `pipe --gui` failed at glow init with
“OpenGL 2.0 is not available” when launched at the machine, while the
same binary opened over Remote Desktop. Device Manager can still show
a healthy NVIDIA adapter (for example Quadro M2000M) and Microsoft
Basic Display Adapter. That is a hybrid-graphics / WGL OpenGL
problem.

The default is now wgpu, which on Windows prefers Direct3D 12, then
Vulkan, then GL. If wgpu cannot start, the same process tries glow
and prints why. If both fail, the console names each cause and how to
override.

What to try, once sitting at the laptop and once over Remote Desktop:

```bat
pipe --gui
pipe --gui --renderer wgpu
pipe --gui --renderer glow
set PIPE_GUI_RENDERER=glow
pipe --gui
```

`--renderer wgpu` stays on wgpu, so it is the check that
Direct3D/Vulkan alone can open the window. `--renderer glow` is the
OpenGL path. The flag wins over `PIPE_GUI_RENDERER`.
`pipe --gui --renderer glow` is forwarded to `pipe-gui`; it is a
launch flag.

`pipe.exe` prints `PIPE: starting …` before it waits for the window.
If the window fails to appear, that same console prints the GUI exit
status and a plain-language renderer error. `pipe-gui.exe` attaches
to this console so its own errors are not swallowed by the Windows
subsystem.

Quit (and the window close box) still abort the session. Helper
children still use `CREATE_NO_WINDOW`. Those behaviours are
independent of the renderer.

### Linux and macOS

You need a working display. On X11, `DISPLAY` must be set. wgpu
typically uses Vulkan; glow uses OpenGL. The same overrides apply:

```bash
pipe --gui --renderer auto
pipe --gui --renderer wgpu
pipe --gui --renderer glow
PIPE_GUI_RENDERER=glow pipe --gui
```

If both backends fail, the console names each cause. `pipe` on the
terminal still runs pipelines. Building from a clone is in
[CONTRIBUTING.md](../../CONTRIBUTING.md).

### The window opened, then the graph is blank

Paste a specification or **Open** a `.pipe` file. An empty untitled
window is supposed to look like the
[empty screenshot](../gui.md#the-one-idea): a grid and a sentence
asking you to paste.

If Source has text and the graph is still empty, look at **Console and
messages** for a parser error. A broken specification cannot be drawn.

### Run does nothing I can see

Check the status word: Failed will say why. Unknown verbs fail at Run.
A pipeline of HOLE sinks is silent on purpose — watch the in-counts
instead of the console. CONSOLE output is in **Console and messages**.

If you expected files to appear, confirm you pressed **Run** and that
**working directory** is the clone root so `examples/marina/out/…`
resolves.

### Python / REXX stage fails

- Python: 3.10+, found as `PIPE_PYTHON`, then `py -3` (Windows), then
  `python3`, then `python`.
- REXX on Windows: bundled Regina in the release zip next to `pipe`.
  The GUI preview still needs Regina on PATH or in the usual install
  locations when you are not using that zip.
- REXX on Linux/macOS: install Regina for full behaviour, or use the
  embedded subset.
- The verb must be the **path** (`examples/upper.py`), not
  `python examples/upper.py`.

[v0.2.0 requirements](../release-notes-v0.2.0.md#requirements) lists
install commands.

### Still stuck

1. Run the same specification with `pipe` on the terminal. If `pipe`
   fails, the GUI will fail the same way; fix the pipeline first.
2. Read **Console and messages** from the top. Session ready, unknown
   stage, missing file, and renderer errors all land there.
3. Read the [reference](reference.md) for the control you are using.
4. Read [STATUS](../STATUS.md) if the verb might be 📅 planned or ⛔
   not planned.

The window watches pipelines this port can already run.
