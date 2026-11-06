# Read the Cedar Cove nested documents and write the related CSV tables.
# Safe to start from any directory:
#
#   powershell -ExecutionPolicy Bypass -File .\examples\marina\to-csv.ps1
#
# The pipeline itself is examples\marina\to-csv.pipe. It reads
# examples\marina\vessels.jsonl and writes four files under
# examples\marina\out\. Python is not required for this direction.
# Windows PowerShell 5.1 and pwsh: no ??, no ternary, no &&.

$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
if (-not $here) {
    $here = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$root = Split-Path -Parent (Split-Path -Parent $here)
$outDir = Join-Path -Path $root -ChildPath 'examples/marina/out'
if (-not (Test-Path -LiteralPath $outDir)) {
    New-Item -ItemType Directory -Path $outDir | Out-Null
}

function Test-Leaf {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $false
    }
    return [bool](Test-Path -LiteralPath $Path -PathType Leaf)
}

$pipe = $null
$kind = $null
$candidates = @(
    (Join-Path -Path $root -ChildPath 'pipe.exe'),
    (Join-Path -Path $root -ChildPath 'pipe'),
    (Join-Path -Path $root -ChildPath 'target/release/pipe.exe'),
    (Join-Path -Path $root -ChildPath 'target/release/pipe'),
    (Join-Path -Path $root -ChildPath 'target/debug/pipe.exe'),
    (Join-Path -Path $root -ChildPath 'target/debug/pipe'),
    (Join-Path -Path $root -ChildPath 'target/x86_64-pc-windows-gnu/release/pipe.exe'),
    (Join-Path -Path $root -ChildPath 'target/x86_64-pc-windows-msvc/release/pipe.exe')
)
foreach ($candidate in $candidates) {
    if (Test-Leaf $candidate) {
        $pipe = $candidate
        $kind = 'exe'
        break
    }
}
if (-not $pipe) {
    $cmd = Get-Command pipe -ErrorAction SilentlyContinue
    if ($cmd) {
        $pipe = 'pipe'
        $kind = 'exe'
    }
}
if (-not $pipe) {
    $cargoToml = Join-Path -Path $root -ChildPath 'Cargo.toml'
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($cargo -and (Test-Leaf $cargoToml)) {
        $pipe = 'cargo'
        $kind = 'cargo'
    }
}
if (-not $pipe) {
    [Console]::Error.WriteLine('Cannot find pipe.exe, pipe, a build under target/, or cargo.')
    exit 1
}

$specPath = Join-Path -Path $here -ChildPath 'to-csv.pipe'
$spec = (Get-Content -LiteralPath $specPath -Raw).Trim()

Push-Location -LiteralPath $root
try {
    if ($kind -eq 'cargo') {
        & cargo run -q -p pipe-cli -- $spec
    }
    else {
        & $pipe $spec
    }
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
}

Write-Host "Wrote CSV files in $outDir"
