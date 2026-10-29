# Packaging

Portable Pipelines ships two kinds of binary packages:

- A **Windows zip** built by the GitHub Actions release workflow on a GitHub-hosted Windows runner when a GitHub Release is published.
- **Linux and macOS tarballs** built by that same workflow.

You can still pack a Windows zip on a laptop with `scripts/package-windows.ps1`. That local default stays GNU so you do not need Visual Studio.

Neither Unix tarball bundles Regina or any other third-party binary. Regina and Python 3.10 or later are installed separately on the machine that runs `pipe`. The Windows zip does bundle Regina (see below).

## Windows zip

`scripts/package-windows.ps1` builds the test/release zip (`pipe.exe`, `examples\` including `run-examples.ps1`, the `pipelines\` helper package for Python stages, a short `README.txt`, `LICENSE-MIT`, `LICENSE-APACHE`, Regina, and MinGW runtime DLLs when the binary imports them). End users can run `powershell -ExecutionPolicy Bypass -File .\examples\run-examples.ps1` from the unzipped folder. After the zip is written, the script also writes a GNU `sha256sum`-style sidecar named `<zip>.sha256`.

The GitHub Release zip uses the MSVC toolchain (`x86_64-pc-windows-msvc`). Regina is not linked at compile time: `pipe.exe` loads `regina.dll` at run time (`LoadLibraryW` in `crates/pipe-rexx`). That is the same for GNU and MSVC builds. An MSVC `pipe.exe` does not import MinGW runtime DLLs, so those files are omitted from the Release zip unless a future binary actually imports them.

### GitHub Release (Windows runner)

`.github/workflows/release.yml` has a `pipe-windows-x64` job on `windows-latest`. On a published Release it:

1. Installs the `x86_64-pc-windows-msvc` Rust target (the runner image already has rustup and Visual Studio).
2. Runs `scripts/fetch-regina-windows.ps1` to download the official Regina 3.9.7 Windows x64 zip from SourceForge, check its SHA-256, and extract it. The zip is not stored in git.
3. Runs `powershell -File .\scripts\package-windows.ps1 -Target msvc -ReginaDir <extract> -Version <tag>`.
4. Smoke-tests the staged `pipe.exe` (`literal hello| console` and a short Regina `.rexx` filter).
5. Records a GitHub artifact attestation (`actions/attest-build-provenance`) for the zip.
6. Uploads `pipe-windows-x64-<tag>.zip` and `pipe-windows-x64-<tag>.zip.sha256` to the GitHub Release.

A `workflow_dispatch` run (and a pull request) builds the same zip and uploads it as a workflow artifact only. It does not publish a GitHub Release. `workflow_dispatch` has a `windows_only` input if you want to skip the Linux and macOS jobs.

### Maintainer (Windows, local)

From the repo root after `git pull`:

```powershell
powershell -File .\scripts\package-windows.ps1
```

That local default is still GNU. For a named archive that matches the Release naming:

```powershell
powershell -File .\scripts\package-windows.ps1 -Version v0.1.2
```

To pack an MSVC zip locally (needs Visual Studio Build Tools / `link.exe`):

```powershell
powershell -File .\scripts\package-windows.ps1 -Target msvc -Version v0.1.2
```

That writes `dist\pipe-windows-x64-v0.1.2.zip` and `dist\pipe-windows-x64-v0.1.2.zip.sha256` instead of using the short git sha. You do not need to attach a laptop-built zip to a GitHub Release any more.

Windows PowerShell 5.1 (`powershell.exe`) and PowerShell 7 (`pwsh -File .\scripts\package-windows.ps1`) are both supported. The same command works from `cmd.exe`. With `-Target gnu` (the default) the script:

1. Runs `cargo +stable-x86_64-pc-windows-gnu build --release --target x86_64-pc-windows-gnu -p pipe-cli`. The GNU *host* toolchain matters: `rust-toolchain.toml` pins channel `stable` so Linux and macOS clones work with plain `cargo`, and rustup's default host on Windows is often `x86_64-pc-windows-msvc`. `--target x86_64-pc-windows-gnu` alone still compiles build scripts with Visual Studio `link.exe`. The `+stable-x86_64-pc-windows-gnu` override avoids that. The binary is `target\x86_64-pc-windows-gnu\release\pipe.exe`.
2. Stages files under `dist\pipe-windows-x64-<short git sha>\` (or `dist\pipe-windows-x64-<Version>\` when you pass `-Version`), including `pipelines\` from `crates/pipe-python/python/pipelines` and the two project license files. `__pycache__` directories and `*.pyc` files are removed from the staged `examples\` and `pipelines\` trees before the zip is written.
3. Writes `dist\pipe-windows-x64-<short git sha>.zip` (or `dist\pipe-windows-x64-<Version>.zip`) with those files at the zip root, plus a `.sha256` sidecar.

If the GNU host toolchain is not installed yet, the first GNU run installs it (`rustup toolchain install stable-x86_64-pc-windows-gnu`) and may take a little longer. You can also install it yourself:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
```

`rustup target add x86_64-pc-windows-gnu` on an MSVC host is not enough for a GNU zip. Build scripts still need the GNU linker. You do not need Visual Studio Build Tools for `-Target gnu`. You do need them for `-Target msvc`.

`dist\` is gitignored. Do not commit `regina.dll`, `en.mtb`, MinGW DLLs, or the zip.

### Cross-compile (Linux / pwsh)

```powershell
cargo build --release --target x86_64-pc-windows-gnu -p pipe-cli
pwsh -File ./scripts/package-windows.ps1 -NoBuild `
  -ExePath ./target/x86_64-pc-windows-gnu/release/pipe.exe `
  -ReginaDir /path/to/Regina397w64/extract
```

`-MingwBinDir` is only required when `pipe.exe` imports `libgcc_s_seh-1.dll` or `libwinpthread-1.dll`. The script reads the PE import table and skips those DLLs when they are not imported.

### Options

| Option | Meaning |
|---|---|
| `-NoBuild` | Do not run cargo. Use `-ExePath` or `pipe.exe` already under `target/`. |
| `-ExePath` | Explicit `pipe.exe` (GNU `target\x86_64-pc-windows-gnu\release\pipe.exe`, MSVC `target\x86_64-pc-windows-msvc\release\pipe.exe`, or a host `target\release\pipe.exe`). |
| `-ReginaDir` | Regina install directory, or a path to `regina.dll`. |
| `-MingwBinDir` | Directory that contains the MinGW runtime DLLs `pipe.exe` imports. |
| `-OutDir` | Directory for staging and the zip. Default: `<repo>/dist`. |
| `-NoRegina` | Omit Regina. Warns that REXX stages then need a system Regina (or the subset interpreter). |
| `-Version` | Name the zip and staging folder `pipe-windows-x64-<Version>` instead of `pipe-windows-x64-<gitsha>`. |
| `-Target` | `gnu` (default, no Visual Studio) or `msvc` (GitHub Release zip; needs `link.exe`). |

End-user copy: `packaging/windows/README.txt` (copied into the zip as `README.txt`).

### Regina discovery (when `-ReginaDir` is omitted)

Same order as `crates/pipe-rexx/src/regina/discover.rs`:

1. `PIPE_REGINA`, `REGINA_HOME`, `REGINA` (file or directory)
2. `PATH`
3. Common Windows dirs: `C:\Program Files\Regina`, `C:\Program Files (x86)\Regina`, `C:\Regina`, plus `%ProgramFiles%\Regina`

`en.mtb` and the license (`LICENSE-Regina.txt`, else `COPYING-LIB` / `COPYING` / `LICENSE`) are taken from next to the DLL or the install tree, and the license is stored as `LICENSE-Regina.txt`. A missing file fails with the filename and the override that fixes it (`-ReginaDir`).

### Official Regina zip used by CI

`scripts/fetch-regina-windows.ps1` downloads the official Regina 3.9.7 Windows x64 zip from SourceForge, checks the SHA-256 printed on that file's SourceForge page, and extracts it. Pass the extract directory to `package-windows.ps1 -ReginaDir`.

- File: `Regina397w64.zip`
- URL: https://sourceforge.net/projects/regina-rexx/files/regina-rexx/3.9.7/Regina397w64.zip/download
- SHA-256: `7cc7993bcbcabea57e38f17468ad94081503b4d1b1685964891567b093860add`

Do not commit that zip or `regina.dll`. The fetch script writes under `dist/regina-windows/` (gitignored) unless you pass `-OutDir`.

### MinGW runtime DLLs

`libgcc_s_seh-1.dll` and `libwinpthread-1.dll` are packaged only if `pipe.exe` imports them. A Linux cross-compile (`cargo build --release --target x86_64-pc-windows-gnu -p pipe-cli` with Ubuntu mingw-w64) produced a `pipe.exe` whose PE import table lists `KERNEL32.dll`, `msvcrt.dll`, `ntdll.dll`, `userenv.dll`, `ws2_32.dll`, `bcryptprimitives.dll`, and `api-ms-win-core-synch-l1-2-0.dll`, not those two MinGW DLLs. A Windows GNU build may still import them; the script inspects the binary being packed. An MSVC build typically does not import them.

Discovery when `-MingwBinDir` is omitted:

1. Directory of `gcc` or `x86_64-w64-mingw32-gcc` on `PATH`, including `gcc -print-file-name=...`
2. rustup `stable` sysroot and the GNU host toolchain `stable-x86_64-pc-windows-gnu` (`bin\` and `lib\rustlib\x86_64-pc-windows-gnu\bin\`)

If an imported DLL is missing, the script names it and tells you to pass `-MingwBinDir`.

### If your antivirus complains

Portable Pipelines is a new project. The Windows `pipe.exe` in the GitHub Release zip is not Authenticode-signed yet. Some antivirus products treat an unsigned program they have not seen before as suspicious. That does not by itself mean the file is harmful, and it does not by itself mean the file is safe.

Check the SHA-256 of the zip against the `.zip.sha256` file on the GitHub Release. If you use GitHub's CLI:

```powershell
Get-FileHash .\pipe-windows-x64-v0.1.2.zip -Algorithm SHA256
Get-Content .\pipe-windows-x64-v0.1.2.zip.sha256
gh attestation verify .\pipe-windows-x64-v0.1.2.zip --repo portable-pipelines/portable-pipelines
```

You can also clone this repository and build from source (`cargo build --release`). If a vendor still flags the file after the hash matches, please report a false positive to that vendor and include the SHA-256. We cannot promise a particular scanner result. There is a matching section in the [README](../README.md#if-your-antivirus-complains).

## Linux and macOS tarballs (GitHub Release)

`.github/workflows/release.yml` runs when a GitHub Release is published (`release: types: [published]`). You can also start it by hand with `workflow_dispatch`. Pull requests run the Windows job only, as a packaging dry run. The workflow does not run on every push.

It builds `pipe` in release mode for:

- Linux x86_64 (`ubuntu-22.04`, `x86_64-unknown-linux-gnu`)
- macOS arm64 (`macos-14`, `aarch64-apple-darwin`)
- macOS x86_64 (`macos-14` cross-compile, `x86_64-apple-darwin`)
- Windows x64 (`windows-latest`, `x86_64-pc-windows-msvc`), as the zip described above

After each Unix build, the job runs the binary with `literal hello| console` and checks that stdout is exactly `hello`. The macOS x86_64 binary is cross-built on the arm64 runner; that smoke test uses `arch -x86_64` (Rosetta) when it is available, and otherwise skips with a log note.

Each Unix job writes a tarball named `pipe-<os>-<arch>-<tag>.tar.gz` (for example `pipe-linux-x86_64-v0.1.2.tar.gz`) and a matching `*.tar.gz.sha256` checksum file, records a GitHub artifact attestation, then uploads both files to that release with `GITHUB_TOKEN`. A `workflow_dispatch` run builds the same tarballs and checksums but does not upload them to a release.

Each tarball contains a top-level folder with:

- the `pipe` binary
- `examples/` (including `run-examples.sh` and `run-examples.ps1`)
- the Python `pipelines` helper folder
- `README.md`
- `LICENSE-MIT` and `LICENSE-APACHE`
- a short `README.txt` that says Regina and Python 3.10+ are installed separately (`apt` or `brew`)

Unix tarballs do not bundle Regina or any third-party binary.

End-user copy: `packaging/unix/README.txt`.
