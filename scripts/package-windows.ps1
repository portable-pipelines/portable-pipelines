<#
.SYNOPSIS
    Build the Windows test/release zip for Portable Pipelines (pipe.exe plus examples and Regina).

.DESCRIPTION
    Stages the same layout the maintainer previously assembled by hand, then writes
    dist/pipe-windows-x64-<gitsha>.zip (override the directory with -OutDir, or
    pass -Version to use that label instead of the git sha).

    Layout (zip root):
      pipe.exe
      examples\
      pipelines\                               (Python helper package)
      README.txt
      LICENSE-MIT, LICENSE-APACHE
      regina.dll, en.mtb, LICENSE-Regina.txt   (unless -NoRegina)
      libgcc_s_seh-1.dll, libwinpthread-1.dll  (only if pipe.exe imports them)

    The default Windows build is
    `cargo +stable-x86_64-pc-windows-gnu build --release --target x86_64-pc-windows-gnu -p pipe-cli`
    so the zip is always the Windows GNU binary, even when rustup's default host
    is MSVC. rust-toolchain.toml stays on channel "stable" (Linux and macOS clones
    need that) and lists the GNU target. The binary lands at
    target\x86_64-pc-windows-gnu\release\pipe.exe. If the GNU host toolchain is
    missing, the script installs it with rustup.

.PARAMETER NoBuild
    Skip cargo; use -ExePath or a pipe.exe already under target/.

.PARAMETER ExePath
    Prebuilt pipe.exe (native target\release\pipe.exe or a cross-compiled
    target\x86_64-pc-windows-gnu\release\pipe.exe).

.PARAMETER ReginaDir
    Regina install directory or path to regina.dll. When omitted, uses PIPE_REGINA /
    REGINA_HOME / REGINA, then PATH, then the common Windows install dirs in
    crates/pipe-rexx/src/regina/discover.rs.

.PARAMETER MingwBinDir
    Directory that contains libgcc_s_seh-1.dll and/or libwinpthread-1.dll.
    Used only when pipe.exe imports those DLLs. When omitted, searches gcc /
    x86_64-w64-mingw32-gcc (including -print-file-name) and rustup (the current
    stable sysroot and the GNU host toolchain this script builds with).

.PARAMETER OutDir
    Directory for the staging folder and zip. Default: <repo>/dist.

.PARAMETER NoRegina
    Package without Regina. REXX stages then need Regina installed on the machine
    that runs pipe.exe, or they fall back to the embedded subset interpreter.

.PARAMETER Version
    When set, name the zip and staging folder pipe-windows-x64-<Version>
    instead of pipe-windows-x64-<gitsha>. Use this for a release asset
    (for example -Version v0.1.2 writes pipe-windows-x64-v0.1.2.zip).

.EXAMPLE
    powershell -File .\scripts\package-windows.ps1

.EXAMPLE
    powershell -File .\scripts\package-windows.ps1 -Version v0.1.2

.EXAMPLE
    pwsh -File ./scripts/package-windows.ps1 -NoBuild -ExePath ./target/x86_64-pc-windows-gnu/release/pipe.exe -ReginaDir /tmp/regina-win/extract
#>
[CmdletBinding()]
param(
    [switch]$NoBuild,
    [string]$ExePath,
    [string]$ReginaDir,
    [string]$MingwBinDir,
    [string]$OutDir,
    [switch]$NoRegina,
    [string]$Version
)

Set-StrictMode -Version 1
$ErrorActionPreference = 'Stop'

function Test-IsWindowsOS {
    if ($PSVersionTable.PSVersion.Major -ge 6) {
        return [bool]$IsWindows
    }
    return $true
}

function Join-Many {
    # Join-Path in Windows PowerShell 5.1 takes two arguments only.
    # Call as: Join-Many -Parts $root, 'target', 'release', 'pipe.exe'
    #
    # Do not use ValueFromRemainingArguments, and do not call Join-Many @(a, b, c).
    # `@(...)` is an array subexpression (one argument), not a splat. Splatting is
    # `@variable`. In Windows PowerShell 5.1 a single Object[] remaining argument is
    # converted to string with $OFS (a space), so the path becomes
    # "C:\repo target release pipe.exe" and Test-Path fails.
    param(
        [Parameter(Mandatory = $true, Position = 0)]
        [string[]]$Parts
    )
    $items = @($Parts)
    if ($items.Length -lt 1) {
        return ''
    }
    $acc = $items[0]
    $i = 1
    while ($i -lt $items.Length) {
        $acc = Join-Path -Path $acc -ChildPath $items[$i]
        $i += 1
    }
    return $acc
}

