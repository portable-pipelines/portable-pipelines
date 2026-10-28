# Packaging

Portable Pipelines ships two kinds of binary packages:

- A **Windows zip** that the maintainer builds on a Windows laptop.
- **Linux and macOS tarballs** built by the GitHub Actions release workflow when a GitHub Release is published.

Neither Unix tarball bundles Regina or any other third-party binary. Regina and Python 3.10 or later are installed separately on the machine that runs `pipe`.

## Windows zip

`scripts/package-windows.ps1` builds the test/release zip (`pipe.exe`, `examples\` including `run-examples.ps1`, the `pipelines\` helper package for Python stages, a short `README.txt`, `LICENSE-MIT`, `LICENSE-APACHE`, Regina, and MinGW runtime DLLs when the binary imports them). End users can run `powershell -ExecutionPolicy Bypass -File .\examples\run-examples.ps1` from the unzipped folder.

### Maintainer (Windows)

From the repo root after `git pull`:

```powershell
powershell -File .\scripts\package-windows.ps1
```

For a named release zip (upload this file as the Windows GitHub Release asset):

```powershell
powershell -File .\scripts\package-windows.ps1 -Version v0.1.0
```

That writes `dist\pipe-windows-x64-v0.1.0.zip` instead of `dist\pipe-windows-x64-<short git sha>.zip`. Upload the release asset as `pipe-windows-x64-v0.1.0.zip`.

Windows PowerShell 5.1 (`powershell.exe`) and PowerShell 7 (`pwsh -File .\scripts\package-windows.ps1`) are both supported. The same command works from `cmd.exe`. The script:

1. Runs `cargo +stable-x86_64-pc-windows-gnu build --release --target x86_64-pc-windows-gnu -p pipe-cli`. The GNU *host* toolchain matters: `rust-toolchain.toml` pins channel `stable` so Linux and macOS clones work with plain `cargo`, and rustup's default host on Windows is often `x86_64-pc-windows-msvc`. `--target x86_64-pc-windows-gnu` alone still compiles build scripts with Visual Studio `link.exe`. The `+stable-x86_64-pc-windows-gnu` override avoids that. The binary is `target\x86_64-pc-windows-gnu\release\pipe.exe`.
2. Stages files under `dist\pipe-windows-x64-<short git sha>\` (or `dist\pipe-windows-x64-<Version>\` when you pass `-Version`), including `pipelines\` from `crates/pipe-python/python/pipelines` and the two project license files. `__pycache__` directories and `*.pyc` files are removed from the staged `examples\` and `pipelines\` trees before the zip is written.
3. Writes `dist\pipe-windows-x64-<short git sha>.zip` (or `dist\pipe-windows-x64-<Version>.zip`) with those files at the zip root.

If the GNU host toolchain is not installed yet, the first run installs it (`rustup toolchain install stable-x86_64-pc-windows-gnu`) and may take a little longer. You can also install it yourself:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
```

`rustup target add x86_64-pc-windows-gnu` on an MSVC host is not enough. Build scripts still need the GNU linker. You do not need Visual Studio Build Tools for this zip.

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
| `-ExePath` | Explicit `pipe.exe` (GNU `target\x86_64-pc-windows-gnu\release\pipe.exe` or a host `target\release\pipe.exe`). |
| `-ReginaDir` | Regina install directory, or a path to `regina.dll`. |
| `-MingwBinDir` | Directory that contains the MinGW runtime DLLs `pipe.exe` imports. |
| `-OutDir` | Directory for staging and the zip. Default: `<repo>/dist`. |
| `-NoRegina` | Omit Regina. Warns that REXX stages then need a system Regina (or the subset interpreter). |
| `-Version` | Name the zip and staging folder `pipe-windows-x64-<Version>` instead of `pipe-windows-x64-<gitsha>`. For the v0.1.0 release, pass `-Version v0.1.0` and upload `pipe-windows-x64-v0.1.0.zip`. |

End-user copy: `packaging/windows/README.txt` (copied into the zip as `README.txt`).

### Regina discovery (when `-ReginaDir` is omitted)

Same order as `crates/pipe-rexx/src/regina/discover.rs`:

1. `PIPE_REGINA`, `REGINA_HOME`, `REGINA` (file or directory)
2. `PATH`
3. Common Windows dirs: `C:\Program Files\Regina`, `C:\Program Files (x86)\Regina`, `C:\Regina`, plus `%ProgramFiles%\Regina`

`en.mtb` and the license (`LICENSE-Regina.txt`, else `COPYING-LIB` / `COPYING` / `LICENSE`) are taken from next to the DLL or the install tree, and the license is stored as `LICENSE-Regina.txt`. A missing file fails with the filename and the override that fixes it (`-ReginaDir`).

### MinGW runtime DLLs

`libgcc_s_seh-1.dll` and `libwinpthread-1.dll` are packaged only if `pipe.exe` imports them. A Linux cross-compile (`cargo build --release --target x86_64-pc-windows-gnu -p pipe-cli` with Ubuntu mingw-w64) produced a `pipe.exe` whose PE import table lists `KERNEL32.dll`, `msvcrt.dll`, `ntdll.dll`, `userenv.dll`, `ws2_32.dll`, `bcryptprimitives.dll`, and `api-ms-win-core-synch-l1-2-0.dll`, not those two MinGW DLLs. A Windows GNU build may still import them; the script inspects the binary being packed.

Discovery when `-MingwBinDir` is omitted:

1. Directory of `gcc` or `x86_64-w64-mingw32-gcc` on `PATH`, including `gcc -print-file-name=...`
2. rustup `stable` sysroot and the GNU host toolchain `stable-x86_64-pc-windows-gnu` (`bin\` and `lib\rustlib\x86_64-pc-windows-gnu\bin\`)

If an imported DLL is missing, the script names it and tells you to pass `-MingwBinDir`.

## Linux and macOS tarballs (GitHub Release)

`.github/workflows/release.yml` runs only when a GitHub Release is published (`release: types: [published]`). You can also start it by hand with `workflow_dispatch`. It does not run on push or pull request.

It builds `pipe` in release mode for:

- Linux x86_64 (`ubuntu-22.04`, `x86_64-unknown-linux-gnu`)
- macOS arm64 (`macos-14`, `aarch64-apple-darwin`)
- macOS x86_64 (`macos-14` cross-compile, `x86_64-apple-darwin`)

After each build, the job runs the binary with `literal hello| console` and checks that stdout is exactly `hello`. The macOS x86_64 binary is cross-built on the arm64 runner; that smoke test uses `arch -x86_64` (Rosetta) when it is available, and otherwise skips with a log note.

Each job writes a tarball named `pipe-<os>-<arch>-<tag>.tar.gz` (for example `pipe-linux-x86_64-v0.1.0.tar.gz`) and a matching `*.tar.gz.sha256` checksum file, then uploads both to that release with `GITHUB_TOKEN`. A `workflow_dispatch` run builds the same tarballs and checksums but does not upload them to a release.

Each tarball contains a top-level folder with:

- the `pipe` binary
- `examples/` (including `run-examples.sh` and `run-examples.ps1`)
- the Python `pipelines` helper folder
- `README.md`
- `LICENSE-MIT` and `LICENSE-APACHE`
- a short `README.txt` that says Regina and Python 3.10+ are installed separately (`apt` or `brew`)

The workflow does not bundle Regina or any third-party binary. It does not build or upload the Windows zip; keep using `scripts/package-windows.ps1` on Windows for that.

End-user copy: `packaging/unix/README.txt`.
