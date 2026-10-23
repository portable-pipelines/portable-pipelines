# Multistream examples with stage labels and (ENDCHAR ?).
# Works from a Windows zip, a Unix tarball, or a git clone. Safe to start
# from any current directory:
#   powershell -File ./examples/multistream.ps1
#
# First `L:` defines the stage and its primary streams.
# Later `L:` attaches that same stage's secondary stream.

$ErrorActionPreference = "Continue"

$here = $PSScriptRoot
if (-not $here) {
    $here = Split-Path -Parent $MyInvocation.MyCommand.Path
}
. (Join-Path -Path $here -ChildPath 'resolve-pipe.ps1')

function Invoke-Pipe([string]$Spec) {
    Write-Host ""
    Write-Host ">>> $Spec" -ForegroundColor Cyan
    Invoke-ResolvedPipe -Spec $Spec -Trace
    $code = $LASTEXITCODE
    if ($null -eq $code) {
        $code = 0
    }
    if ($code -ne 0) {
        Write-Host "(exit $code)" -ForegroundColor Yellow
    }
}

Write-Host "=== LOCATE: matches → primary, misses → secondary ===" -ForegroundColor Green
# Secondary is prefixed so you can tell the streams apart when both print.
# Do not end the prefix with ':' — that would be parsed as a stage label.
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx apple banana apricot | L: locate /ap/ | console ? L: | ./examples/prefix.rexx miss= | console"

Write-Host "=== NLOCATE: non-matches → primary, hits → secondary ===" -ForegroundColor Green
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx ax b x | L: nlocate /x/ | console ? L: | ./examples/prefix.rexx hit= | console"

Write-Host "=== TAKE: first 2 → primary, remainder → secondary ===" -ForegroundColor Green
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take 2 | console ? T: | ./examples/prefix.rexx rest= | console"

Write-Host "=== DROP: kept → primary, dropped → secondary ===" -ForegroundColor Green
# Both sides to console (no REXX on secondary): DROP severs the secondary after
# the dropped records; a REXX reader can treat that as a hard error.
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | D: drop 2 | console ? D: | console"

Write-Host "=== disk file: TODO lines vs the rest ===" -ForegroundColor Green
Invoke-Pipe "(ENDCHAR ?) disk ./examples/lines.txt | L: locate /todo/ | console ? L: | ./examples/prefix.rexx other= | console"

Write-Host ""
Write-Host "Multistream demos finished." -ForegroundColor Green
