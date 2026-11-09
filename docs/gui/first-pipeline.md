# A first pipeline

This chapter is a slow walk through one small pipeline in the native
debugger. You will paste three fruit names, filter them with LOCATE,
press **Step one record** once so you can see the session come alive,
then press **Run** and watch two records go through while one is
discarded.

By the end you will recognize the in/out badges, the green checks, the
zzz vs the hourglass, the discard X, the hover spreadsheet, and the
Records pane. You will also know the one sentence that saves a lot of
confusion: **looking at this picture never ran the pipeline.**

If you have not built `pipe-gui` yet, start from
[The native debugger](../gui.md#launch). Come back here with the window
open.

## What we will run

On Windows, from the clone root (the folder that holds `pipe.exe` after
a release build):

```powershell
.\target\release\pipe.exe --gui
```

On Linux and macOS:

```bash
./target/release/pipe --gui
```

The window opens on an untitled buffer. Source on the left is empty.
The graph says *Paste a specification or open a file. The graph is drawn
from source without running it.* Status is **Idle**. Records says *No
live session*.

Paste this specification into Source. The whole pipeline is one line;
the pane wraps so you can still read it.

```text
literal apple|literal banana|literal apricot|locate /ap/|console
```

On Windows, if you would rather launch already filled in:

```powershell
.\target\release\pipe.exe --gui
```

then paste. Or, from the command line, the same text is a `pipe`
pipeline you already know:

```powershell
.\target\release\pipe.exe "literal apple|literal banana|literal apricot|locate /ap/|console"
```

```bash
./target/release/pipe 'literal apple|literal banana|literal apricot|locate /ap/|console'
```

The GUI is the same pipeline, watched. We are not changing the
language.

## The graph appears, and still nothing has run

As soon as the text parses, five boxes appear, left to right:

LITERAL (`literal apple`) → LITERAL (`literal banana`) → LITERAL
(`literal apricot`) → LOCATE (`locate /ap/`) → CONSOLE.

Each connection is a right-angle line in the gutter between columns.
A small zzz sits on every live edge. Status is still **Idle**. Records
still says *No session yet. Step or Run to start.*

![Five stage boxes for three LITERALs, LOCATE, and CONSOLE. No in/out counts. Status Idle. Records: no live session.](images/locate-idle.png)

*The specification is drawn. The console notes that the specification
was loaded from the command line and was **not executed**. Captured on
Linux.*

Notice that:

- Source is labelled **authoritative**. The boxes are a view of that
  text, not a second copy you have to keep in sync.
- The path reads `untitled buffer  • unsaved`, because this text is not
  a file on disk yet. Save when you want to; you do not have to, to
  press Run.
- **Right-angle edges** is on. Parallel connections would get a few
  pixels of lane offset so they do not sit on top of each other.
- The Marks legend is already visible along the bottom. You do not have
  to run anything to see what the shapes mean.

> **Why looking is free.** Drawing the graph walks the specification
> the same way `pipe` would parse it. It does not create a debug
> session, does not start Python or Regina, does not open files named
> by `<` or `>`, and does not call COMMAND. A COMMAND stage in the
> picture is a box with a name, nothing more, until you Step or Run.
> That is deliberate: you can open a marina pipeline that writes three
> CSV files, look at the topology, and close the window without having
> written anything.

## Step once: the session prepares, then one record moves

Press **Step one record** (or F10).

Two things happen, in order, and both are visible in **Console and
messages**:

1. `Session ready. Idle: nothing has run yet.`
2. A line naming the stage that just moved, for example
   `LITERAL: produced a record`.

Status becomes **Paused**. The first LITERAL box gets a green check and
`1 →`. An amber hourglass sits on the connection into the second
LITERAL. The remaining boxes still show zzz. Records, with nothing
selected, lists each stage and says **Waiting for LITERAL to read**.

![After one Step: first LITERAL done with 1 out, hourglass on the next connection, other stages idle, status Paused.](images/locate-step.png)

*One record cycle after the session was prepared. The hourglass is the
live pipe: a record is sitting there for the next LITERAL to read.
Captured on Linux.*

> **Why Step prepares first.** A debug session is a real pipeline on a
> worker thread, with the same stages `pipe` would run. Until that
> session exists, there is nothing to step *in*. The first click of
> **Step one record** starts the session and then takes one record
> cycle: the selected stage if it can still run, otherwise a stage that
> has a record waiting. It never keeps targeting an already-ended
> stage. If nothing can move, the console names the stage that is
> waiting (for example **Waiting for tuck.py to read**).
>
> That is why the console says the session is ready and idle *before*
> it names a stage. The prepare is not a wasted click. It is the
> moment the picture becomes a live debugger instead of a drawing.

A record cycle is one stage doing one unit of work: producing a
LITERAL, reading and maybe writing, discarding, or noticing that input
has ended. It is not “one record all the way to CONSOLE.” Press Step a
few more times if you want to walk apple, banana, and apricot by hand.
Or press **Run** and let the rest drain, which is what we do next.

Pause and Stop stay usable the whole time. Pause takes effect at the
next stage read, write, or short.

## Run: the filter does its job

Press **Run** (or F5, or Ctrl+Enter / ⌘+Enter).

The remaining stages run to completion. Status becomes **Ended rc 0**.
Every box has a green check. CONSOLE’s pane shows the records it
printed. Records, with nothing selected, lists each stage with its
marks and says **All stages finished.** **N steps taken** counts
**Step one record** clicks, not the cycles Run performed, so a session
you mostly Run can still read **0 steps taken** (or **1**, if you
Stepped once first).

![After Run: every stage has a green check. LOCATE shows 3 in, 2 out, and 1 discarded. CONSOLE shows apricot and apple.](images/locate-run.png)

*The run summary with nothing selected. LOCATE is the interesting box:
it read three, sent two, discarded one. Captured on Linux.*

Read the badges on LOCATE, left to right:

- `→ 3` on the upper left: three records flowed **in**.
- `2 →` on the upper right: two records flowed **out**.
- `✕ 1` next to the out count: one record was discarded instead of
  sent.

That is `/ap/` against `apple`, `banana`, and `apricot`. Banana does
not contain `ap`, so LOCATE discards it. Apple and apricot go to
CONSOLE.

Notice that apricot is listed in the console before apple. Chained
LITERAL stages emit right-to-left: each writes its own argument, then
copies whatever arrived from the left. That is CMS Pipelines (Author’s
Edition 1.1.11, around the LITERAL pages), not a GUI quirk. The
debugger is showing you the order the stages actually wrote.

> **Why a finished stage is a check, not a thumbs-up.** The check means
> “this stage has ended and will do no more work.” It does not mean
> “this record was good.” Banana’s discard is a separate X on LOCATE.
> A stage can finish after discarding everything; the check still
> appears. A stage can also sit with zzz for a long time, not finished,
> because nothing has arrived yet. zzz is waiting for input. The check
> is done.

Hover the in-count or the out-count for a plain-language tooltip
(“records flowing into this stage”, “records this stage has sent”).
Multi-input stages name each source.

## Click LOCATE: the same grid, pinned

Click the LOCATE box. Source highlights `locate /ap/`. A compact card
opens against the box, labelled **Pinned** with a **Close** button. The
Records pane below shows the same headings and the same rows.

![LOCATE selected: hover card and Records pane both show Received (apricot, banana, apple), Sent (apricot, apple), and Discarded (banana).](images/locate-locate.png)

*Hover and Records share one grid. There is no separate table / text /
hex switcher: the window picks a layout from the bytes. Plain text is
one column named **value**. Captured on Linux.*

Read the card from the top:

- Title **LOCATE**, detail `locate /ap/`.
- Chips: `2 sent`, `1 discarded`.
- **This stage has finished.** with a green check, and *No more input*.
- **→ Received**: a spreadsheet. Column `#` is the row number. Column
  **value** is the record. Row 1 apricot, row 2 banana, row 3 apple —
  the order LOCATE actually read them.
- **Sent →**: apricot and apple, the records that went to CONSOLE.
- **✕ Discarded**: banana.

The headings put the mark on the side the records flowed: the in-arrow
before **Received**, the out-arrow after **Sent**, the X before
**Discarded**.

Click **Close**, press Esc, or click empty canvas to dismiss the card.
Clicking the box (or the card) pins it so it does not disappear when
the pointer leaves. That pin is for Remote Desktop jitter as much as
for reading: a popup that vanishes the moment the mouse wavers is
unusable over RDP.

Hover without clicking also opens the card, after a short delay. It
stays while the pointer is on the stage, on the popup, on the bridge
between them, or on the scroll bars, and it keeps a short grace so a
shaky connection does not dismiss it. Hover does not run anything and
does not open files.

The Records pane is the same data, not a second log. Hide it with
**Hide** if you want the graph taller; **Show** brings it back.

If a stage has sent thousands of records, the sheet keeps the last
1000 and the heading says so: **Sent (showing last 1000 of 2500)**.
Row numbers keep the original sequence, so row 1501 is still record
1501.

## Click a connection: only that edge

Click the first connector, between `literal apple` and `literal banana`.
The card title becomes **LITERAL → LITERAL**. You see only what was
sent on that connection — here, `apple` — not the writer’s input, and
not later fruits.

![The first connection selected: Sent on that edge is apple only.](images/locate-edge0.png)

*A connection is not “the writer’s whole life.” It is one stream from
one writer to one reader. Captured on Linux.*

That matters more once a stage has two outputs. The
[multi-stream walkthrough](walkthroughs.md#secondary-streams-so-lanes-show)
connects LOCATE’s secondary output; clicking that purple lane shows
banana and only banana.

## What you just learned, in one pass

| You saw | What it means |
|---|---|
| Graph appears, status Idle | Parsed, not run. |
| `Session ready. Idle: nothing has run yet.` | Step started the debug session. |
| Hourglass on a line | A record is waiting to be read. |
| zzz on a box | Alive, nothing on input, not finished. |
| `→ 3` / `2 →` / `✕ 1` | Read 3, sent 2, discarded 1. |
| Green check | That stage has ended. |
| **→ Received** / **Sent →** / **✕ Discarded** | The same records as a spreadsheet. |
| Column **value** | These records are plain text, not JSON objects. |
| CONSOLE pane | CONSOLE stdout is captured. Stdin is EOF. |

The Marks legend at the bottom is the same list. **Show words** labels
each icon; **Hide words** leaves the shapes.

## Save, or don’t

This buffer is unsaved. **Save** writes it as a `.pipe` or `.txt` file.
**Quit** or the window close box will ask **Save this buffer before
closing?** with Save / Discard / Cancel.

![Unsaved changes: Save this buffer before closing? Save, Discard, Cancel.](images/unsaved.png)

*Discard really discards; it does not reopen the prompt. Cancel leaves
you in the window. Captured on Linux.*

Saving is about the specification text, not about the run. The debug
session is already over (Ended rc 0). Host files the pipeline wrote
(none, in this example) stay written even if you Discard the buffer.

## Try the same pipeline from `pipe`, then come back

The GUI did not invent a different LOCATE. From the clone root:

```powershell
.\target\release\pipe.exe "literal apple|literal banana|literal apricot|locate /ap/|console"
```

```bash
./target/release/pipe 'literal apple|literal banana|literal apricot|locate /ap/|console'
```

You should see apricot and apple on the terminal, in the same order the
CONSOLE pane showed. The debugger added marks and a spreadsheet; it did
not change the pipeline.

## Where to go next

- [Secondary streams](walkthroughs.md#secondary-streams-so-lanes-show),
  so banana is not discarded: it takes the other lane.
- [SORT buffering](walkthroughs.md#sort-holds-records-until-input-ends),
  so you can see the violet stack and the sentence “SORT waits for all
  input before sending.”
- [A Python or REXX stage](walkthroughs.md#python-and-rexx-script-info-and-source-view),
  so you can read **View script file** without guessing.
- The [reference](reference.md), when you want Fit, zoom, keyboard
  shortcuts, and the renderer flags in one place.
- The [glossary](glossary.md), when a mark appears that this chapter
  did not pause on.