function Fail {
    param([Parameter(Mandatory = $true)][string]$Message)
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Get-RepoRoot {
    $scripts = $PSScriptRoot
    if (-not $scripts) {
        $scripts = Split-Path -Parent $MyInvocation.MyCommand.Path
    }
    $root = (Resolve-Path (Join-Path $scripts '..')).Path
    $cargo = Join-Path $root 'Cargo.toml'
    if (-not (Test-Path -LiteralPath $cargo)) {
        Fail "Cannot find repo root (no Cargo.toml next to scripts/). Looked at: $root"
    }
    return $root
}

function Get-GitShortSha {
    param([string]$RepoRoot)
    $git = Get-Command git -ErrorAction SilentlyContinue
    if (-not $git) {
        return 'nogit'
    }
    Push-Location $RepoRoot
    try {
        $sha = & git rev-parse --short HEAD
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($sha)) {
            return 'nogit'
        }
        return $sha.Trim()
    }
    finally {
        Pop-Location
    }
}

function Read-AsciiZ {
    param([byte[]]$Bytes, [int]$Offset)
    if ($Offset -lt 0 -or $Offset -ge $Bytes.Length) {
        return ''
    }
    $end = $Offset
    while ($end -lt $Bytes.Length -and $Bytes[$end] -ne 0) {
        $end++
    }
    $len = $end - $Offset
    if ($len -le 0) {
        return ''
    }
    return [System.Text.Encoding]::ASCII.GetString($Bytes, $Offset, $len)
}

function Convert-RvaToOffset {
    param(
        [byte[]]$Bytes,
        [uint32]$Rva,
        [int]$SectionStart,
        [int]$NumSections
    )
    for ($i = 0; $i -lt $NumSections; $i++) {
        $s = $SectionStart + ($i * 40)
        if (($s + 24) -gt $Bytes.Length) {
            break
        }
        $virtSize = [BitConverter]::ToUInt32($Bytes, $s + 8)
        $virtAddr = [BitConverter]::ToUInt32($Bytes, $s + 12)
        $rawSize = [BitConverter]::ToUInt32($Bytes, $s + 16)
        $rawPtr = [BitConverter]::ToUInt32($Bytes, $s + 20)
        $span = $virtSize
        if ($rawSize -gt $span) {
            $span = $rawSize
        }
        if ($span -eq 0) {
            continue
        }
        if ($Rva -ge $virtAddr -and $Rva -lt ($virtAddr + $span)) {
            return [int]($rawPtr + ($Rva - $virtAddr))
        }
    }
    return -1
}

function Get-PeImportedDllNames {
    <#
    .SYNOPSIS
        DLL names from the PE import table (and delay-load table when present).
    #>
    param([Parameter(Mandatory = $true)][string]$Path)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
        Fail "Not a PE executable: $Path"
    }
    $peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
    if ($peOffset -lt 0 -or ($peOffset + 24) -gt $bytes.Length) {
        Fail "Invalid PE header in $Path"
    }
    $peSig = [BitConverter]::ToUInt32($bytes, $peOffset)
    if ($peSig -ne 0x00004550) {
        Fail "Not a PE executable: $Path"
    }
    $coff = $peOffset + 4
    $numSections = [int][BitConverter]::ToUInt16($bytes, $coff + 2)
    $optSize = [int][BitConverter]::ToUInt16($bytes, $coff + 16)
    $opt = $coff + 20
    $magic = [BitConverter]::ToUInt16($bytes, $opt)
    if ($magic -eq 0x20B) {
        $dataDir = $opt + 112
    }
    elseif ($magic -eq 0x10B) {
        $dataDir = $opt + 96
    }
    else {
        Fail "Unsupported PE optional-header magic 0x$($magic.ToString('X')) in $Path"
    }
    $sectionStart = $opt + $optSize
    $names = New-Object System.Collections.Generic.List[string]

    function Add-ImportDir {
        param([int]$DirIndex)
        $entry = $dataDir + ($DirIndex * 8)
        if (($entry + 8) -gt $bytes.Length) {
            return
        }
        $rva = [BitConverter]::ToUInt32($bytes, $entry)
        if ($rva -eq 0) {
            return
        }
        $off = Convert-RvaToOffset -Bytes $bytes -Rva $rva -SectionStart $sectionStart -NumSections $numSections
        if ($off -lt 0) {
            return
        }
        for ($n = 0; $n -lt 256; $n++) {
            $desc = $off + ($n * 20)
            if (($desc + 20) -gt $bytes.Length) {
                break
            }
            $orig = [BitConverter]::ToUInt32($bytes, $desc)
            $nameRva = [BitConverter]::ToUInt32($bytes, $desc + 12)
            $first = [BitConverter]::ToUInt32($bytes, $desc + 16)
            if ($orig -eq 0 -and $nameRva -eq 0 -and $first -eq 0) {
                break
            }
            if ($nameRva -eq 0) {
                continue
            }
            $nameOff = Convert-RvaToOffset -Bytes $bytes -Rva $nameRva -SectionStart $sectionStart -NumSections $numSections
            if ($nameOff -lt 0) {
                continue
            }
            $dll = Read-AsciiZ -Bytes $bytes -Offset $nameOff
            if ($dll) {
                [void]$names.Add($dll)
            }
        }
    }

    Add-ImportDir -DirIndex 1

    # Delay-load (IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT = 13), RVA-based descriptors.
    $delayEntry = $dataDir + (13 * 8)
    if (($delayEntry + 8) -le $bytes.Length) {
        $delayRva = [BitConverter]::ToUInt32($bytes, $delayEntry)
        if ($delayRva -ne 0) {
            $doff = Convert-RvaToOffset -Bytes $bytes -Rva $delayRva -SectionStart $sectionStart -NumSections $numSections
            if ($doff -ge 0) {
                for ($n = 0; $n -lt 64; $n++) {
                    $desc = $doff + ($n * 32)
                    if (($desc + 8) -gt $bytes.Length) {
                        break
                    }
                    $attr = [BitConverter]::ToUInt32($bytes, $desc)
                    $nameRva = [BitConverter]::ToUInt32($bytes, $desc + 4)
                    if ($attr -eq 0 -and $nameRva -eq 0) {
                        break
                    }
                    if ($nameRva -eq 0) {
                        continue
                    }
                    $nameOff = Convert-RvaToOffset -Bytes $bytes -Rva $nameRva -SectionStart $sectionStart -NumSections $numSections
                    if ($nameOff -lt 0) {
                        continue
                    }
                    $dll = Read-AsciiZ -Bytes $bytes -Offset $nameOff
                    if ($dll) {
                        [void]$names.Add($dll)
                    }
                }
            }
        }
    }

    return @($names)
}

