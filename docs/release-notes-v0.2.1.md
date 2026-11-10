# Portable Pipelines 0.2.1

This is a GUI polish on 0.2.0. `pipe --gui` is the same window. Stage
names that do not fit end in `…`, and hover shows the full clause. The
Records pane lists Received first, then Sent. The window, taskbar, and
Windows `.exe` use the site logo. Stepping to the last record finishes
the run and shows the return code. After Finished or Stop you can start
over with **Run again**, **Step again**, or **Restart**. Hover is a
short peek; the record grids live in Records.

If you already use `pipe` on the command line, that path is unchanged.
`pipe --version` prints `pipe 0.2.1`. The download is still `pipe` and
`pipe-gui`, plus examples, docs, and (on Windows) Regina.

## What this means for you

- Stage boxes end in `…` when the argument text does not fit. File
  stages (`<`, `>`, `>>`) keep the end of the path visible. Hover the
  box for the full clause, for example
  `< examples/marina/vessels.jsonl`.
- The Records pane for a selected stage shows Received first, then
  Sent. A waiting or status line stays with the section it describes.
- The window, the taskbar, and the Windows `pipe-gui.exe` icon are the
  Portable Pipelines site logo (the same mark as
  https://portable-pipelines.github.io).
- Stepping to the end of a pipeline now moves to Finished and shows
  the return code, the same as **Run**. Output files are written and
  closed. The console gets a plain line such as
  `Finished. Return code 0.`
- After Finished or Stop, **Run again**, **Step again**, and
  **Restart** start over from the top without quitting. Source and
  layout stay; marks, counts, and Records reset. The console says
  `Started again.` If **Run** is still going, **Step** pauses at the
  next record.
- Hover on a stage or connection is a short peek: name, full clause,
  status word, and in/out counts in plain words (`Read 5, wrote 5`).
  Record grids live only in the Records pane. Click a stage or
  connection to select it and see its records there. The peek says
  `Click to see records below.`

## What's new

- **Truncated stage names.** When a stage box cannot show the whole
  argument, it ends in `…`. Hover for the full clause.
- **Records order.** Received is above Sent, matching how records
  move.
- **Site logo.** Window, taskbar, and Windows `.exe` use the site
  mark.
- **Finish while stepping.** A multi-stream pipeline that you step to
  the last record now finishes (return code, closed files) instead of
  staying Paused with every stage marked done.
- **Start over.** **Run again** / **Step again** / **Restart** after
  Finished or Stop. No need to quit the window.
- **Hover peek.** A short tooltip. Spreadsheets stay in Records.
- **Release uploads.** Merging a dedicated release request now builds,
  signs the Windows programs, and uploads all sixteen files in the
  same run.

User guide (tutorial, walkthroughs, reference, glossary, FAQ):
[docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.1/docs/gui.md).

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-v0.2.1.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.1/pipe-windows-x64-v0.2.1.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-v0.2.1.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.1/pipe-linux-x86_64-v0.2.1.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-v0.2.1.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.1/pipe-macos-arm64-v0.2.1.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-v0.2.1.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.2.1/pipe-macos-x86_64-v0.2.1.tar.gz) | macOS on Intel (x86_64) |

The same bytes are also published without a version in the name:

| File | Platform |
|---|---|
| [`pipe-windows-x64.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-windows-x64.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-linux-x86_64.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-macos-arm64.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-macos-x86_64.tar.gz) | macOS on Intel (x86_64) |

Each archive has a matching `.sha256` file so you can check your download. The checksum names the file it belongs to. GitHub also records a build provenance attestation for each archive. The Linux and macOS tarballs do not include Regina or Python. Install those separately if you want them.

## Windows zip, checksums, and antivirus

The Windows `pipe.exe` and `pipe-gui.exe` in this zip are
Authenticode-signed by John Lynn via Microsoft Artifact Signing. A
signed program usually means fewer antivirus and Defender scan delays.
The bundled `regina.dll` is third-party Regina and is not signed.

Signing does not mean every warning is gone. Windows SmartScreen or
Defender may still do a one-time cloud check the first time you run
`pipe.exe` or `pipe-gui.exe`. Some antivirus products still treat a
new program they have not seen before as suspicious. That does not by
itself mean the file is harmful, and it does not by itself mean the
file is safe. We do not claim any scanner result.

You can check that the zip you downloaded is the one published with
this release:

1. From the GitHub Release page, download `pipe-windows-x64-v0.2.1.zip`
   and the matching `.zip.sha256` file. `pipe-windows-x64.zip` is the
   same zip under a stable name, with its own `.sha256` file.
2. In Command Prompt, in the folder that holds both files:

   ```text
   certutil -hashfile pipe-windows-x64-v0.2.1.zip SHA256
   type pipe-windows-x64-v0.2.1.zip.sha256
   ```

   The 64-character hex digest should match. Letter case does not
   matter. The same check works for `pipe-windows-x64.zip`.
3. If you use GitHub's CLI, you can also check the build provenance
   attestation for that zip:

   ```text
   gh attestation verify pipe-windows-x64-v0.2.1.zip --repo portable-pipelines/portable-pipelines
   ```

If a vendor still flags the file after the hash matches, please report
a false positive to that vendor and include the SHA-256. You are also
welcome to open a discussion or issue here so we know which product
complained. We cannot promise a particular scanner result.

## Known limits

- The window needs a working display. Renderer overrides are
  `--renderer auto|wgpu|glow` and `PIPE_GUI_RENDERER`.

## Quick start

A small tip first: in `literal hello| console` there is no space before the `|`. Any space after the literal text becomes part of the record.

**Windows** (Command Prompt, from the unpacked folder)

```text
pipe.exe --version
pipe.exe --gui
pipe.exe --gui examples\marina\to-csv.pipe
```

**Linux and macOS** (use the file name for your platform)

```sh
tar -xzf pipe-linux-x86_64-v0.2.1.tar.gz
cd pipe-linux-x86_64
./pipe --version
./pipe --gui
./pipe --gui examples/marina/to-csv.pipe
```

`pipe-linux-x86_64.tar.gz` unpacks into the same `pipe-linux-x86_64` folder. Each tarball unpacks into a folder named after the platform, such as `pipe-macos-arm64`.

**macOS note:** macOS may refuse to open `pipe` the first time because it was downloaded from the internet. If that happens, run this once from the unpacked folder: `xattr -r -d com.apple.quarantine .` (add `sudo` in front only if it reports a permissions error). Then `./pipe` runs normally.

To check which version you have, run `pipe --version` (`pipe.exe --version` on Windows). It prints `pipe 0.2.1`.

To run `pipe` from any directory, add its folder to your `PATH`.

## Requirements

- **Python stages** need Python 3.10 or later on the machine. `pipe` looks for `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`.
- **REXX stages** on Windows use the bundled Regina. On Linux and macOS, install Regina if you want `.rexx` stages to run through real Regina; otherwise they use the embedded subset interpreter.
- **The GUI** needs a working display. wgpu uses Direct3D 12 on Windows when it can. See [docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.1/docs/gui.md).

## More

- Portable Pipelines GUI: [docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.1/docs/gui.md)
  (tutorial, walkthroughs, reference, glossary, FAQ)
- What works today: [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.1/docs/STATUS.md)
- JSON rows and CSV: [docs/json-and-fields.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.1/docs/json-and-fields.md)
- Changes: [CHANGELOG.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.1/CHANGELOG.md)
- Examples: [examples/README.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.2.1/examples/README.md)

Thank you for trying Portable Pipelines.
