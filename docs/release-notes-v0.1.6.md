# Portable Pipelines 0.1.6

A record can be one JSON object, and a dollar field such as `$qty` or
`$books.0.title` can stand in for a column in SPEC, PICK, SORT, LOCATE,
NLOCATE, CHANGE, COLLATE, and LOOKUP. CSV reads and writes those
objects. Host file stages FILEBACK, FILERAND, FILEFAST, FILESLOW, and
FILEUPDATE are in, along with CRC, INSTORE, and OUTSTORE. A REXX stage
that stops early now severs input it did not read, so the pipeline
finishes instead of stalling. Portable Pipelines remains a small
portable `pipe` command, written in tribute to John Hartmann, who
created CMS Pipelines. Records still move the way John designed: a
stage never calls the next one; it only reads records and writes
records, and a coroutine dispatcher moves them along.

## What's new

- **JSON and CSV.** `json add`, `json set`, `json delete`, `json keep`,
  and `json get` edit or extract fields. `json split` writes one record
  per array element. `json collect` joins object records into an array.
  `json array` expands a top-level JSON array into object records. `csv`
  reads a header row and later rows as JSON objects. `csv write` writes
  those objects back to CSV. `csv strings` keeps every field a string.
  Reference: [docs/json-and-fields.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/docs/json-and-fields.md).
- **Dollar fields.** `$qty`, `$a.b`, and `$books.0.title` (and
  `$books[0].title`) work as an input range in SPEC, PICK, SORT,
  LOCATE, NLOCATE, CHANGE, COLLATE, and LOOKUP. A numeric step indexes
  an array, and selects the object key when that step is an object.
  Negative indexes count from the end.
- **Host files.** FILEBACK reads a file last record first. FILERAND
  reads selected records or ranges. FILEFAST is the host-path stand-in
  for DISK. FILESLOW is FILEFAST with optional FROM. FILEUPDATE
  replaces records whose numbers sit in columns 1-10.
- **CRC, INSTORE, and OUTSTORE.** CRC writes a 16-bit checksum of the
  concatenated input. INSTORE holds records and writes one token.
  OUTSTORE writes those records back. They work across CALLPIPE in the
  same process.
- **A REXX stage that stops early.** If it ends without reading the
  rest of its input, those connections are severed. An upstream writer
  gets RC 12 and the pipeline finishes instead of stalling.
- **Clearer JSON and PICK errors.** A missing field message echoes the
  operand you wrote. PICK accepts `!=` (numeric) and `!==` (string).
  A numeric PICK operator on text names the string operator to use.
- **128 of the 254 built-in programs** in the CMS/TSO Pipelines
  Author's Edition 1.1.11 catalog run today, some with documented
  limits. JSON and CSV sit beside that catalog. REXX pipeline commands
  are **23 complete** and **3 limited**.
  [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/docs/STATUS.md)
  lists every stage and where it stands.
- **Stable download names.** Each archive is uploaded under its
  versioned name and again under a name with no version. The second
  file is a copy of the same bytes, with its own `.sha256` file. A
  latest-release link such as
  `https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-windows-x64.zip`
  keeps working after later releases.

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-v0.1.6.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.6/pipe-windows-x64-v0.1.6.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-v0.1.6.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.6/pipe-linux-x86_64-v0.1.6.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-v0.1.6.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.6/pipe-macos-arm64-v0.1.6.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-v0.1.6.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.6/pipe-macos-x86_64-v0.1.6.tar.gz) | macOS on Intel (x86_64) |

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

1. From the GitHub Release page, download `pipe-windows-x64-v0.1.6.zip`
   and the matching `.zip.sha256` file. `pipe-windows-x64.zip` is the
   same zip under a stable name, with its own `.sha256` file.
2. In PowerShell 5.1 or later, in the folder that holds both files:

   ```text
   Get-FileHash .\pipe-windows-x64-v0.1.6.zip -Algorithm SHA256
   Get-Content .\pipe-windows-x64-v0.1.6.zip.sha256
   ```

   The 64-character hex digest should match. Letter case does not
   matter. The same check works for `pipe-windows-x64.zip`.
3. If you use GitHub's CLI, you can also check the build provenance
   attestation for that zip:

   ```text
   gh attestation verify .\pipe-windows-x64-v0.1.6.zip --repo portable-pipelines/portable-pipelines
   ```

If you would rather not run a pre-built binary, clone the repository
and build from source with `cargo build --release`. See
[CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/CONTRIBUTING.md).

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
tar -xzf pipe-linux-x86_64-v0.1.6.tar.gz
cd pipe-linux-x86_64
./pipe "literal hello| console"
sh ./examples/run-examples.sh
```

`pipe-linux-x86_64.tar.gz` unpacks into the same `pipe-linux-x86_64` folder. Each tarball unpacks into a folder named after the platform, such as `pipe-macos-arm64`.

**macOS note:** macOS may refuse to open `pipe` the first time because it was downloaded from the internet. If that happens, run this once from the unpacked folder: `xattr -r -d com.apple.quarantine .` (add `sudo` in front only if it reports a permissions error). Then `./pipe` runs normally.

To check which version you have, run `pipe --version` (`.\pipe.exe --version` on Windows). It prints `pipe 0.1.6`.

To run `pipe` from any directory, add its folder to your `PATH`.

One JSON example, from the unpacked folder:

```text
pipe "< examples/orders.csv | csv | json add $total = $qty * $amount | pick $total > /100/ | spec $customer 1 / spent / next $total next | console"
```

## Requirements

- **Python stages** need Python 3.10 or later on the machine. `pipe` looks for `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y python3`
  - macOS (Homebrew): `brew install python`
- **REXX stages** on Windows use the bundled Regina. On Linux and macOS, install Regina if you want `.rexx` stages to run through real Regina; otherwise they use the embedded subset interpreter. Variable pool stages and Chapter 21 commands need Regina for full behaviour. A Regina stage streams through a helper process, including CALLPIPE (STEM and VAR too). `PIPE_REGINA_INPROC=1` still drains, then runs, then flushes.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y regina-rexx libregina3-dev`
  - macOS (Homebrew): `brew install regina-rexx`

## More

- What works today, what is planned, and what is not: [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/docs/STATUS.md)
- JSON rows and CSV: [docs/json-and-fields.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/docs/json-and-fields.md)
- Changes: [CHANGELOG.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/CHANGELOG.md)
- Examples: [examples/README.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/examples/README.md)
- Building from source and contributing: [CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/CONTRIBUTING.md)

Portable Pipelines is licensed under MIT OR Apache-2.0, at your option ([LICENSE-MIT](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/LICENSE-MIT), [LICENSE-APACHE](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.6/LICENSE-APACHE)). The Windows zip also carries Regina REXX under `LICENSE-Regina.txt`.

CMS Pipelines and z/VM are trademarks of IBM. Portable Pipelines is not affiliated with or endorsed by IBM.