function Get-PathDirectories {
    $sep = [System.IO.Path]::PathSeparator
    $list = New-Object System.Collections.Generic.List[string]
    foreach ($entry in ($env:PATH -split [regex]::Escape($sep))) {
        if ([string]::IsNullOrWhiteSpace($entry)) {
            continue
        }
        $list.Add($entry.Trim()) | Out-Null
    }
    return @($list)
}

function Find-FileInDirs {
    param(
        [string[]]$Directories,
        [string[]]$Names
    )
    foreach ($dir in $Directories) {
        if ([string]::IsNullOrWhiteSpace($dir)) {
            continue
        }
        if (-not (Test-Path -LiteralPath $dir -PathType Container)) {
            continue
        }
        foreach ($name in $Names) {
            $candidate = Join-Path $dir $name
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                return (Resolve-Path -LiteralPath $candidate).Path
            }
        }
    }
    return $null
}

function Get-CommandPath {
    param([string[]]$Names)
    foreach ($name in $Names) {
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if (-not $cmd) {
            continue
        }
        # ApplicationInfo.Path is the 5.1-safe file path; Source exists on 5.0+ but
        # is empty for some command types.
        if ($null -ne $cmd.PSObject.Properties['Path'] -and $cmd.Path) {
            return [string]$cmd.Path
        }
        if ($null -ne $cmd.PSObject.Properties['Source'] -and $cmd.Source) {
            return [string]$cmd.Source
        }
    }
    return $null
}

function Test-RustupToolchainInstalled {
    param([string]$Toolchain)
    $rustup = Get-Command rustup -ErrorAction SilentlyContinue
    if (-not $rustup) {
        return $false
    }
    try {
        $rustc = & rustup which rustc --toolchain $Toolchain 2>$null
    }
    catch {
        return $false
    }
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($rustc)) {
        return $false
    }
    return (Test-Path -LiteralPath $rustc.Trim() -PathType Leaf)
}

function Install-GnuHostToolchain {
    param([string]$Toolchain)
    $installCmd = "rustup toolchain install $Toolchain"
    $rustup = Get-Command rustup -ErrorAction SilentlyContinue
    if (-not $rustup) {
        Fail "The GNU host toolchain $Toolchain is not installed, and rustup was not found on PATH. Install rustup, then run:`n  $installCmd`nOr pass -ExePath with -NoBuild."
    }
    Write-Host "Installing $Toolchain so this build uses the GNU linker, not Visual Studio link.exe."
    Write-Host "Running: $installCmd"
    & rustup toolchain install $Toolchain
    if ($LASTEXITCODE -ne 0) {
        Fail "rustup toolchain install failed with exit code $LASTEXITCODE. Install it yourself, then re-run this script:`n  $installCmd"
    }
    if (-not (Test-RustupToolchainInstalled -Toolchain $Toolchain)) {
        Fail "Installed $Toolchain, but rustup still cannot find rustc for it. Install it yourself, then re-run this script:`n  $installCmd"
    }
}

