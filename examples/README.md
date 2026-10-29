# Try these

You have `pipe`. These examples are small pipelines you can run right away.

## Guided tour

Windows, from the folder that contains `pipe.exe`:

```text
powershell -ExecutionPolicy Bypass -File .\examples\run-examples.ps1
```

The script finds `pipe.exe` next to the `examples` folder, prints each pipeline, and runs it. You can start it from any current directory as long as you pass the path to the script.

Linux and macOS, from the folder that contains `pipe`:

```text
sh ./examples/run-examples.sh
```

## Copy and paste

Windows, from the folder that contains `pipe.exe`:

```text
.\pipe.exe "literal hello| console"
.\pipe.exe "literal hello | examples\upper.rexx | console"
.\pipe.exe "< examples\lines.txt | locate /todo/ | console"
.\pipe.exe "literal hello | examples\upper.py | console"
.\pipe.exe "examples\emit.py a b c | examples\upper.py | console"
.\pipe.exe "(ENDCHAR ?) examples\emit.rexx apple banana apricot | L: locate /ap/ | console ? L: | examples\prefix.rexx miss= | console"
.\pipe.exe "(end ?) < examples\users.txt | a: strfind /USER / | spec 1-* 1 / / next | j: juxtapose | console ? a: | j:"
```

In Command Prompt you can leave off the `.\`. A space before `|` becomes part of the record (a trailing blank), so the first example has no space before `|`.

Linux and macOS, from the folder that contains `pipe`:

```text
./pipe "literal hello| console"
./pipe "literal hello | examples/upper.rexx | console"
./pipe "< examples/lines.txt | locate /todo/ | console"
./pipe "literal hello | examples/upper.py | console"
./pipe "examples/emit.py a b c | examples/upper.py | console"
./pipe "(ENDCHAR ?) examples/emit.rexx apple banana apricot | L: locate /ap/ | console ? L: | examples/prefix.rexx miss= | console"
./pipe "(end ?) < examples/users.txt | a: strfind /USER / | spec 1-* 1 / / next | j: juxtapose | console ? a: | j:"
```

A `.py` program named in the pipeline reads records and writes records, the same way a `.rexx` program does. Python stages need Python 3.10 or later on the machine. The tour skips those demos if Python is not found.

The JUXTAPOSE example shows how to add the owning USER to each MDISK line. STRFIND sends the USER lines down the main path and the MDISK lines out its second output. SPEC adds a trailing blank to each USER line. JUXTAPOSE holds each USER line and puts it in front of each MDISK line arriving on its second input, producing records like `USER AAA MDISK 191`.

## What is in this folder

- `run-examples.ps1` / `run-examples.sh`: guided tour of about a dozen pipelines
- `phase1.ps1`: TAKE, DROP, NLOCATE, and STRIP
- `multistream.ps1`: labels and secondary streams
- `emit.rexx` / `emit.py`: write each argument as one record
- `upper.rexx` / `upper.py`: uppercase each record
- `prefix.rexx`, `filter.rexx`, `fields.rexx`, `chop.rexx`, `take.rexx`, `wrap.rexx`, `tee_say.rexx`, `callpipe_demo.rexx`, `callpipe_files.rexx`, `luhn.rexx`: small REXX samples (`luhn.rexx` is an original Luhn mod-10 check that uses PEEKTO then CALLPIPE `*:`; `callpipe_files.rexx` loops over file names with nested `callpipe < file | ... | *:`)
- `gen-callpipe-files.ps1` / `.cmd` / `.sh`: write a temp file set for the one-line SPEC plus PIPCMD demo; `run-callpipe-files.ps1` / `.sh` is an optional longer runner
- `photo_workflow.rexx`: load photo names into a stem, LOOKUP leftovers, print commands (needs Regina)
- `varpool_demo.rexx`: STEM, VAR, VARSET, VARFETCH, REXXVARS, VARLOAD, VARDROP (needs Regina)
- `wordfreq.py`: count common words
- `webget.py`: fetch a URL (needs a network)
- `printtest.py`: `print()` goes to the terminal; only `output()` writes records
- `lines.txt`: sample input
- `users.txt`: sample USER and MDISK lines for the JUXTAPOSE example
- `update-*.txt`: sample files for the UPDATE stage
- `showcase/`: ASA print report to JSON Lines

## Streaming CALLPIPE with many files

Make a folder of small text files, then run one `pipe` command. The generator writes the files and a list of their paths (GETFILES reads file contents, it does not glob a directory). The pipeline reads that list, and SPEC builds one command per path: `callpipe < thatfile | xlate 1-* upper | *:`. Write a bar inside the SPEC text as two bars (`||`) so it is not a pipeline separator. PIPCMD runs each nested CALLPIPE as records arrive, so the first uppercase lines print before the last file is opened. Add `pipcmd stop anyeof | take 10` at the end to keep ten lines and leave the rest unread. Expected output is `FILE=0001 LINE=0001` and so on, all uppercase: 200 lines for the 50-file set, or exactly ten lines with `take 10`.

Generate the files once (from the folder that contains `pipe.exe` / `pipe`):

```text
examples\gen-callpipe-files.cmd -Files 50 -Lines 4 -Width 48
```

```text
powershell -ExecutionPolicy Bypass -File .\examples\gen-callpipe-files.ps1 -Files 50 -Lines 4 -Width 48
```

```text
sh ./examples/gen-callpipe-files.sh --files 50 --lines 4 --width 48
```

Then this is the whole demo. Command Prompt:

```text
pipe.exe "< %TEMP%\pipe-callpipe-files\paths.txt | spec /callpipe < / 1 1-* next / || xlate 1-* upper || *:/ next | pipcmd | console"
```

PowerShell 5.1 (the `$env:TEMP` bit is the only PowerShell in this line):

```text
.\pipe.exe "< $env:TEMP\pipe-callpipe-files\paths.txt | spec /callpipe < / 1 1-* next / || xlate 1-* upper || *:/ next | pipcmd | console"
```

Linux and macOS:

```text
./pipe "< /tmp/pipe-callpipe-files/paths.txt | spec /callpipe < / 1 1-* next / || xlate 1-* upper || *:/ next | pipcmd | console"
```

Same pipeline, stop after ten lines. Command Prompt:

```text
pipe.exe "< %TEMP%\pipe-callpipe-files\paths.txt | spec /callpipe < / 1 1-* next / || xlate 1-* upper || *:/ next | pipcmd stop anyeof | take 10 | console"
```

PowerShell 5.1:

```text
.\pipe.exe "< $env:TEMP\pipe-callpipe-files\paths.txt | spec /callpipe < / 1 1-* next / || xlate 1-* upper || *:/ next | pipcmd stop anyeof | take 10 | console"
```

Linux and macOS:

```text
./pipe "< /tmp/pipe-callpipe-files/paths.txt | spec /callpipe < / 1 1-* next / || xlate 1-* upper || *:/ next | pipcmd stop anyeof | take 10 | console"
```

`take 10` should print:

```text
FILE=0001 LINE=0001 XXXX...
FILE=0001 LINE=0002 XXXX...
FILE=0001 LINE=0003 XXXX...
FILE=0001 LINE=0004 XXXX...
FILE=0002 LINE=0001 XXXX...
FILE=0002 LINE=0002 XXXX...
FILE=0002 LINE=0003 XXXX...
FILE=0002 LINE=0004 XXXX...
FILE=0003 LINE=0001 XXXX...
FILE=0003 LINE=0002 XXXX...
```

and then exit. That is streaming. If it paused until every file was read, that would be buffering.

The rest of this section is extra: which REXX engine actually runs, how to watch memory, the REXX stage, and a longer `run-callpipe-files` script if you want the three demos in one go.

### Which REXX interpreter actually runs a `.rexx` stage

There is no supported switch to force the embedded subset interpreter (`PIPE_FORCE_SUBSET` is not a thing). Discovery is: use Regina when a library is found, otherwise the subset.

| How you run `pipe` | Engine for a top-level `.rexx` stage | Does this demo's CALLPIPE stream? |
|---|---|---|
| Windows zip (`regina.dll` next to `pipe.exe`) | Regina | No. Regina drains every path, runs the program, then flushes buffered output. |
| Linux or macOS with `libregina` installed and discoverable | Regina | No. Same drain, run, flush. Nested CALLPIPE from that host is the buffered path. |
| Linux or macOS with no Regina | Subset interpreter | Yes. `READTO` / `PEEKTO` / `CALLPIPE` / `OUTPUT` talk to a live `StageCtx`. |
| Nested `.rexx` under an already running Regina `RexxStart` | Subset (Regina 3.9 cannot re-enter) | Yes, but that is not a supported way to opt out of Regina for this demo. |

PIPCMD is the cleaner streaming path on every host, including the Windows zip. It issues `CALLPIPE` on a live `StageCtx`, so records move one at a time even when Regina is sitting next to `pipe.exe` unused.

Streaming looks like this: `FILE=0001` lines print while later files are still unread, RSS stays near a few records, and `take 10` returns after ten lines. Buffering looks like this: nothing prints until every file has been read, RSS climbs toward the size of the file set, and `take 10` still reads everything first (then prints ten lines).

`--trace` on a live caller shows `CALLPIPE enter` / `WRITE` / `CALLPIPE exit` interleaved with later files. A Regina stage traces a drain, then a burst of writes at the end.

### Extra: generator flags, memory, REXX, and a longer script

Parameters are number of files, lines per file, and line width. The script prints `OUTDIR=`, `PATHS=`, and `COMMANDS=`. Default output is `%TEMP%\pipe-callpipe-files` on Windows and `/tmp/pipe-callpipe-files` on Unix. To try a directory of your own, pass `-OutDir` / `--outdir` (keep it outside git, or under the gitignored `examples/callpipe-data/`). Each record looks like `file=0001 line=0001` plus `x` padding.

If you want the three demos (small files, large files, take 10) without pasting the SPEC line, `run-callpipe-files.ps1` / `run-callpipe-files.sh` still do that. Add `-Way rexx` / `--way rexx` for the REXX stage, or `-Demo take` / `--demo take` for early-stop only.

### (a) Many small files

Same 50-file set, using the generator's `commands.txt` (SPEC already applied). Windows:

```text
.\pipe.exe "< %TEMP%\pipe-callpipe-files\commands.txt | pipcmd | console"
```

In Command Prompt, `%TEMP%` expands. In PowerShell use `$env:TEMP` in double quotes:

```text
.\pipe.exe "< $env:TEMP\pipe-callpipe-files\commands.txt | pipcmd | console"
```

Linux and macOS:

```text
./pipe "< /tmp/pipe-callpipe-files/commands.txt | pipcmd | console"
```

Expected: 200 records, `FILE=0001 LINE=0001` first, `FILE=0050 LINE=0004` last. First output is the first line of the first file, not a pause until file 50.

The REXX stage (streams only on the subset interpreter):

```text
.\pipe.exe "< $env:TEMP\pipe-callpipe-files\paths.txt | examples\callpipe_files.rexx | console"
./pipe "< /tmp/pipe-callpipe-files/paths.txt | examples/callpipe_files.rexx | console"
```

### (b) A few large files, watch memory

Generate something big enough to see in a memory display, small enough to keep on disk:

```text
examples\gen-callpipe-files.cmd -Files 3 -Lines 20000 -Width 120
sh ./examples/gen-callpipe-files.sh --files 3 --lines 20000 --width 120
```

`BYTES_HINT` is about 7 MB. Then the same PIPCMD pipeline as (a).

Windows: Task Manager, Details, `pipe.exe`, Memory (Private Working Set). It should stay far under the file-set size while records print.

Linux: GNU `time` if you have it (`sudo apt-get install time` on Debian/Ubuntu), then:

```text
/usr/bin/time -v ./pipe "< /tmp/pipe-callpipe-files/commands.txt | pipcmd | console"
```

Look at Maximum resident set size. Without GNU time, start the pipeline and in another terminal sample RSS (`ps -o rss= -p PID`, or `grep VmRSS /proc/PID/status`). macOS: Activity Monitor, or `time -l` on the `pipe` command (the `maximum resident set size` line).

Streaming: RSS stays near the process baseline plus a few records. Buffering: RSS climbs by megabytes toward `BYTES_HINT` (and a second copy on the way out) before the first line prints.

### (c) `take 10` stops early

PIPCMD, same 50-file set as (a). `stop anyeof` makes PIPCMD halt when TAKE closes the output, so later command records (and later files) are never issued:

```text
.\pipe.exe "< $env:TEMP\pipe-callpipe-files\commands.txt | pipcmd stop anyeof | take 10 | console"
./pipe "< /tmp/pipe-callpipe-files/commands.txt | pipcmd stop anyeof | take 10 | console"
```

Expected: exactly ten records (file 1's four lines, file 2's four lines, then the first two lines of file 3):

```text
FILE=0001 LINE=0001 XXXX...
FILE=0001 LINE=0002 XXXX...
FILE=0001 LINE=0003 XXXX...
FILE=0001 LINE=0004 XXXX...
FILE=0002 LINE=0001 XXXX...
FILE=0002 LINE=0002 XXXX...
FILE=0002 LINE=0003 XXXX...
FILE=0002 LINE=0004 XXXX...
FILE=0003 LINE=0001 XXXX...
FILE=0003 LINE=0002 XXXX...
```

and then the pipeline ends. That is streaming: TAKE 10 closed the stream after ten lines, and files 4 through 50 were not opened.

REXX plus TAKE (subset streams; Regina still reads every path first):

```text
.\pipe.exe "< $env:TEMP\pipe-callpipe-files\paths.txt | examples\callpipe_files.rexx | take 10 | console"
./pipe "< /tmp/pipe-callpipe-files/paths.txt | examples/callpipe_files.rexx | take 10 | console"
```

You can make the contrast loud by generating the large set from (b) and appending `| take 10`. PIPCMD returns almost immediately with ten lines. A Regina `.rexx` stage still reads all three large files before those ten lines appear.

## More

The long developer catalog (every harvested `pipe` line and expected output) lives in the git repo at `docs/examples-reference.md`.

https://github.com/portable-pipelines/portable-pipelines
