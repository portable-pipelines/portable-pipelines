# Phase 1 built-in stage demos (TAKE, DROP, NLOCATE, STRIP).
# Run from the repo root:
#   powershell -File ./examples/phase1.ps1
#
# Uses ./examples/emit.rexx (or disk ./examples/lines.txt) so several records
# stay in argument order (a LITERAL cascade writes in reverse order).

$ErrorActionPreference = "Continue"

# Prefer `cargo run` so demos always match the sources on this branch
# (a stale target\release\pipe.exe will miss TAKE/DROP/NLOCATE/STRIP).
function Invoke-Pipe([string]$Spec) {
    Write-Host ""
    Write-Host ">>> $Spec" -ForegroundColor Cyan
    & cargo run -q -p pipe-cli -- $Spec
    if ($LASTEXITCODE -ne 0) {
        Write-Host "(exit $LASTEXITCODE)" -ForegroundColor Yellow
    }
}

# --- TAKE -----------------------------------------------------------------
# TAKE FIRST stops early when the secondary is absent; attach it to hole so
# the producer can finish cleanly (IBM multistream pattern).
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take | console ? T: | hole"
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take 2 | console ? T: | hole"
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take first 3 | console ? T: | hole"
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take 2 first | console ? T: | hole"
Invoke-Pipe "./examples/emit.rexx a b c d | take last 2 | console"
Invoke-Pipe "./examples/emit.rexx a b c d | take last * | console"
Invoke-Pipe "./examples/emit.rexx a b c d | take * | console"
Invoke-Pipe "(ENDCHAR ?) ./examples/emit.rexx a b c d | T: take 0 | console ? T: | hole"

# --- DROP -----------------------------------------------------------------
Invoke-Pipe "./examples/emit.rexx a b c d | drop | console"
Invoke-Pipe "./examples/emit.rexx a b c d | drop 2 | console"
Invoke-Pipe "./examples/emit.rexx a b c d | drop first 1 | console"
Invoke-Pipe "./examples/emit.rexx a b c d | drop last 2 | console"
Invoke-Pipe "./examples/emit.rexx a b c d | drop * | console"
Invoke-Pipe "./examples/emit.rexx a b c d | drop 0 | console"

# --- NLOCATE / NOTLOCATE --------------------------------------------------
Invoke-Pipe "./examples/emit.rexx ax b x | nlocate /x/ | console"
Invoke-Pipe "disk ./examples/lines.txt | nlocate | console"
Invoke-Pipe "./examples/emit.rexx apple banana apricot | notlocate /ap/ | console"
Invoke-Pipe "disk ./examples/lines.txt | nlocate /todo/ | console"

# --- STRIP ----------------------------------------------------------------
Invoke-Pipe "literal   aa   | strip | console"
Invoke-Pipe "literal   aa   | strip leading | console"
Invoke-Pipe "literal   aa   | strip trailing | console"
Invoke-Pipe "literal   aa   | strip both | console"
Invoke-Pipe "literal   aa   | strip 1 | console"
Invoke-Pipe "literal xxyzz | strip anyof /xy/ | console"
Invoke-Pipe "literal aaa | strip leading anyof /a/ 2 | console"
Invoke-Pipe "literal AaA | strip anycase anyof /a/ | console"
Invoke-Pipe "literal a b | strip not | console"
Invoke-Pipe "literal a.b. | strip trailing . | console"
Invoke-Pipe "literal   spaced   | strip blank | console"

# --- Exit-criteria cascade ------------------------------------------------
Invoke-Pipe "disk ./examples/lines.txt | take 20 | nlocate /x/ | strip | console"

Write-Host ""
Write-Host "Phase 1 demos finished." -ForegroundColor Green