function Invoke-ReleaseBuild {
    param([string]$RepoRoot)
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        Fail "cargo not found on PATH. Install rustup, or pass -ExePath with -NoBuild."
    }
    $gnuTarget = 'x86_64-pc-windows-gnu'
    $gnuHostToolchain = 'stable-x86_64-pc-windows-gnu'
    # Always the GNU target so the zip matches the Windows GNU build.
    # On Windows, also pin the GNU *host* toolchain. rust-toolchain.toml channel
    # "stable" follows rustup's default host; if that is *-windows-msvc, then
    # cargo --target gnu still compiles build scripts with link.exe.
    $cargoArgs = @()
    if (Test-IsWindowsOS) {
        if (-not (Test-RustupToolchainInstalled -Toolchain $gnuHostToolchain)) {
            Install-GnuHostToolchain -Toolchain $gnuHostToolchain
        }
        $cargoArgs += "+$gnuHostToolchain"
    }
    $cargoArgs += @('build', '--release', '--target', $gnuTarget, '-p', 'pipe-cli')
    Write-Host "Running: cargo $($cargoArgs -join ' ')"
    Push-Location $RepoRoot
    try {
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) {
            Fail "cargo build failed with exit code $LASTEXITCODE."
        }
    }
    finally {
        Pop-Location
    }
}

function Resolve-PipeExe {
    param(
        [string]$RepoRoot,
        [string]$Explicit
    )
    if ($Explicit) {
        if (-not (Test-Path -LiteralPath $Explicit -PathType Leaf)) {
            Fail "Missing pipe.exe at $Explicit (from -ExePath)."
        }
        return (Resolve-Path -LiteralPath $Explicit).Path
    }
    $gnu = Join-Many -Parts $RepoRoot, 'target', 'x86_64-pc-windows-gnu', 'release', 'pipe.exe'
    $native = Join-Many -Parts $RepoRoot, 'target', 'release', 'pipe.exe'
    $gnuOk = Test-Path -LiteralPath $gnu -PathType Leaf
    $nativeOk = Test-Path -LiteralPath $native -PathType Leaf
    if ($gnuOk) { return (Resolve-Path -LiteralPath $gnu).Path }
    if ($nativeOk) { return (Resolve-Path -LiteralPath $native).Path }
    $gnuNote = 'not found'
    if ($gnuOk) { $gnuNote = 'found' }
    $nativeNote = 'not found'
    if ($nativeOk) { $nativeNote = 'found' }
    Fail "Missing pipe.exe. Checked:`n  $gnu ($gnuNote)`n  $native ($nativeNote)`nBuild with cargo +stable-x86_64-pc-windows-gnu --target x86_64-pc-windows-gnu, or pass -ExePath."
}

function Get-ReginaLibraryNames {
    return @('regina.dll', 'regina64.dll', 'regina32.dll')
}

function Get-ReginaCommonDirs {
    $dirs = New-Object System.Collections.Generic.List[string]
    foreach ($base in @(
            'C:\Program Files\Regina',
            'C:\Program Files (x86)\Regina',
            'C:\Regina'
        )) {
        $dirs.Add($base) | Out-Null
    }
    if ($env:ProgramFiles) {
        $dirs.Add((Join-Path $env:ProgramFiles 'Regina')) | Out-Null
    }
    $pf86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
    if ($pf86) {
        $dirs.Add((Join-Path $pf86 'Regina')) | Out-Null
    }
    return @($dirs)
}

function Get-ReginaSearchRootsFrom {
    param([string]$PathValue)
    $roots = New-Object System.Collections.Generic.List[string]
    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return @()
    }
    if (Test-Path -LiteralPath $PathValue -PathType Leaf) {
        $roots.Add((Split-Path -Parent (Resolve-Path -LiteralPath $PathValue).Path)) | Out-Null
        return @($roots)
    }
    if (Test-Path -LiteralPath $PathValue -PathType Container) {
        $resolved = (Resolve-Path -LiteralPath $PathValue).Path
        $roots.Add($resolved) | Out-Null
        $roots.Add((Join-Path $resolved 'bin')) | Out-Null
        $roots.Add((Join-Path $resolved 'lib')) | Out-Null
        return @($roots)
    }
    return @()
}

function Find-ReginaDllInRoots {
    param([string[]]$Roots)
    $names = Get-ReginaLibraryNames
    return Find-FileInDirs -Directories $Roots -Names $names
}

