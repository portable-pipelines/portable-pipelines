# Packaging

Portable Pipelines ships two kinds of binary packages:

- A **Windows zip** that the maintainer builds on a Windows laptop.
- **Linux and macOS tarballs** built by the GitHub Actions release workflow when a GitHub Release is published.

Neither Unix tarball bundles Regina or any other third-party binary. Regina and Python 3.10 or later are installed separately on the machine that runs `pipe`.

## Windows zip

`scripts/package-windows.ps1` builds the test/release zip (`pipe.exe`, `examples\`, the `pipelines\` helper package for Python stages, a short `README.txt`, `LICENSE-MIT`, `LICENSE-APACHE`, Regina, and MinGW runtime DLLs when the binary imports them).

### Maintainer (Windows)

From the repo root after `git pull`:

```powershell
powershell -File .\scripts\package-windows.ps1
```

Windows PowerShell 5.1 (`powershell.exe`) and PowerShell 7 (`pwsh -File .\scripts\package-windows.ps1`) are both supported. The same command works from `cmd.exe`. The script:

1. Runs `cargo build --release --target x86_64-pc-windows-gnu -p pipe-cli`. The repo `rust-toolchain.toml` pins channel `stable` and lists that GNU target so rustup installs it. The binary is `target\x86_64-pc-windows-gnu\release\pipe.exe`.
2. Stages files under `dist\pipe-windows-x64-<short git sha>\`, including `pipelines\` from `crates/pipe-python/python/pipelines` and the two project license files.
3. Writes `dist\pipe-windows-x64-<short git sha>.zip` with those files at the zip root.

If rustup has not yet installed the GNU target, the first run may take a little longer while it does. You can also install it yourself:

```powershell
rustup target add x86_64-pc-windows-gnu
```

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
2. rustup `stable` sysroot and, if still installed, the older `stable-x86_64-pc-windows-gnu` toolchain (`bin\` and `lib\rustlib\x86_64-pc-windows-gnu\bin\`)

If an imported DLL is missing, the script names it and tells you to pass `-MingwBinDir`.

## Linux and macOS tarballs (GitHub Release)

`.github/workflows/release.yml` runs only when a GitHub Release is published (`release: types: [published]`). You can also start it by hand with `workflow_dispatch`. It does not run on push or pull request.

It builds `pipe` in release mode for:

- Linux x86_64 (`ubuntu-22.04`, `x86_64-unknown-linux-gnu`)
- macOS arm64 (`macos-14`, `aarch64-apple-darwin`)
- macOS x86_64 (`macos-14` cross-compile, `x86_64-apple-darwin`)

Each job writes a tarball named `pipe-<os>-<arch>-<tag>.tar.gz` (for example `pipe-linux-x86_64-v0.1.0.tar.gz`) and uploads it to that release with `GITHUB_TOKEN`. A `workflow_dispatch` run builds the same tarballs but does not upload them to a release.

Each tarball contains a top-level folder with:

- the `pipe` binary
- `examples/`
- the Python `pipelines` helper folder
- `README.md`
- `LICENSE-MIT` and `LICENSE-APACHE`
- a short `README.txt` that says Regina and Python 3.10+ are installed separately (`apt` or `brew`)

The workflow does not bundle Regina or any third-party binary. It does not build or upload the Windows zip; keep using `scripts/package-windows.ps1` on Windows for that.

End-user copy: `packaging/unix/README.txt`.
