# Portable Pipelines 0.1.3

Nested pipelines now stream for live callers, nine more host stages are
in the catalog, and the Windows zip is built on GitHub with the Microsoft
toolchain. Portable Pipelines remains a small portable `pipe` command,
written in tribute to John Hartmann, who created CMS Pipelines. Records
still move the way John designed: a stage never calls the next one; it
only reads records and writes records, and a coroutine dispatcher moves
them along.

## What's new

- **Nested pipelines stream.** CALLPIPE for live callers (subset REXX,
  PIPCMD, PREFACE, APPEND, and any in-process caller) joins the nested
  stages to the parent dispatcher and loans the caller's streams for the
  call. Records move one at a time, so memory stays bounded. Connectors
  `*:`, `*.input:`, `*.output:`, `*.input.n:`, `*.output.n:`, and
  PRIMARY / SECONDARY / TERTIARY are honoured. TOLABEL, WHILELABEL,
  STRTOLABEL, and STRWHILELABEL leave their matching record so the
  caller can read it again. ADDPIPE is still a CALLPIPE alias. A Regina
  stage still drains its input, runs, then writes buffered output.
- **Nine host stages.** VARDROP, DATECONVERT, GETFILES, GREG2SEC,
  SEC2GREG, STACK, SUBCOM, SYSVAR, and TIMESTAMP. DATECONVERT is a
  portable FROM/TO subset, not an invented DATE stage. STACK is an
  in-process deque, not the CMS console stack. SUBCOM and SYSVAR talk to
  the host shell and environment, not CMS or TSO. COMMAND and SUBCOM on
  Windows now pass the command text to `cmd /C` the way you would type
  it, so nested quotes survive.
- **REXX after TAKE and DROP LAST.** A Regina stage used to fail with
  RC 8 "stream severed" when an upstream stage SHORTed its output.
  It now treats that as end of file, matching the subset interpreter,
  Python stages, and PIPCMD.
- **The Windows zip is a GitHub Release asset.** GitHub Actions builds
  it on a Windows runner with the MSVC toolchain, attaches
  `pipe-windows-x64-v0.1.3.zip` and a matching `.sha256` file, and
  records a build provenance attestation. You do not need a laptop
  build. See [Windows zip, checksums, and antivirus](#windows-zip-checksums-and-antivirus)
  below.
- **120 of the 254 built-in programs** in the CMS/TSO Pipelines Author's
  Edition 1.1.11 catalog run today, some with documented limits.
  [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/docs/STATUS.md)
  lists every stage and where it stands.

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-v0.1.3.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.3/pipe-windows-x64-v0.1.3.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-v0.1.3.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.3/pipe-linux-x86_64-v0.1.3.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-v0.1.3.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.3/pipe-macos-arm64-v0.1.3.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-v0.1.3.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.3/pipe-macos-x86_64-v0.1.3.tar.gz) | macOS on Intel (x86_64) |

Each archive has a matching `.sha256` file so you can check your download. GitHub also records a build provenance attestation for each archive. The Linux and macOS tarballs do not include Regina or Python. Install those separately if you want them.

## Windows zip, checksums, and antivirus

The Windows `pipe.exe` in this zip is not Authenticode-signed yet.
This build is made on GitHub Actions with the Microsoft MSVC toolchain,
not on a maintainer laptop. Some antivirus products treat an unsigned
program they have not seen before as suspicious. That does not by itself
mean the file is harmful, and it does not by itself mean the file is
safe. We do not claim any scanner result.

You can check that the zip you downloaded is the one published with
this release:

1. From the GitHub Release page, download `pipe-windows-x64-v0.1.3.zip`
   and the matching `.zip.sha256` file.
2. In PowerShell 5.1 or later, in the folder that holds both files:

   ```text
   Get-FileHash .\pipe-windows-x64-v0.1.3.zip -Algorithm SHA256
   Get-Content .\pipe-windows-x64-v0.1.3.zip.sha256
   ```

   The 64-character hex digest should match. Letter case does not
   matter.
3. If you use GitHub's CLI, you can also check the build provenance
   attestation for that zip:

   ```text
   gh attestation verify .\pipe-windows-x64-v0.1.3.zip --repo portable-pipelines/portable-pipelines
   ```

If you would rather not run a pre-built binary, clone the repository
and build from source with `cargo build --release`. See
[CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/CONTRIBUTING.md).

If a vendor still flags the file after the hash matches, please report
a false positive to that vendor and include the SHA-256. You are also
welcome to open a discussion or issue here so we know which product
complained. We cannot promise a particular scanner result.

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
tar -xzf pipe-linux-x86_64-v0.1.3.tar.gz
cd pipe-linux-x86_64
./pipe "literal hello| console"
sh ./examples/run-examples.sh
```

Each tarball unpacks into a folder named after the platform, such as `pipe-macos-arm64`.

**macOS note:** macOS may refuse to open `pipe` the first time because it was downloaded from the internet. If that happens, run this once from the unpacked folder: `xattr -r -d com.apple.quarantine .` (add `sudo` in front only if it reports a permissions error). Then `./pipe` runs normally.

To check which version you have, run `pipe --version` (`.\pipe.exe --version` on Windows). It prints `pipe 0.1.3`.

To run `pipe` from any directory, add its folder to your `PATH`.

## Requirements

- **Python stages** need Python 3.10 or later on the machine. `pipe` looks for `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y python3`
  - macOS (Homebrew): `brew install python`
- **REXX stages** on Windows use the bundled Regina. On Linux and macOS, install Regina if you want `.rexx` stages to run through real Regina; otherwise they use the embedded subset interpreter. Variable pool stages and Chapter 21 commands need Regina for full behaviour. A Regina CALLPIPE is still drain, run, then flush.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y regina-rexx libregina3-dev`
  - macOS (Homebrew): `brew install regina-rexx`

## More

- What works today, what is planned, and what is not: [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/docs/STATUS.md)
- Changes: [CHANGELOG.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/CHANGELOG.md)
- Examples: [examples/README.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/examples/README.md)
- Building from source and contributing: [CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/CONTRIBUTING.md)

Portable Pipelines is licensed under MIT OR Apache-2.0, at your option ([LICENSE-MIT](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/LICENSE-MIT), [LICENSE-APACHE](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.3/LICENSE-APACHE)). The Windows zip also carries Regina REXX under `LICENSE-Regina.txt`.

CMS Pipelines and z/VM are trademarks of IBM. Portable Pipelines is not affiliated with or endorsed by IBM.