function Find-BesideOrUnder {
    param(
        [string[]]$Roots,
        [string[]]$Names,
        [int]$Depth = 3
    )
    $hit = Find-FileInDirs -Directories $Roots -Names $Names
    if ($hit) {
        return $hit
    }
    foreach ($root in $Roots) {
        $found = Find-NamedFileUnder -Root $root -Names $Names -MaxDepth $Depth
        if ($found) {
            return $found
        }
    }
    return $null
}

function Find-NamedFileUnder {
    # Depth-limited directory walk without Get-ChildItem -Depth (available in 5.0,
    # but the walk is obvious on 5.1 and pwsh).
    param(
        [string]$Root,
        [string[]]$Names,
        [int]$MaxDepth
    )
    if ([string]::IsNullOrWhiteSpace($Root)) {
        return $null
    }
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
        return $null
    }
    $queue = New-Object System.Collections.Generic.List[object]
    $null = $queue.Add(@{ Path = $Root; Level = 0 })
    $index = 0
    while ($index -lt $queue.Count) {
        $item = $queue[$index]
        $index += 1
        $dir = $item['Path']
        $level = $item['Level']
        foreach ($name in $Names) {
            $candidate = Join-Path -Path $dir -ChildPath $name
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                return (Resolve-Path -LiteralPath $candidate).Path
            }
        }
        if ($level -ge $MaxDepth) {
            continue
        }
        $children = @(Get-ChildItem -LiteralPath $dir -Directory -ErrorAction SilentlyContinue)
        foreach ($child in $children) {
            $null = $queue.Add(@{ Path = $child.FullName; Level = ($level + 1) })
        }
    }
    return $null
}

function Resolve-ReginaFiles {
    param([string]$OverrideDir)

    $roots = New-Object System.Collections.Generic.List[string]
    $source = $null

    if ($OverrideDir) {
        $fromOverride = Get-ReginaSearchRootsFrom -PathValue $OverrideDir
        if ($fromOverride.Count -eq 0) {
            Fail "Missing Regina directory or DLL at $OverrideDir (from -ReginaDir)."
        }
        foreach ($r in $fromOverride) { $roots.Add($r) | Out-Null }
        $source = '-ReginaDir'
    }
    else {
        foreach ($key in @('PIPE_REGINA', 'REGINA_HOME', 'REGINA')) {
            $val = [Environment]::GetEnvironmentVariable($key)
            if ([string]::IsNullOrWhiteSpace($val)) {
                continue
            }
            $fromEnv = Get-ReginaSearchRootsFrom -PathValue $val
            if ($fromEnv.Count -gt 0) {
                foreach ($r in $fromEnv) { $roots.Add($r) | Out-Null }
                $source = $key
                break
            }
            Write-Verbose "$key is set to '$val' but is not an existing file or directory."
        }
        if ($roots.Count -eq 0) {
            foreach ($p in Get-PathDirectories) {
                $roots.Add($p) | Out-Null
            }
            if (-not $source) {
                $source = 'PATH'
            }
        }
        if (Test-IsWindowsOS) {
            foreach ($d in Get-ReginaCommonDirs) {
                $roots.Add($d) | Out-Null
                if (Test-Path -LiteralPath $d -PathType Container) {
                    $roots.Add((Join-Path $d 'bin')) | Out-Null
                    $roots.Add((Join-Path $d 'lib')) | Out-Null
                }
            }
        }
        if ($source -eq 'PATH') {
            $source = 'PIPE_REGINA / REGINA_HOME / REGINA, PATH, or a common Regina install directory'
        }
    }

    $dll = Find-ReginaDllInRoots -Roots @($roots)
    if (-not $dll) {
        Fail "Missing regina.dll (looked via $source). Pass -ReginaDir to the Regina install directory (or to regina.dll), or set PIPE_REGINA / REGINA_HOME."
    }
    Write-Verbose "Found Regina DLL: $dll"

    $dllDir = Split-Path -Parent $dll
    $parent = Split-Path -Parent $dllDir
    # Next to the DLL and one level up (bin\regina.dll -> install\en.mtb).
    # Recurse only under the DLL directory, never under its parent (C:\ or /tmp).
    $beside = New-Object System.Collections.Generic.List[string]
    $beside.Add($dllDir) | Out-Null
    if ($parent) { $beside.Add($parent) | Out-Null }

    $mtb = Find-FileInDirs -Directories @($beside) -Names @('en.mtb')
    if (-not $mtb) {
        $mtb = Find-BesideOrUnder -Roots @($dllDir) -Names @('en.mtb')
    }
    if (-not $mtb) {
        Fail "Missing Regina message file en.mtb next to $dll or in the Regina install. Pass -ReginaDir to the directory that contains en.mtb."
    }

    $licenseNames = @(
        'LICENSE-Regina.txt',
        'COPYING-LIB',
        'COPYING.LIB',
        'COPYING',
        'LICENSE',
        'license.txt',
        'LICENCE'
    )
    $license = Find-FileInDirs -Directories @($beside) -Names $licenseNames
    if (-not $license) {
        $license = Find-BesideOrUnder -Roots @($dllDir) -Names $licenseNames
    }
    if (-not $license) {
        Fail "Missing Regina license (LICENSE-Regina.txt or COPYING-LIB) next to $dll or in the Regina install. Pass -ReginaDir to the directory that contains it."
    }

    return @{
        Dll     = $dll
        Mtb     = $mtb
        License = $license
    }
}

