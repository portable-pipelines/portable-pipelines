# Portable Pipelines 0.1.2

This release is for people who already write CMS Pipelines. Old REXX
filters that use PEEKTO and the rest of Chapter 21, glued labels such as
`o:fanout`, DEBLOCK FIXED, and SPEC IF/SET/PRINT can run on `pipe` with
Regina. Portable Pipelines remains a small portable `pipe` command,
written in tribute to John Hartmann, who created CMS Pipelines. Records
still move the way John designed: a stage never calls the next one; it
only reads records and writes records, and a coroutine dispatcher moves
them along.

## What's new

- **Old CMS scripts run more cleanly.** Labels may sit against the stage
  verb (`o:fanout`, `v:verify`, `short: locate 1`) and a label may stand
  alone. DEBLOCK FIXED *n* (also a bare number, and bare DEBLOCK as FIXED
  80) concatenates input as a byte stream and splits every *n*
  characters. SPEC now has field identifiers (`a:1`), counters `#0` to
  `#9`, IF/THEN/ELSE/ENDIF, SET (`:=` / `+=`), PRINT, PICTURE (a run of
  `9` digits), and NOWRITE. CMS REXX `/=` and `^=` are rewritten to
  Regina `\=` outside strings and comments, so old CMS filters can run
  unmodified.
- **The rest of the REXX pipeline commands.** PEEKTO and the eighteen
  other Chapter 21 verbs that were still to come are in this release.
  That is **22 complete** and **4 limited**. Shared verbs use the same
  code as PIPCMD. SELECT accepts BOTH, a bare number, ANYINPUT, and the
  IBM abbreviations. COMMIT runs in the subset interpreter as well as
  Regina. CALLPIPE and ADDPIPE are still buffered. `examples/luhn.rexx`
  is an original Luhn check that uses PEEKTO then CALLPIPE `*:`.
- **Tests, and two bugs they found.** Dedicated tests per command, plus
  matching depth for the CMS compatibility work. `SETRC 7` then `EXIT 0`
  in the subset interpreter now keeps stage RC 7, matching Regina.
  `SEVER OUTPUT` waits for an already written record to be consumed, so
  that record is not dropped.
- **A stalled pipeline** now prints blocked stages in plain words and a
  short hint. The old debug dump is only with `--trace`. Return code 8 is
  unchanged.
- **The Windows zip** from `scripts/package-windows.ps1` drops
  `__pycache__` and `*.pyc`.
- **111 of the 254 built-in programs** in the CMS/TSO Pipelines Author's
  Edition 1.1.11 catalog run today, some with documented limits.
  [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.2/docs/STATUS.md)
  lists every stage and where it stands.

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-v0.1.2.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.2/pipe-windows-x64-v0.1.2.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-v0.1.2.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.2/pipe-linux-x86_64-v0.1.2.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-v0.1.2.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.2/pipe-macos-arm64-v0.1.2.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-v0.1.2.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/v0.1.2/pipe-macos-x86_64-v0.1.2.tar.gz) | macOS on Intel (x86_64) |

Each Linux and macOS tarball has a matching `.sha256` file so you can check your download. Those tarballs do not include Regina or Python. Install those separately if you want them.

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
tar -xzf pipe-linux-x86_64-v0.1.2.tar.gz
cd pipe-linux-x86_64
./pipe "literal hello| console"
sh ./examples/run-examples.sh
```

Each tarball unpacks into a folder named after the platform, such as `pipe-macos-arm64`.

**macOS note:** macOS may refuse to open `pipe` the first time because it was downloaded from the internet. If that happens, run this once from the unpacked folder: `xattr -r -d com.apple.quarantine .` (add `sudo` in front only if it reports a permissions error). Then `./pipe` runs normally.

To check which version you have, run `pipe --version` (`.\pipe.exe --version` on Windows). It prints `pipe 0.1.2`.

To run `pipe` from any directory, add its folder to your `PATH`.

## Requirements

- **Python stages** need Python 3.10 or later on the machine. `pipe` looks for `PIPE_PYTHON`, then `py -3` on Windows, then `python3`, then `python`.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y python3`
  - macOS (Homebrew): `brew install python`
- **REXX stages** on Windows use the bundled Regina. On Linux and macOS, install Regina if you want `.rexx` stages to run through real Regina; otherwise they use the embedded subset interpreter. Variable pool stages and the new Chapter 21 commands need Regina for full behaviour.
  - Linux (Debian/Ubuntu): `sudo apt-get install -y regina-rexx libregina3-dev`
  - macOS (Homebrew): `brew install regina-rexx`

## More

- What works today, what is planned, and what is not: [docs/STATUS.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.2/docs/STATUS.md)
- Changes: [CHANGELOG.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.2/CHANGELOG.md)
- Examples: [examples/README.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.2/examples/README.md)
- Building from source and contributing: [CONTRIBUTING.md](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.2/CONTRIBUTING.md)

Portable Pipelines is licensed under MIT OR Apache-2.0, at your option ([LICENSE-MIT](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.2/LICENSE-MIT), [LICENSE-APACHE](https://github.com/portable-pipelines/portable-pipelines/blob/v0.1.2/LICENSE-APACHE)). The Windows zip also carries Regina REXX under `LICENSE-Regina.txt`.

CMS Pipelines and z/VM are trademarks of IBM. Portable Pipelines is not affiliated with or endorsed by IBM.
