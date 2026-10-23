# Resolve pipe for examples/*.ps1 (Windows zip, Unix tarball, or a git clone).
# Dot-source this file from the same folder:
#   . (Join-Path $PSScriptRoot 'resolve-pipe.ps1')
#
# Order:
#   1. pipe.exe (then pipe) next to the examples folder  (zip / tarball)
#   2. pipe on PATH
#   3. target\x86_64-pc-windows-gnu\release\pipe.exe
#      or target\release\pipe.exe / target\release\pipe  (repo checkout)
#   4. cargo run -q -p pipe-cli --
#
# Windows PowerShell 5.1 and pwsh: no ??, no ternary, no &&, Join-Path takes
# two arguments only.

Set-StrictMode -Version 1

function Join-Many {
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

function Get-ExamplesDir {
    $here = $PSScriptRoot
    if (-not $here) {
        $here = Split-Path -Parent $MyInvocation.MyCommand.Path
    }
    return $here
}

function Test-Leaf {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $false
    }
    return [bool](Test-Path -LiteralPath $Path -PathType Leaf)
}

function Get-CommandPath {
    param([string]$Name)
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $cmd) {
        return $null
    }
    if ($null -ne $cmd.PSObject.Properties['Path'] -and $cmd.Path) {
        return [string]$cmd.Path
    }
    if ($null -ne $cmd.PSObject.Properties['Source'] -and $cmd.Source) {
        return [string]$cmd.Source
    }
    return $Name
}

function Resolve-PortablePipe {
    $examples = Get-ExamplesDir
    $root = Split-Path -Parent $examples

    $zipExe = Join-Path -Path $root -ChildPath 'pipe.exe'
    if (Test-Leaf $zipExe) {
        return @{
            Kind    = 'exe'
            Path    = (Resolve-Path -LiteralPath $zipExe).Path
            WorkDir = $root
            Label   = "pipe.exe next to examples: $zipExe"
        }
    }

    $zipBin = Join-Path -Path $root -ChildPath 'pipe'
    if (Test-Leaf $zipBin) {
        return @{
            Kind    = 'exe'
            Path    = (Resolve-Path -LiteralPath $zipBin).Path
            WorkDir = $root
            Label   = "pipe next to examples: $zipBin"
        }
    }

    $pathPipe = Get-CommandPath -Name 'pipe'
    if ($pathPipe) {
        return @{
            Kind    = 'exe'
            Path    = $pathPipe
            WorkDir = $root
            Label   = "pipe on PATH: $pathPipe"
        }
    }

    $gnuExe = Join-Many -Parts $root, 'target', 'x86_64-pc-windows-gnu', 'release', 'pipe.exe'
    if (Test-Leaf $gnuExe) {
        return @{
            Kind    = 'exe'
            Path    = (Resolve-Path -LiteralPath $gnuExe).Path
            WorkDir = $root
            Label   = "repo GNU pipe.exe: $gnuExe"
        }
    }

    $relExe = Join-Many -Parts $root, 'target', 'release', 'pipe.exe'
    if (Test-Leaf $relExe) {
        return @{
            Kind    = 'exe'
            Path    = (Resolve-Path -LiteralPath $relExe).Path
            WorkDir = $root
            Label   = "repo pipe.exe: $relExe"
        }
    }

    $relBin = Join-Many -Parts $root, 'target', 'release', 'pipe'
    if (Test-Leaf $relBin) {
        return @{
            Kind    = 'exe'
            Path    = (Resolve-Path -LiteralPath $relBin).Path
            WorkDir = $root
            Label   = "repo pipe: $relBin"
        }
    }

    $cargoToml = Join-Path -Path $root -ChildPath 'Cargo.toml'
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($cargo -and (Test-Leaf $cargoToml)) {
        return @{
            Kind    = 'cargo'
            Path    = 'cargo'
            WorkDir = $root
            Label   = 'cargo run -q -p pipe-cli --'
        }
    }

    [Console]::Error.WriteLine("Cannot find pipe.exe, pipe, a release build under target/, or cargo. Looked next to $examples and under $root.")
    exit 1
}

function Invoke-ResolvedPipe {
    param(
        [Parameter(Mandatory = $true)][string]$Spec,
        [switch]$Trace
    )
    $info = $script:PipeInfo
    Push-Location -LiteralPath $info.WorkDir
    try {
        # Do not capture or return stdout: a PowerShell function's output
        # stream would swallow pipe's records. Callers check $LASTEXITCODE.
        if ($info.Kind -eq 'cargo') {
            $cargoArgs = @('run', '-q', '-p', 'pipe-cli', '--')
            if ($Trace) {
                $cargoArgs += '--trace'
            }
            $cargoArgs += $Spec
            & cargo @cargoArgs
        }
        else {
            $pipeArgs = @()
            if ($Trace) {
                $pipeArgs += '--trace'
            }
            $pipeArgs += $Spec
            & $info.Path @pipeArgs
        }
    }
    finally {
        Pop-Location
    }
}

$script:PipeInfo = Resolve-PortablePipe
Write-Host ("Using pipe: " + $script:PipeInfo.Label)