function Get-GccPrintFileName {
    param([string]$GccPath, [string]$FileName)
    try {
        $printed = & $GccPath -print-file-name=$FileName 2>$null
    }
    catch {
        return $null
    }
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($printed)) {
        return $null
    }
    $printed = $printed.Trim()
    if ($printed -eq $FileName) {
        return $null
    }
    if (Test-Path -LiteralPath $printed -PathType Leaf) {
        return (Resolve-Path -LiteralPath $printed).Path
    }
    return $null
}

function Add-GnuRustlibDirs {
    param(
        [System.Collections.Generic.List[string]]$Dirs,
        [string]$Root
    )
    if (-not $Root) {
        return
    }
    $Dirs.Add((Join-Path $Root 'bin')) | Out-Null
    $Dirs.Add((Join-Many -Parts $Root, 'lib', 'rustlib', 'x86_64-pc-windows-gnu', 'bin')) | Out-Null
}

function Get-RustupGnuDirs {
    $dirs = New-Object System.Collections.Generic.List[string]
    # Current rustc (rust-toolchain.toml channel = "stable") plus the GNU host
    # toolchain this script builds with (stable-x86_64-pc-windows-gnu).
    $rustcCmd = Get-Command rustc -ErrorAction SilentlyContinue
    if ($rustcCmd) {
        try {
            $sysroot = & rustc --print sysroot 2>$null
        }
        catch {
            $sysroot = $null
        }
        if ($LASTEXITCODE -eq 0 -and $sysroot) {
            Add-GnuRustlibDirs -Dirs $dirs -Root $sysroot.Trim()
        }
    }
    $rustup = Get-Command rustup -ErrorAction SilentlyContinue
    $toolchainNames = @('stable', 'stable-x86_64-pc-windows-gnu')
    if ($rustup) {
        foreach ($toolchain in $toolchainNames) {
            try {
                $rustc = & rustup which rustc --toolchain $toolchain 2>$null
            }
            catch {
                $rustc = $null
            }
            if ($LASTEXITCODE -eq 0 -and $rustc -and (Test-Path -LiteralPath $rustc.Trim() -PathType Leaf)) {
                $bin = Split-Path -Parent $rustc.Trim()
                $root = Split-Path -Parent $bin
                Add-GnuRustlibDirs -Dirs $dirs -Root $root
            }
        }
    }
    $homes = New-Object System.Collections.Generic.List[string]
    if ($env:RUSTUP_HOME) { $homes.Add($env:RUSTUP_HOME) | Out-Null }
    if ($env:USERPROFILE) { $homes.Add((Join-Path $env:USERPROFILE '.rustup')) | Out-Null }
    if ($env:HOME) { $homes.Add((Join-Path $env:HOME '.rustup')) | Out-Null }
    foreach ($h in $homes) {
        foreach ($toolchain in $toolchainNames) {
            Add-GnuRustlibDirs -Dirs $dirs -Root (Join-Many -Parts $h, 'toolchains', $toolchain)
        }
    }
    return @($dirs)
}

