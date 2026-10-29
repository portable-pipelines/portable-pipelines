# Generate a temp file set for the streaming CALLPIPE demos.
# Windows PowerShell 5.1 and pwsh: no ??, no ternary, no &&, Join-Path
# takes two arguments only.
#
#   powershell -ExecutionPolicy Bypass -File .\examples\gen-callpipe-files.ps1
#   powershell -ExecutionPolicy Bypass -File .\examples\gen-callpipe-files.ps1 -Files 50 -Lines 4 -Width 48
#
# From Command Prompt:
#   examples\gen-callpipe-files.cmd -Files 50 -Lines 4 -Width 48
#
# Writes under the process temp directory (outside the repo) by default.
# Prints OUTDIR, PATHS, and COMMANDS so you can paste them into pipe.

[CmdletBinding()]
param(
    [int]$Files = 50,
    [int]$Lines = 4,
    [int]$Width = 48,
    [string]$OutDir = ''
)

Set-StrictMode -Version 1
$ErrorActionPreference = 'Stop'

if ($Files -lt 1) {
    [Console]::Error.WriteLine('Files must be at least 1.')
    exit 1
}
if ($Lines -lt 1) {
    [Console]::Error.WriteLine('Lines must be at least 1.')
    exit 1
}
if ($Width -lt 1) {
    [Console]::Error.WriteLine('Width must be at least 1.')
    exit 1
}

if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath 'pipe-callpipe-files'
}

if ($OutDir -match '#') {
    [Console]::Error.WriteLine('OutDir must not contain # (used as a delimiter in some nested specs).')
    exit 1
}
if ($OutDir -match '\|') {
    [Console]::Error.WriteLine('OutDir must not contain |.')
    exit 1
}

$filesDir = Join-Path -Path $OutDir -ChildPath 'files'
if (Test-Path -LiteralPath $OutDir) {
    Remove-Item -LiteralPath $OutDir -Recurse -Force
}
New-Item -ItemType Directory -Path $filesDir | Out-Null

$utf8 = New-Object System.Text.UTF8Encoding $false
$pathLines = New-Object System.Collections.Generic.List[string]
$cmdLines = New-Object System.Collections.Generic.List[string]

function New-RecordText {
    param(
        [int]$FileNum,
        [int]$LineNum,
        [int]$LineWidth
    )
    $prefix = 'file={0:D4} line={1:D4} ' -f $FileNum, $LineNum
    if ($prefix.Length -ge $LineWidth) {
        return $prefix
    }
    return $prefix + ('x' * ($LineWidth - $prefix.Length))
}

$n = 1
while ($n -le $Files) {
    $name = 'f{0:D4}.txt' -f $n
    $filePath = Join-Path -Path $filesDir -ChildPath $name
    $writer = New-Object System.IO.StreamWriter($filePath, $false, $utf8)
    try {
        $ln = 1
        while ($ln -le $Lines) {
            $writer.WriteLine((New-RecordText -FileNum $n -LineNum $ln -LineWidth $Width))
            $ln += 1
        }
    }
    finally {
        $writer.Close()
    }
    $full = [System.IO.Path]::GetFullPath($filePath)
    [void]$pathLines.Add($full)
    [void]$cmdLines.Add(('callpipe < {0} | xlate 1-* upper | *:' -f $full))
    $n += 1
}

$pathsFile = Join-Path -Path $OutDir -ChildPath 'paths.txt'
$cmdsFile = Join-Path -Path $OutDir -ChildPath 'commands.txt'
[System.IO.File]::WriteAllLines($pathsFile, $pathLines.ToArray(), $utf8)
[System.IO.File]::WriteAllLines($cmdsFile, $cmdLines.ToArray(), $utf8)

$outFull = [System.IO.Path]::GetFullPath($OutDir)
$pathsFull = [System.IO.Path]::GetFullPath($pathsFile)
$cmdsFull = [System.IO.Path]::GetFullPath($cmdsFile)

Write-Output ('OUTDIR={0}' -f $outFull)
Write-Output ('PATHS={0}' -f $pathsFull)
Write-Output ('COMMANDS={0}' -f $cmdsFull)
Write-Output ('FILES={0}' -f $Files)
Write-Output ('LINES={0}' -f $Lines)
Write-Output ('WIDTH={0}' -f $Width)
Write-Output ('BYTES_HINT={0}' -f ($Files * $Lines * ($Width + 1)))
