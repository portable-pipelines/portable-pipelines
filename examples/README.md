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

The JUXTAPOSE example shows how to add the owning USER to each MDISK line. STRFIND splits the USER lines from the rest and sends them to the secondary input. SPEC adds a trailing blank to each record. JUXTAPOSE holds each USER line and prefixes it to each following MDISK line from its second input, producing records like `USER AAA MDISK 191`.

## What is in this folder

- `run-examples.ps1` / `run-examples.sh`: guided tour of about a dozen pipelines
- `phase1.ps1`: TAKE, DROP, NLOCATE, and STRIP
- `multistream.ps1`: labels and secondary streams
- `emit.rexx` / `emit.py`: write each argument as one record
- `upper.rexx` / `upper.py`: uppercase each record
- `prefix.rexx`, `filter.rexx`, `fields.rexx`, `chop.rexx`, `take.rexx`, `wrap.rexx`, `tee_say.rexx`, `callpipe_demo.rexx`: small REXX samples
- `wordfreq.py`: count common words
- `webget.py`: fetch a URL (needs a network)
- `printtest.py`: `print()` goes to the terminal; only `output()` writes records
- `lines.txt`: sample input
- `users.txt`: sample USER and MDISK lines for the JUXTAPOSE example
- `update-*.txt`: sample files for the UPDATE stage
- `showcase/`: ASA print report to JSON Lines

## More

The long developer catalog (every harvested `pipe` line and expected output) lives in the git repo at `docs/examples-reference.md`.

https://github.com/portable-pipelines/portable-pipelines