function Resolve-MingwRuntimeDlls {
    param(
        [string[]]$ImportedDlls,
        [string]$OverrideDir
    )
    $wanted = @('libgcc_s_seh-1.dll', 'libwinpthread-1.dll')
    $imported = @{}
    foreach ($d in $ImportedDlls) {
        if ($d) {
            $imported[$d] = $true
        }
    }
    $needed = @($wanted | Where-Object { $imported.ContainsKey($_) })
    if ($needed.Count -eq 0) {
        Write-Host "pipe.exe does not import libgcc_s_seh-1.dll or libwinpthread-1.dll; not packaging them."
        return @()
    }

    $found = New-Object System.Collections.Generic.List[string]
    $searchDirs = New-Object System.Collections.Generic.List[string]

    if ($OverrideDir) {
        if (-not (Test-Path -LiteralPath $OverrideDir -PathType Container)) {
            Fail "MinGW directory not found: $OverrideDir (from -MingwBinDir)."
        }
        $searchDirs.Add((Resolve-Path -LiteralPath $OverrideDir).Path) | Out-Null
    }
    else {
        $gcc = Get-CommandPath -Names @('x86_64-w64-mingw32-gcc', 'gcc')
        if ($gcc) {
            $gccDir = Split-Path -Parent $gcc
            $searchDirs.Add($gccDir) | Out-Null
            foreach ($name in $needed) {
                $printed = Get-GccPrintFileName -GccPath $gcc -FileName $name
                if ($printed) {
                    $searchDirs.Add((Split-Path -Parent $printed)) | Out-Null
                }
            }
        }
        foreach ($d in Get-RustupGnuDirs) {
            $searchDirs.Add($d) | Out-Null
        }
        foreach ($p in Get-PathDirectories) {
            $searchDirs.Add($p) | Out-Null
        }
    }

    foreach ($name in $needed) {
        $hit = $null
        if (-not $OverrideDir) {
            $gcc = Get-CommandPath -Names @('x86_64-w64-mingw32-gcc', 'gcc')
            if ($gcc) {
                $hit = Get-GccPrintFileName -GccPath $gcc -FileName $name
            }
        }
        if (-not $hit) {
            $hit = Find-FileInDirs -Directories @($searchDirs) -Names @($name)
        }
        if (-not $hit) {
            $hint = if ($OverrideDir) {
                "-MingwBinDir ($OverrideDir)"
            }
            else {
                '-MingwBinDir (gcc bin directory, or the rustup lib\rustlib\x86_64-pc-windows-gnu\bin dir that contains the DLL)'
            }
            Fail "Missing $name (imported by pipe.exe). Pass $hint."
        }
        Write-Verbose "Found MinGW DLL: $hit"
        $found.Add($hit) | Out-Null
    }
    return @($found)
}

function Remove-PythonBytecode {
    # PowerShell 5.1 and cmd compatible. Drop interpreter cache that
    # Copy-Item -Recurse would otherwise put in the zip.
    param([Parameter(Mandatory = $true)][string]$Root)
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
        return
    }
    $dirs = @(Get-ChildItem -LiteralPath $Root -Recurse -Force -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -eq '__pycache__' })
    foreach ($dir in $dirs) {
        Remove-Item -LiteralPath $dir.FullName -Recurse -Force -ErrorAction SilentlyContinue
    }
    $pyc = @(Get-ChildItem -LiteralPath $Root -Recurse -Force -File -Filter '*.pyc' -ErrorAction SilentlyContinue)
    foreach ($file in $pyc) {
        Remove-Item -LiteralPath $file.FullName -Force -ErrorAction SilentlyContinue
    }
}

function Copy-TextAsCrlf {
    param([string]$Source, [string]$Dest)
    $text = [System.IO.File]::ReadAllText($Source)
    $text = $text -replace "`r`n", "`n"
    $text = $text -replace "`n", "`r`n"
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Dest, $text, $utf8)
}

function New-ZipFromDirectory {
    param([string]$SourceDir, [string]$ZipPath)
    if (Test-Path -LiteralPath $ZipPath) {
        Remove-Item -LiteralPath $ZipPath -Force
    }
    # Compress-Archive ships with Windows PowerShell 5.1. Do not use ZipFile;
    # that type is not loaded by default on 5.1 (Add-Type is easy to miss).
    $paths = @(Get-ChildItem -LiteralPath $SourceDir -Force | ForEach-Object { $_.FullName })
    if ($paths.Count -lt 1) {
        Fail "Staging directory is empty: $SourceDir"
    }
    Compress-Archive -Path $paths -DestinationPath $ZipPath -Force
}

