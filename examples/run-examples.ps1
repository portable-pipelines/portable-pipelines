# Guided tour of Portable Pipelines. Works from a Windows zip, a Unix tarball,
# or a git clone. Safe to start from any current directory:
#
#   powershell -ExecutionPolicy Bypass -File .\examples\run-examples.ps1
#
# Windows PowerShell 5.1 and pwsh: no ??, no ternary, no &&, Join-Path takes
# two arguments only.

Set-StrictMode -Version 1
$ErrorActionPreference = 'Continue'

$here = $PSScriptRoot
if (-not $here) {
    $here = Split-Path -Parent $MyInvocation.MyCommand.Path
}
. (Join-Path -Path $here -ChildPath 'resolve-pipe.ps1')

function Test-PythonAtLeast310 {
    param(
        [string]$Exe,
        [string[]]$PrefixArgs
    )
    if ([string]::IsNullOrWhiteSpace($Exe)) {
        return $false
    }
    $argList = @()
    if ($PrefixArgs) {
        $argList += $PrefixArgs
    }
    $argList += '-c'
    $argList += 'import sys; v=sys.version_info; raise SystemExit(0 if (v[0]>3) or (v[0]==3 and v[1]>=10) else 1)'
    try {
        & $Exe @argList 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) {
            return $true
        }
    }
    catch {
    }
    return $false
}

function Test-IsWindowsOS {
    if ($PSVersionTable.PSVersion.Major -ge 6) {
        return [bool]$IsWindows
    }
    return $true
}

function Test-PythonReady {
    $envPy = [Environment]::GetEnvironmentVariable('PIPE_PYTHON')
    if (-not [string]::IsNullOrWhiteSpace($envPy)) {
        if (Test-Path -LiteralPath $envPy -PathType Container) {
            $names = @('python.exe', 'python3.exe', 'python3', 'python')
            foreach ($name in $names) {
                $cand = Join-Path -Path $envPy -ChildPath $name
                if (Test-PythonAtLeast310 -Exe $cand) {
                    return $true
                }
            }
        }
        elseif (Test-PythonAtLeast310 -Exe $envPy) {
            return $true
        }
    }

    if (Test-IsWindowsOS) {
        $py = Get-Command py -ErrorAction SilentlyContinue
        if ($py) {
            if (Test-PythonAtLeast310 -Exe 'py' -PrefixArgs @('-3')) {
                return $true
            }
        }
    }

    foreach ($name in @('python3', 'python')) {
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if (-not $cmd) {
            continue
        }
        $path = $name
        if ($null -ne $cmd.PSObject.Properties['Path'] -and $cmd.Path) {
            $path = [string]$cmd.Path
        }
        elseif ($null -ne $cmd.PSObject.Properties['Source'] -and $cmd.Source) {
            $path = [string]$cmd.Source
        }
        if (Test-PythonAtLeast310 -Exe $path) {
            return $true
        }
    }
    return $false
}

$script:HavePython = Test-PythonReady
if (-not $script:HavePython) {
    Write-Host 'Python 3.10 or later was not found. Python demos will be skipped (install Python, or set PIPE_PYTHON).'
}

$script:Passed = 0
$script:Skipped = 0
$script:Failed = 0

function Show-Example {
    param(
        [Parameter(Mandatory = $true)][string]$Spec,
        [Parameter(Mandatory = $true)][string]$Expected,
        [switch]$NeedPython
    )

    Write-Host ''
    Write-Host ('Pipeline: ' + $Spec) -ForegroundColor Cyan
    Write-Host ('Expected: ' + $Expected)

    if ($NeedPython -and -not $script:HavePython) {
        Write-Host 'Skipped: needs Python 3.10 or later on PATH (or PIPE_PYTHON).'
        $script:Skipped += 1
        return
    }

    Invoke-ResolvedPipe -Spec $Spec
    $code = $LASTEXITCODE
    if ($null -eq $code) {
        $code = 0
    }
    if ($code -ne 0) {
        Write-Host ("(exit $code)") -ForegroundColor Yellow
        $script:Failed += 1
    }
    else {
        $script:Passed += 1
    }
}

Write-Host ''
Write-Host 'Built-in stages'
Write-Host '---------------'

Show-Example `
    -Spec 'literal hello| console' `
    -Expected 'hello'

Show-Example `
    -Spec '< examples/lines.txt | locate /todo/ | console' `
    -Expected 'todo: one, then todo: two'

Show-Example `
    -Spec 'examples/emit.rexx ax b x | nlocate /x/ | console' `
    -Expected 'b (records that do not contain x)'

Show-Example `
    -Spec '(ENDCHAR ?) examples/emit.rexx a b c d | T: take 2 | console ? T: | hole' `
    -Expected 'a, then b (remainder goes to hole)'

Show-Example `
    -Spec 'examples/emit.rexx a b c d | drop 2 | console' `
    -Expected 'c, then d'

Show-Example `
    -Spec 'literal   aa   | strip | console' `
    -Expected 'aa (leading and trailing blanks removed)'

Show-Example `
    -Spec 'literal xx| change /xx/ /yy/ | console' `
    -Expected 'yy'

Show-Example `
    -Spec 'literal c|literal a|literal b| sort | console' `
    -Expected 'a, then b, then c'

Show-Example `
    -Spec 'examples/emit.rexx a b c | count lines | console' `
    -Expected '3'

Write-Host ''
Write-Host 'REXX stage'
Write-Host '----------'

Show-Example `
    -Spec 'literal hello| examples/upper.rexx | console' `
    -Expected 'HELLO'

Write-Host ''
Write-Host 'Python stages'
Write-Host '-------------'

Show-Example `
    -Spec 'literal hello| examples/upper.py | console' `
    -Expected 'HELLO' `
    -NeedPython

Show-Example `
    -Spec 'examples/emit.py apple banana apricot | examples/upper.py | console' `
    -Expected 'APPLE, BANANA, APRICOT' `
    -NeedPython

Show-Example `
    -Spec 'disk examples/lines.txt | examples/wordfreq.py 5 | console' `
    -Expected '2 todo, then one each of apple, banana, apricot, spaced' `
    -NeedPython

Write-Host ''
Write-Host 'REXX and Python together'
Write-Host '------------------------'

Show-Example `
    -Spec 'examples/emit.rexx a b c | examples/upper.py | console' `
    -Expected 'A, then B, then C' `
    -NeedPython

Write-Host ''
Write-Host 'Multistream'
Write-Host '-----------'

Show-Example `
    -Spec '(ENDCHAR ?) examples/emit.rexx apple banana apricot | L: locate /ap/ | console ? L: | examples/prefix.rexx miss= | console' `
    -Expected 'apple, apricot, and miss=banana on the other stream'

Show-Example `
    -Spec '(end ?) < examples/users.txt | a: strfind /USER / | spec 1-* 1 / / next | j: juxtapose | console ? a: | j:' `
    -Expected 'USER AAA MDISK 191, USER AAA MDISK 192, USER AAA MDISK 193, USER BBB MDISK 191, USER BBB MDISK 192'

Write-Host ''
Write-Host ("Tour finished: {0} ran, {1} skipped (Python), {2} failed." -f $script:Passed, $script:Skipped, $script:Failed)
if ($script:Failed -ne 0) {
    exit 1
}
