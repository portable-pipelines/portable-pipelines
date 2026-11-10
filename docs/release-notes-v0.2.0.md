# Portable Pipelines 0.2.0

You can watch a pipeline run. `pipe --gui` opens a native desktop window
that draws the stages, steps one record at a time, and shows the records
as a spreadsheet. It is not a browser and not a web page. JSON can be a
stream of objects, not only one array. The Cedar Cove marina examples
turn four CSV tables into nested documents and back. The download
includes `pipe-gui` next to `pipe`, so `pipe --gui` works from the
unpacked folder. Portable Pipelines remains a small portable `pipe` command,
written in tribute to John Hartmann, who created CMS Pipelines. Records
still move the way John designed: a stage never calls the next one; it
only reads records and writes records, and a coroutine dispatcher moves
them along.

## What this means for you

If you already use `pipe` on the command line, that path is unchanged.
`pipe --version` prints `pipe 0.2.0`. The download is `pipe` and
`pipe-gui`, plus examples, docs, and (on Windows) Regina.

If you want to *see* a pipeline, run `pipe --gui` from the unpacked
folder (`.\pipe.exe --gui` on Windows). Open a `.pipe` file, press
**Run** or **Step**, and watch records move. Nothing runs until you
press one of those. Quit stops the session; it does not finish a paused
pipeline for you.

Windows: if the window failed to open at the laptop with an OpenGL
error, but worked over Remote Desktop, this build tries Direct3D first.
You can force a renderer with `--renderer wgpu` or `--renderer glow`.

JSON files that are many objects in a row (pretty-printed or jammed
together) can be read with `json objects` so a later `take 10` does not
have to parse the rest of the file. The marina folder is a worked
example of CSV in, nested JSON out, and the reverse.

The public `docs/` tree is in the zip and tarballs now, next to
`examples/`. The website (portable-pipelines.github.io) still needs a
separate update for this version; that repo is not part of this release.

## What's new

- **Native GUI debugger.** `pipe --gui` launches sibling `pipe-gui`.
  Opening a file and drawing the graph do not execute. **Run** starts a
  live debug session; **Step** takes one record cycle. Stage boxes show
  in and out counts; marks are drawn shapes (hourglass, zzz, arrow, X,
  stack, check). Hover a stage for a compact spreadsheet of received and
  sent records; the Records pane is the same grid, pinned. Orthogonal
  edges stay in the column gutter. Quit and the close box abort; they
  do not run to completion. Helper children on Windows use
  `CREATE_NO_WINDOW` so a cmd window does not flash. Docs:
  [docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/docs/gui.md).
- **Windows graphics.** The window uses wgpu first (Direct3D 12,
  Vulkan, Metal, or GL) and falls back to OpenGL (glow) if wgpu cannot
  start. `--renderer auto|wgpu|glow` and `PIPE_GUI_RENDERER` override.
  Forced backends do not fall back. A failed launch names the cause in
  the `pipe --gui` console.
- **The GUI ships in the download.** `pipe --gui` looks for `pipe-gui`
  in the same folder as `pipe`. From the unpacked archive:

  ```text
  pipe --gui
  pipe --gui examples/marina/to-csv.pipe
  ```
- **`json objects`.** Reads consecutive JSON objects and writes each
  complete object as one record as soon as it closes. Pretty-printed
  across records, adjacent in one record, or one object per line.
  Downstream `take 10` can stop without parsing the rest. Top-level
  arrays still use `json array`. MongoDB shell constructors such as
  `ObjectId(...)` are not converted. Example
  [examples/objects.json](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/examples/objects.json).
- **Cedar Cove marina.** Four CSV tables (vessels, owners, berths,
  work orders) to nested JSON documents and back, using LOOKUP, COLLATE,
  `json split` / `json collect`, and small Python stages. Folder:
  [examples/marina/](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/examples/marina/README.md).
- **Docs in the download.** The public `docs/` folder is in the Windows
  zip and the Unix tarballs, next to `examples/`. Local IBM PDFs under
  `docs/reference/` stay out.
- **Chat-driven releases.** After review, merging a dedicated
  release-intent pull request is what publishes. Ordinary work does not.
  [docs/releasing.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/docs/releasing.md).
- **128 of the 254 built-in programs** in the CMS/TSO Pipelines
  Author's Edition 1.1.11 catalog run today, some with documented
  limits. JSON and CSV sit beside that catalog. REXX pipeline commands
  are **23 complete** and **3 limited**.
  [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/docs/STATUS.md)
  lists every stage and where it stands.
- **Stable download names** are unchanged. Each archive is uploaded
  under its versioned name and again under a name with no version.

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-v0.2.0.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.0/pipe-windows-x64-v0.2.0.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-v0.2.0.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.0/pipe-linux-x86_64-v0.2.0.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-v0.2.0.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.0/pipe-macos-arm64-v0.2.0.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-v0.2.0.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.0/pipe-macos-x86_64-v0.2.0.tar.gz) | macOS on Intel (x86_64) |

The same bytes are also published without a version in the name:

