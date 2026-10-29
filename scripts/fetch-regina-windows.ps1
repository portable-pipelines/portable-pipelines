<#
.SYNOPSIS
    Download the official Regina REXX Windows x64 zip and extract it.

.DESCRIPTION
    Fetches Regina 3.9.7 for 64-bit Windows from SourceForge (the same
    Regina397w64.zip a maintainer would install by hand), checks the SHA-256
    published on that SourceForge file page, and extracts it. Does not commit
    the zip or DLLs; output is a local directory you pass to
    package-windows.ps1 -ReginaDir.

    Source (do not vendor this binary in git):
      https://sourceforge.net/projects/regina-rexx/files/regina-rexx/3.9.7/Regina397w64.zip/download
    File: Regina397w64.zip
    SHA-256: 7cc7993bcbcabea57e38f17468ad94081503b4d1b1685964891567b093860add
    (from the SourceForge download page for that file)

    Windows PowerShell 5.1 and PowerShell 7 are both supported.

.PARAMETER OutDir
    Directory that will hold the downloaded zip and an extract\ folder.
    Default: <repo>/dist/regina-windows (gitignored with the rest of dist/).

.EXAMPLE
    powershell -File .\scripts\fetch-regina-windows.ps1
    powershell -File .\scripts\package-windows.ps1 -ReginaDir .\dist\regina-windows\extract
#>
[CmdletBinding()]
param(
    [string]$OutDir
)

Set-StrictMode -Version 1
$ErrorActionPreference = 'Stop'

# Official Regina 3.9.7 Windows x64 zip. Hash is from the SourceForge file page.
# Try the project /download URL first, then the downloads.sourceforge.net
# content URL if a mirror returns an HTML interstitial instead of the zip.
$ReginaUrls = @(
    'https://sourceforge.net/projects/regina-rexx/files/regina-rexx/3.9.7/Regina397w64.zip/download',
    'https://downloads.sourceforge.net/project/regina-rexx/regina-rexx/3.9.7/Regina397w64.zip'
)
$ReginaZipName = 'Regina397w64.zip'
$ExpectedSha256 = '7cc7993bcbcabea57e38f17468ad94081503b4d1b1685964891567b093860add'
$MinZipBytes = 1000000

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

function Get-CommandPath {
    param([string[]]$Names)
    foreach ($name in $Names) {
        $cmd = Get-Command $name -ErrorAction SilentlyContinue
        if (-not $cmd) {
            continue
        }
        if ($null -ne $cmd.PSObject.Properties['Path'] -and $cmd.Path) {
            return [string]$cmd.Path
        }
        if ($null -ne $cmd.PSObject.Properties['Source'] -and $cmd.Source) {
            return [string]$cmd.Source
        }
    }
    return $null
}

function Save-UrlToFile {
    param(
        [string]$Url,
        [string]$Dest
    )
    $curl = Get-CommandPath -Names @('curl.exe')
    if ($curl) {
        Write-Host "Downloading with curl.exe:"
        Write-Host "  $Url"
        & $curl -L --fail --retry 5 --retry-delay 2 `
            -A 'portable-pipelines-fetch-regina' `
            -o $Dest `
            $Url
        if ($LASTEXITCODE -ne 0) {
            throw "curl.exe failed with exit code $LASTEXITCODE while downloading Regina from $Url"
        }
        return
    }

    Write-Host "Downloading with Invoke-WebRequest:"
    Write-Host "  $Url"
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    }
    catch {
        # Some hosts already enable TLS 1.2; ignore if the assignment fails.
    }
    Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing -MaximumRedirection 8
}

function Test-ZipMagic {
    param([string]$Path)
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $b0 = $stream.ReadByte()
        $b1 = $stream.ReadByte()
    }
    finally {
        $stream.Close()
    }
    return ($b0 -eq 0x50 -and $b1 -eq 0x4B)
}

$RepoRoot = Get-RepoRoot
if (-not $OutDir) {
    $OutDir = Join-Path (Join-Path $RepoRoot 'dist') 'regina-windows'
}
elseif (-not [System.IO.Path]::IsPathRooted($OutDir)) {
    $OutDir = Join-Path (Get-Location).Path $OutDir
}

if (-not (Test-Path -LiteralPath $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}
$OutDir = (Resolve-Path -LiteralPath $OutDir).Path
$zipPath = Join-Path $OutDir $ReginaZipName
$extractDir = Join-Path $OutDir 'extract'

Write-Host "Expected SHA-256: $ExpectedSha256"

$usedUrl = $null
$lastError = $null
foreach ($url in $ReginaUrls) {
    Write-Host "Regina source: $url"
    try {
        Save-UrlToFile -Url $url -Dest $zipPath
    }
    catch {
        $lastError = $_.Exception.Message
        Write-Host "Download failed: $lastError"
        continue
    }
    if (-not (Test-Path -LiteralPath $zipPath -PathType Leaf)) {
        $lastError = "download finished but $zipPath is missing"
        Write-Host $lastError
        continue
    }
    $length = (Get-Item -LiteralPath $zipPath).Length
    if ($length -lt $MinZipBytes) {
        $lastError = "file is only $length bytes (need at least $MinZipBytes); likely an HTML page"
        Write-Host $lastError
        continue
    }
    if (-not (Test-ZipMagic -Path $zipPath)) {
        $lastError = 'file is not a ZIP (missing PK magic); likely an HTML page'
        Write-Host $lastError
        continue
    }
    $actual = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $ExpectedSha256) {
        $lastError = "SHA-256 mismatch (actual $actual)"
        Write-Host $lastError
        continue
    }
    $usedUrl = $url
    Write-Host "SHA-256 ok: $actual"
    break
}

if (-not $usedUrl) {
    Fail "Could not download $ReginaZipName from SourceForge with SHA-256 $ExpectedSha256. Last error: $lastError"
}

if (Test-Path -LiteralPath $extractDir) {
    Remove-Item -LiteralPath $extractDir -Recurse -Force
}
New-Item -ItemType Directory -Path $extractDir | Out-Null
Expand-Archive -LiteralPath $zipPath -DestinationPath $extractDir -Force

$dll = $null
$hits = @(Get-ChildItem -LiteralPath $extractDir -Recurse -Force -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -eq 'regina.dll' -or $_.Name -eq 'regina64.dll' })
if ($hits.Count -gt 0) {
    $dll = $hits[0].FullName
}
if (-not $dll) {
    Fail "Extracted $ReginaZipName but did not find regina.dll under $extractDir."
}

Write-Host "Extracted to: $extractDir"
Write-Host "regina.dll: $dll"
Write-Host "Pass -ReginaDir `"$extractDir`" to scripts/package-windows.ps1."
