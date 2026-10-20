# Packaging

## Windows zip

`scripts/package-windows.ps1` builds the test/release zip that used to be assembled by hand (`pipe.exe`, `examples\`, a short `README.txt`, Regina, and MinGW runtime DLLs when the binary imports them).

### Maintainer (Windows)

From the repo root after `git pull`:

```powershell
powershell -File .\scripts\package-windows.ps1
```

Windows PowerShell 5.1 (`powershell.exe`) and PowerShell 7 (`pwsh -File .\scripts\package-windows.ps1`) are both supported. The script:

1. Runs `cargo build --release -p pipe-cli` (the repo `rust-toolchain.toml` pins `stable-x86_64-pc-windows-gnu`).
2. Stages files under `dist\pipe-windows-x64-<short git sha>\`.
3. Writes `dist\pipe-windows-x64-<short git sha>.zip` with those files at the zip root.

`dist\` is gitignored. Do not commit `regina.dll`, `en.mtb`, MinGW DLLs, or the zip.

### Cross-compile (Linux / pwsh)

```powershell
cargo +stable build --release --target x86_64-pc-windows-gnu -p pipe-cli
pwsh -File ./scripts/package-windows.ps1 -NoBuild `
  -ExePath ./target/x86_64-pc-windows-gnu/release/pipe.exe `
  -ReginaDir /path/to/Regina397w64/extract
```

`-MingwBinDir` is only required when `pipe.exe` imports `libgcc_s_seh-1.dll` or `libwinpthread-1.dll`. The script reads the PE import table and skips those DLLs when they are not imported.

### Options

| Option | Meaning |
|---|---|
| `-NoBuild` | Do not run cargo. Use `-ExePath` or `pipe.exe` already under `target/`. |
| `-ExePath` | Explicit `pipe.exe` (native `target\release\pipe.exe` or cross-compiled). |
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

`libgcc_s_seh-1.dll` and `libwinpthread-1.dll` are packaged only if `pipe.exe` imports them. A Linux cross-compile (`cargo +stable build --release --target x86_64-pc-windows-gnu -p pipe-cli` with Ubuntu mingw-w64 and rustc 1.99) produced a `pipe.exe` whose PE import table lists `KERNEL32.dll`, `msvcrt.dll`, `ntdll.dll`, `userenv.dll`, `ws2_32.dll`, `bcryptprimitives.dll`, and `api-ms-win-core-synch-l1-2-0.dll`, not those two MinGW DLLs. A rustup `stable-x86_64-pc-windows-gnu` build on Windows may still import them; the script inspects the binary being packed.

Discovery when `-MingwBinDir` is omitted:

1. Directory of `gcc` or `x86_64-w64-mingw32-gcc` on `PATH`, including `gcc -print-file-name=...`
2. rustup toolchain `stable-x86_64-pc-windows-gnu` (`bin\` and `lib\rustlib\x86_64-pc-windows-gnu\bin\`)

If an imported DLL is missing, the script names it and tells you to pass `-MingwBinDir`.
