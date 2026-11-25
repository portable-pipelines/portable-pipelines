# Portable Pipelines 0.3.4

This is a GUI follow-up on 0.3.3. With Show records set to On the
graph, the records popup next to a stage sizes to its tables. It stays
on the graph canvas when that canvas is wide enough, and otherwise
keeps a readable width over the neighbouring pane so status lines are
not clipped. `pipe --gui` is the same window. Command-line `pipe` is
unchanged.

If you already use `pipe` on the command line, that path is unchanged.
`pipe --version` prints `pipe 0.3.4`. The download is still `pipe` and
`pipe-gui`, plus examples, docs, and (on Windows) Regina.

## What this means for you

- Set **Show records** to **On the graph**, then click a stage. The
  records popup starts compact and grows with the tables instead of
  opening as a large empty card that covered Properties and Console.
- When the graph canvas is wide enough, the popup stays on the canvas
  beside the stage.
- When the canvas is narrower than about 260 pixels, the popup keeps
  that floor width, or the tables’ natural width if smaller. It may
  cover Palette or Properties. It stays inside the window and sits on
  the side with more room.
- Status lines wrap instead of being cut off. Overflow in the grids
  still scrolls inside the popup.

User guide (tutorial, walkthroughs, reference, glossary, FAQ):
[docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.3.4/docs/gui.md).

## What's new

- The on-graph records popup starts compact and grows with the tables.
- The popup stays on the graph canvas when that canvas can hold it.
- On a narrow canvas it keeps a readable floor width, wraps status
  lines, and may cover the neighbouring pane while staying in the
  window.

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-v0.3.4.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.3.4/pipe-windows-x64-v0.3.4.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-v0.3.4.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.3.4/pipe-linux-x86_64-v0.3.4.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-v0.3.4.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.3.4/pipe-macos-arm64-v0.3.4.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-v0.3.4.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.3.4/pipe-macos-x86_64-v0.3.4.tar.gz) | macOS on Intel (x86_64) |

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

1. From the GitHub Release page, download `pipe-windows-x64-v0.3.4.zip`
   and the matching `.zip.sha256` file. `pipe-windows-x64.zip` is the
   same zip under a stable name, with its own `.sha256` file.
2. In Command Prompt, in the folder that holds both files:

   ```text
   certutil -hashfile pipe-windows-x64-v0.3.4.zip SHA256
   type pipe-windows-x64-v0.3.4.zip.sha256
   ```

   The 64-character hex digest should match. Letter case does not
   matter. The same check works for `pipe-windows-x64.zip`.
3. If you use GitHub's CLI, you can also check the build provenance
   attestation for that zip:

   ```text
   gh attestation verify pipe-windows-x64-v0.3.4.zip --repo portable-pipelines/portable-pipelines
   ```

If a vendor still flags the file after the hash matches, please report
a false positive to that vendor and include the SHA-256. You are also
welcome to open a discussion or issue here so we know which product
complained. We cannot promise a particular scanner result.

## Known limits

- The window needs a working display. Renderer overrides are
  `--renderer auto|wgpu|glow` and `PIPE_GUI_RENDERER`.
- Copy and paste of stages is not in this release. Copy as Mermaid
  copies the diagram as text, not the stages themselves. Pipes that
  use pipe input or pipe output connectors (`*:`) are not rewritten
  from the graph yet. My REXX and Python rows and my pipes are empty
  headings until later.

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
tar -xzf pipe-linux-x86_64-v0.3.4.tar.gz
cd pipe-linux-x86_64
./pipe --version
./pipe --gui
./pipe --gui examples/marina/to-csv.pipe
```

`pipe-linux-x86_64.tar.gz` unpacks into the same `pipe-linux-x86_64` folder. Each tarball unpacks into a folder named after the platform, such as `pipe-macos-arm64`.

**macOS note:** macOS may refuse to open `pipe` the first time because it was downloaded from the internet. If that happens, run this once from the unpacked folder: `xattr -r -d com.apple.quarantine .` (add `sudo` in front only if it reports a permissions error). Then `./pipe` runs normally.

To check which version you have, run `pipe --version` (`pipe.exe --version` on Windows). It prints `pipe 0.3.4`.

To run `pipe` from any directory, add its folder to your `PATH`.

## Requirements

- **Python stages** need Python 3.10 or later on the machine. `pipe` looks for `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`.
- **REXX stages** on Windows use the bundled Regina. On Linux and macOS, install Regina if you want `.rexx` stages to run through real Regina; otherwise they use the embedded subset interpreter.
- **The GUI** needs a working display. wgpu uses Direct3D 12 on Windows when it can. See [docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.3.4/docs/gui.md).

## More

- Portable Pipelines GUI: [docs/gui.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.3.4/docs/gui.md)
  (tutorial, walkthroughs, reference, glossary, FAQ)
- What works today: [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.3.4/docs/STATUS.md)
- JSON rows and CSV: [docs/json-and-fields.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.3.4/docs/json-and-fields.md)
- Changes: [CHANGELOG.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.3.4/CHANGELOG.md)
- Examples: [examples/README.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.3.4/examples/README.md)

Thank you for trying Portable Pipelines.