| File | Platform |
|---|---|
| [`pipe-windows-x64.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-windows-x64.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-linux-x86_64.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-macos-arm64.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-macos-x86_64.tar.gz) | macOS on Intel (x86_64) |

Each archive has a matching `.sha256` file so you can check your download. The checksum names the file it belongs to. GitHub also records a build provenance attestation for each archive. The Linux and macOS tarballs do not include Regina or Python. Install those separately if you want them.

## Windows zip, checksums, and antivirus

The Windows `pipe.exe` in this zip is Authenticode-signed by John Lynn
via Microsoft Artifact Signing. A signed `pipe.exe` usually means fewer
antivirus and Defender scan delays. The bundled `regina.dll` is
third-party Regina and is not signed.

Signing does not mean every warning is gone. Windows SmartScreen or
Defender may still do a one-time cloud check the first time you run
`pipe.exe`. Some antivirus products still treat a new program they have
not seen before as suspicious. That does not by itself mean the file is
harmful, and it does not by itself mean the file is safe. We do not
claim any scanner result.

You can check that the zip you downloaded is the one published with
this release:

1. From the GitHub Release page, download `pipe-windows-x64-v0.2.0.zip`
   and the matching `.zip.sha256` file. `pipe-windows-x64.zip` is the
   same zip under a stable name, with its own `.sha256` file.
2. In PowerShell 5.1 or later, in the folder that holds both files:

   ```text
   Get-FileHash .\pipe-windows-x64-v0.2.0.zip -Algorithm SHA256
   Get-Content .\pipe-windows-x64-v0.2.0.zip.sha256
   ```

   The 64-character hex digest should match. Letter case does not
   matter. The same check works for `pipe-windows-x64.zip`.
3. If you use GitHub's CLI, you can also check the build provenance
   attestation for that zip:

   ```text
   gh attestation verify .\pipe-windows-x64-v0.2.0.zip --repo portable-pipelines/portable-pipelines
   ```

If you would rather not run a pre-built binary, clone the repository
and build from source with `cargo build --release`. See
[CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/CONTRIBUTING.md).

If a vendor still flags the file after the hash matches, please report
a false positive to that vendor and include the SHA-256. You are also
welcome to open a discussion or issue here so we know which product
complained. We cannot promise a particular scanner result.

## Known limits

- CRC has not been checked against an IBM CRC16I result.
- FILERAND indexes the file in memory and rejects BLOCKED.
- `json array` holds the whole document in memory.
- FILEFAST and FILESLOW do not implement CMS VARIABLE or FIXED formats.
- FILEUPDATE keeps later records when a replacement has a different
  length.
- The window needs a working display. Renderer overrides are
  `--renderer auto|wgpu|glow` and `PIPE_GUI_RENDERER`.

## Quick start

A small tip first: in `literal hello| console` there is no space before the `|`. Any space after the literal text becomes part of the record.

**Windows**

1. Extract the zip into a folder of its own, and open a terminal in that folder.
2. Run:

   ```text
   .\pipe.exe "literal hello| console"
   ```

   In Command Prompt you can leave off the `.\` and type `pipe.exe "literal hello| console"`.

   To watch a pipeline:

   ```text
   .\pipe.exe --gui
   .\pipe.exe --gui examples\marina\to-csv.pipe
   ```
3. Take the tour:

   ```text
   powershell -ExecutionPolicy Bypass -File .\examples\run-examples.ps1
   ```

**Linux and macOS** (use the file name for your platform)

```sh
tar -xzf pipe-linux-x86_64-v0.2.0.tar.gz
cd pipe-linux-x86_64
./pipe "literal hello| console"
./pipe --gui
./pipe --gui examples/marina/to-csv.pipe
sh ./examples/run-examples.sh
```

`pipe-linux-x86_64.tar.gz` unpacks into the same `pipe-linux-x86_64` folder. Each tarball unpacks into a folder named after the platform, such as `pipe-macos-arm64`.

**macOS note:** macOS may refuse to open `pipe` the first time because it was downloaded from the internet. If that happens, run this once from the unpacked folder: `xattr -r -d com.apple.quarantine .` (add `sudo` in front only if it reports a permissions error). Then `./pipe` runs normally.

To check which version you have, run `pipe --version` (`.\pipe.exe --version` on Windows). It prints `pipe 0.2.0`.

To run `pipe` from any directory, add its folder to your `PATH`.

One JSON example, from the unpacked folder:

```text
pipe "< examples/orders.csv | csv | json add $total = $qty * $amount | pick $total > /100/ | spec $customer 1 / spent / next $total next | console"
```

The marina round-trip, from a clone that has `pipe` on your PATH (or from the repo root after `cargo build --release`):

```text
sh ./examples/marina/to-nested.sh
sh ./examples/marina/to-csv.sh
```

On Windows, `powershell -ExecutionPolicy Bypass -File .\examples\marina\to-nested.ps1`.

## Requirements

- **Python stages** need Python 3.10 or later on the machine. `pipe` looks for `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y python3`
  - macOS (Homebrew): `brew install python`
- **REXX stages** on Windows use the bundled Regina. On Linux and macOS, install Regina if you want `.rexx` stages to run through real Regina; otherwise they use the embedded subset interpreter. Variable pool stages and Chapter 21 commands need Regina for full behaviour. A Regina stage streams through a helper process, including CALLPIPE (STEM and VAR too). `PIPE_REGINA_INPROC=1` still drains, then runs, then flushes.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y regina-rexx libregina3-dev`
  - macOS (Homebrew): `brew install regina-rexx`
- **The GUI** needs a current stable Rust (the GUI crate asks for 1.85) and a working graphics API. wgpu uses Direct3D 12 on Windows when it can. See [docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/docs/gui.md).

## More

- What works today, what is planned, and what is not: [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/docs/STATUS.md)
- Desktop editor and debugger: [docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/docs/gui.md)
- JSON rows and CSV: [docs/json-and-fields.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/docs/json-and-fields.md)
- Changes: [CHANGELOG.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/CHANGELOG.md)
- Examples: [examples/README.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/examples/README.md)
- Building from source and contributing: [CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.0/CONTRIBUTING.md)

Thank you for trying Portable Pipelines.