function Get-StagedFileNames {
    param([string]$Root)
    $rootFull = (Resolve-Path -LiteralPath $Root).Path
    $slash = [char[]]@([char]'\', [char]'/')
    $names = New-Object System.Collections.Generic.List[string]
    $entries = @(Get-ChildItem -LiteralPath $Root -Recurse -Force)
    foreach ($entry in $entries) {
        if ($entry.PSIsContainer) {
            continue
        }
        $full = $entry.FullName
        $rel = $full
        if ($full.Length -gt $rootFull.Length) {
            $rel = $full.Substring($rootFull.Length).TrimStart($slash)
        }
        $rel = $rel.Replace('\', '/')
        $names.Add($rel)
    }
    return @($names | Sort-Object)
}

# --- main ---

$RepoRoot = Get-RepoRoot
$sha = Get-GitShortSha -RepoRoot $RepoRoot
if (-not [string]::IsNullOrWhiteSpace($Version)) {
    $packageName = "pipe-windows-x64-$($Version.Trim())"
}
else {
    $packageName = "pipe-windows-x64-$sha"
}

if (-not $OutDir) {
    $OutDir = Join-Path $RepoRoot 'dist'
}
else {
    if (-not [System.IO.Path]::IsPathRooted($OutDir)) {
        $OutDir = Join-Path (Get-Location).Path $OutDir
    }
}
if (-not (Test-Path -LiteralPath $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}
$OutDir = (Resolve-Path -LiteralPath $OutDir).Path
$stage = Join-Path $OutDir $packageName
$zipPath = Join-Path $OutDir "$packageName.zip"

if (-not $NoBuild) {
    Invoke-ReleaseBuild -RepoRoot $RepoRoot
}

$exe = Resolve-PipeExe -RepoRoot $RepoRoot -Explicit $ExePath
Write-Host "Using pipe.exe: $exe"

$imports = Get-PeImportedDllNames -Path $exe
Write-Verbose ("PE imports: " + ($imports -join ', '))

$mingwDlls = Resolve-MingwRuntimeDlls -ImportedDlls $imports -OverrideDir $MingwBinDir

$regina = $null
if ($NoRegina) {
    Write-Warning "Packaging without Regina (-NoRegina). REXX stages will need Regina installed on the machine that runs pipe.exe, or they will use the embedded subset interpreter."
}
else {
    $regina = Resolve-ReginaFiles -OverrideDir $ReginaDir
}

$readmeTemplate = Join-Many -Parts $RepoRoot, 'packaging', 'windows', 'README.txt'
if (-not (Test-Path -LiteralPath $readmeTemplate -PathType Leaf)) {
    Fail "Missing end-user README template: $readmeTemplate"
}

$licenseMit = Join-Path $RepoRoot 'LICENSE-MIT'
$licenseApache = Join-Path $RepoRoot 'LICENSE-APACHE'
if (-not (Test-Path -LiteralPath $licenseMit -PathType Leaf)) {
    Fail "Missing $licenseMit"
}
if (-not (Test-Path -LiteralPath $licenseApache -PathType Leaf)) {
    Fail "Missing $licenseApache"
}

$examplesSrc = Join-Path $RepoRoot 'examples'
if (-not (Test-Path -LiteralPath $examplesSrc -PathType Container)) {
    Fail "Missing examples/ folder at $examplesSrc"
}

$pipelinesSrc = Join-Many -Parts $RepoRoot, 'crates', 'pipe-python', 'python', 'pipelines'
if (-not (Test-Path -LiteralPath $pipelinesSrc -PathType Container)) {
    Fail "Missing pipelines helper package at $pipelinesSrc"
}
$pipelinesInit = Join-Path $pipelinesSrc '__init__.py'
$pipelinesStage = Join-Path $pipelinesSrc 'stage.py'
if (-not (Test-Path -LiteralPath $pipelinesInit -PathType Leaf)) {
    Fail "Missing $pipelinesInit"
}
if (-not (Test-Path -LiteralPath $pipelinesStage -PathType Leaf)) {
    Fail "Missing $pipelinesStage"
}

if (Test-Path -LiteralPath $stage) {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
New-Item -ItemType Directory -Path $stage | Out-Null

Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'pipe.exe')
Copy-Item -LiteralPath $examplesSrc -Destination (Join-Path $stage 'examples') -Recurse
Copy-Item -LiteralPath $pipelinesSrc -Destination (Join-Path $stage 'pipelines') -Recurse
Remove-PythonBytecode -Root (Join-Path $stage 'examples')
Remove-PythonBytecode -Root (Join-Path $stage 'pipelines')
Copy-TextAsCrlf -Source $readmeTemplate -Dest (Join-Path $stage 'README.txt')
Copy-Item -LiteralPath $licenseMit -Destination (Join-Path $stage 'LICENSE-MIT')
Copy-Item -LiteralPath $licenseApache -Destination (Join-Path $stage 'LICENSE-APACHE')

if ($regina) {
    Copy-Item -LiteralPath $regina['Dll'] -Destination (Join-Path $stage 'regina.dll')
    Copy-Item -LiteralPath $regina['Mtb'] -Destination (Join-Path $stage 'en.mtb')
    Copy-Item -LiteralPath $regina['License'] -Destination (Join-Path $stage 'LICENSE-Regina.txt')
}

foreach ($dll in $mingwDlls) {
    Copy-Item -LiteralPath $dll -Destination (Join-Path $stage (Split-Path -Leaf $dll))
}

New-ZipFromDirectory -SourceDir $stage -ZipPath $zipPath

Write-Host "Wrote $zipPath"
Write-Host "Zip contents:"
Get-StagedFileNames -Root $stage | ForEach-Object { Write-Host "  $_" }
Write-Host "Staging directory: $stage"
