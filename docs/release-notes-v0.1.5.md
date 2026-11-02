# Portable Pipelines 0.1.5

CALLPIPE from a Regina helper now streams, including STEM and VAR. Named
stream identifiers work on FANOUT, FANIN, DEAL, GATHER, SELECT,
ADDSTREAM, and STREAMNUM. ADDPIPE is a real add to the running set, and
every pull request runs tests on Linux, macOS, and Windows. Portable
Pipelines remains a small portable `pipe` command, written in tribute to
John Hartmann, who created CMS Pipelines. Records still move the way
John designed: a stage never calls the next one; it only reads records
and writes records, and a coroutine dispatcher moves them along.

## What's new

- **Live CALLPIPE from a Regina helper.** CALLPIPE issued from a helper
  `.rexx` stage loans the stage's live streams in the parent, the same
  way subset REXX and PIPCMD already did. An infinite producer plus
  downstream TAKE N stops early. STEM, VAR, and the other variable-pool
  stages stay on that live path through reverse SHV.
  `PIPE_REGINA_INPROC=1` still drains, then runs, then flushes.
- **Named stream identifiers.** FANOUT, FANIN, DEAL, GATHER, SELECT,
  ADDSTREAM, and STREAMNUM accept a name as well as a number. PRIMARY,
  SECONDARY, and TERTIARY are 0, 1, and 2. FANOUT and FANIN take
  identifier lists (`FANOUT LEFT RIGHT`).
- **A real ADDPIPE.** ADDPIPE adds a specification to the running set
  (insert-between). It does not wait and does not restore. The caller
  keeps reading and writing. PIPCMD, subset REXX, and a Regina helper
  share that path.
- **GitHub Actions CI.** Every pull request, and every push to master,
  runs the workspace tests on Linux, macOS (Apple silicon), and Windows.
- **120 of the 254 built-in programs** in the CMS/TSO Pipelines Author's
  Edition 1.1.11 catalog run today, some with documented limits. REXX
  pipeline commands are **23 complete** and **3 limited** (ADDPIPE moved
  to complete).
  [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/docs/STATUS.md)
  lists every stage and where it stands.

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-v0.1.5.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.5/pipe-windows-x64-v0.1.5.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-v0.1.5.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.5/pipe-linux-x86_64-v0.1.5.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-v0.1.5.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.5/pipe-macos-arm64-v0.1.5.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-v0.1.5.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.5/pipe-macos-x86_64-v0.1.5.tar.gz) | macOS on Intel (x86_64) |

Each archive has a matching `.sha256` file so you can check your download. GitHub also records a build provenance attestation for each archive. The Linux and macOS tarballs do not include Regina or Python. Install those separately if you want them.

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

1. From the GitHub Release page, download `pipe-windows-x64-v0.1.5.zip`
   and the matching `.zip.sha256` file.
2. In PowerShell 5.1 or later, in the folder that holds both files:

   ```text
   Get-FileHash .\pipe-windows-x64-v0.1.5.zip -Algorithm SHA256
   Get-Content .\pipe-windows-x64-v0.1.5.zip.sha256
   ```

   The 64-character hex digest should match. Letter case does not
   matter.
3. If you use GitHub's CLI, you can also check the build provenance
   attestation for that zip:

   ```text
   gh attestation verify .\pipe-windows-x64-v0.1.5.zip --repo portable-pipelines/portable-pipelines
   ```

If you would rather not run a pre-built binary, clone the repository
and build from source with `cargo build --release`. See
[CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/CONTRIBUTING.md).

If a vendor still flags the file after the hash matches, please report
a false positive to that vendor and include the SHA-256. You are also
welcome to open a discussion or issue here so we know which product
complained. We cannot promise a particular scanner result.

## Known limits

- `PIPE_REGINA_INPROC=1` still drains the whole stage, then runs, then
  flushes. That fallback is not a live path.
- `SUSPEND` cannot yield the parent while Regina is in bytecode.
- Python stages are single-stream and have no ADDPIPE.
- FANINANY has no identifier list.
- PRIMARY, SECONDARY, and TERTIARY cannot be renamed.

## Quick start

A small tip first: in `literal hello| console` there is no space before the `|`. Any space after the literal text becomes part of the record.

**Windows**

1. Extract the zip into a folder of its own, and open a terminal in that folder.
2. Run:

   ```text
   .\pipe.exe "literal hello| console"
   ```

   In Command Prompt you can leave off the `.\` and type `pipe.exe "literal hello| console"`.
3. Take the tour:

   ```text
   powershell -ExecutionPolicy Bypass -File .\examples\run-examples.ps1
   ```

**Linux and macOS** (use the file name for your platform)

```sh
tar -xzf pipe-linux-x86_64-v0.1.5.tar.gz
cd pipe-linux-x86_64
./pipe "literal hello| console"
sh ./examples/run-examples.sh
```

Each tarball unpacks into a folder named after the platform, such as `pipe-macos-arm64`.

**macOS note:** macOS may refuse to open `pipe` the first time because it was downloaded from the internet. If that happens, run this once from the unpacked folder: `xattr -r -d com.apple.quarantine .` (add `sudo` in front only if it reports a permissions error). Then `./pipe` runs normally.

To check which version you have, run `pipe --version` (`.\pipe.exe --version` on Windows). It prints `pipe 0.1.5`.

To run `pipe` from any directory, add its folder to your `PATH`.

## Requirements

- **Python stages** need Python 3.10 or later on the machine. `pipe` looks for `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y python3`
  - macOS (Homebrew): `brew install python`
- **REXX stages** on Windows use the bundled Regina. On Linux and macOS, install Regina if you want `.rexx` stages to run through real Regina; otherwise they use the embedded subset interpreter. Variable pool stages and Chapter 21 commands need Regina for full behaviour. A Regina stage streams through a helper process, including CALLPIPE (STEM and VAR too). `PIPE_REGINA_INPROC=1` still drains, then runs, then flushes.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y regina-rexx libregina3-dev`
  - macOS (Homebrew): `brew install regina-rexx`

## More

- What works today, what is planned, and what is not: [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/docs/STATUS.md)
- Changes: [CHANGELOG.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/CHANGELOG.md)
- Examples: [examples/README.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/examples/README.md)
- Building from source and contributing: [CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/CONTRIBUTING.md)

Portable Pipelines is licensed under MIT OR Apache-2.0, at your option ([LICENSE-MIT](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/LICENSE-MIT), [LICENSE-APACHE](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.5/LICENSE-APACHE)). The Windows zip also carries Regina REXX under `LICENSE-Regina.txt`.

CMS Pipelines and z/VM are trademarks of IBM. Portable Pipelines is not affiliated with or endorsed by IBM.
