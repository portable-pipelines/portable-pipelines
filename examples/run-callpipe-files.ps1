# Run the streaming CALLPIPE file-set demos. Works from a Windows zip, a
# Unix tarball, or a git clone. Safe to start from any current directory:
#
#   powershell -ExecutionPolicy Bypass -File .\examples\run-callpipe-files.ps1
#   powershell -ExecutionPolicy Bypass -File .\examples\run-callpipe-files.ps1 -Demo take
#   powershell -ExecutionPolicy Bypass -File .\examples\run-callpipe-files.ps1 -Way rexx
#
# Windows PowerShell 5.1 and pwsh: no ??, no ternary, no &&, Join-Path
# takes two arguments only.
#
# -Way pipcmd (default) streams on every host, including a Windows zip
# with bundled Regina. -Way rexx streams only on the subset interpreter.

[CmdletBinding()]
param(
    [ValidateSet('all', 'small', 'large', 'take')]
    [string]$Demo = 'all',
    [ValidateSet('pipcmd', 'rexx')]
    [string]$Way = 'pipcmd'
)

Set-StrictMode -Version 1
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
if (-not $here) {
    $here = Split-Path -Parent $MyInvocation.MyCommand.Path
}
. (Join-Path -Path $here -ChildPath 'resolve-pipe.ps1')

$genScript = Join-Path -Path $here -ChildPath 'gen-callpipe-files.ps1'
$rexxRel = 'examples/callpipe_files.rexx'

function Invoke-Generator {
    param(
        [int]$Files,
        [int]$Lines,
        [int]$Width
    )
    Write-Host ''
    Write-Host ('Generating {0} files, {1} lines, width {2} ...' -f $Files, $Lines, $Width)
    $raw = & $genScript -Files $Files -Lines $Lines -Width $Width
    $map = @{}
    foreach ($line in $raw) {
        $eq = $line.IndexOf('=')
        if ($eq -gt 0) {
            $map[$line.Substring(0, $eq)] = $line.Substring($eq + 1)
        }
        Write-Host $line
    }
    return $map
}

function Show-EngineNote {
    Write-Host ''
    if ($Way -eq 'pipcmd') {
        Write-Host 'Way: PIPCMD. CALLPIPE streams (live StageCtx) on Windows, Linux, and macOS.'
    }
    else {
        Write-Host 'Way: REXX. Streams on the subset interpreter. A Regina stage (Windows zip, or libregina on PATH) still drains, runs, then flushes.'
        Write-Host 'There is no supported switch to force the subset interpreter.'
    }
}

function Invoke-DemoSpec {
    param(
        [string]$Title,
        [string]$Spec,
        [string]$Expect
    )
    Write-Host ''
    Write-Host "=== $Title ==="
    Write-Host "Pipeline: $Spec"
    Write-Host "Expected: $Expect"
    Invoke-ResolvedPipe -Spec $Spec
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        Write-Host ("pipe exit {0}" -f $code)
        exit $code
    }
}

Show-EngineNote

$runSmall = ($Demo -eq 'all' -or $Demo -eq 'small')
$runLarge = ($Demo -eq 'all' -or $Demo -eq 'large')
$runTake = ($Demo -eq 'all' -or $Demo -eq 'take')

if ($runSmall -or $runTake) {
    $small = Invoke-Generator -Files 50 -Lines 4 -Width 48
}

if ($runSmall) {
    if ($Way -eq 'pipcmd') {
        $spec = '< {0} | pipcmd | console' -f $small['COMMANDS']
    }
    else {
        $spec = '< {0} | {1} | console' -f $small['PATHS'], $rexxRel
    }
    Invoke-DemoSpec -Title 'a) many small files' -Spec $spec -Expect '200 uppercase records, FILE=0001 first, FILE=0050 last'
}

if ($runLarge) {
    $large = Invoke-Generator -Files 3 -Lines 20000 -Width 120
    Write-Host ''
    Write-Host 'Memory: Task Manager on Windows (Details, pipe.exe, Memory). On Linux, wrap the same command with /usr/bin/time -v if you have GNU time, or watch RSS in another terminal.'
    Write-Host 'Streaming stays near a few records. Buffering would hold the whole file set (BYTES_HINT above).'
    if ($Way -eq 'pipcmd') {
        $spec = '< {0} | pipcmd | console' -f $large['COMMANDS']
    }
    else {
        $spec = '< {0} | {1} | console' -f $large['PATHS'], $rexxRel
    }
    Invoke-DemoSpec -Title 'b) a few large files' -Spec $spec -Expect '60000 uppercase records; RSS stays far under BYTES_HINT if CALLPIPE is streaming'
}

if ($runTake) {
    if ($Way -eq 'pipcmd') {
        $spec = '< {0} | pipcmd stop anyeof | take 10 | console' -f $small['COMMANDS']
        $expect = 'exactly 10 records (files 1 and 2 in full, then FILE=0003 LINE=0001 and LINE=0002). Files 4 through 50 are not opened.'
    }
    else {
        $spec = '< {0} | {1} | take 10 | console' -f $small['PATHS'], $rexxRel
        $expect = 'exactly 10 records. On the subset interpreter the remaining files are not read. Under Regina every path is still drained first, then 10 records are flushed.'
    }
    Invoke-DemoSpec -Title 'c) take 10 stops early' -Spec $spec -Expect $expect
}

Write-Host ''
Write-Host 'Done. See examples/README.md for the paste-ready Windows and Unix commands.'
