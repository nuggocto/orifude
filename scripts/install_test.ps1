$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Work = Join-Path ([System.IO.Path]::GetTempPath()) ("orifude-installer-test-" + [guid]::NewGuid())
$Fixture = Join-Path $Work "fixture"
$ArchiveSource = Join-Path $Work "archive"
$InstallDirectory = Join-Path $Work "install"

function Invoke-WebRequest {
    param(
        [switch]$UseBasicParsing,
        [string]$Uri,
        [string]$OutFile
    )

    if ($Uri.EndsWith("checksums.txt")) {
        Copy-Item -LiteralPath (Join-Path $Fixture "checksums.txt") -Destination $OutFile
    } else {
        Copy-Item -LiteralPath (Join-Path $Fixture "orifude_0.2.0_windows_amd64.zip") -Destination $OutFile
    }
}

try {
    New-Item -ItemType Directory -Path $Fixture, $ArchiveSource | Out-Null
    Set-Content -LiteralPath (Join-Path $ArchiveSource "orifude.exe") -Value "fixture binary"
    Set-Content -LiteralPath (Join-Path $ArchiveSource "LICENSE") -Value "fixture license"
    Set-Content -LiteralPath (Join-Path $ArchiveSource "README.md") -Value "fixture readme"

    $Archive = Join-Path $Fixture "orifude_0.2.0_windows_amd64.zip"
    Compress-Archive -Path (Join-Path $ArchiveSource "*") -DestinationPath $Archive
    $Hash = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $Fixture "checksums.txt") -Value "$Hash  orifude_0.2.0_windows_amd64.zip"

    & "$PSScriptRoot/install.ps1" -Version 0.2.0 -InstallDirectory $InstallDirectory
    if (-not (Test-Path -LiteralPath (Join-Path $InstallDirectory "orifude.exe") -PathType Leaf)) {
        throw "Installer did not create orifude.exe."
    }

    Remove-Item -Recurse -Force -LiteralPath $InstallDirectory
    Set-Content -LiteralPath (Join-Path $Fixture "checksums.txt") -Value "$("0" * 64)  orifude_0.2.0_windows_amd64.zip"
    try {
        & "$PSScriptRoot/install.ps1" -InstallDirectory $InstallDirectory
        throw "Installer accepted a mismatched checksum."
    } catch {
        if ($_.Exception.Message -eq "Installer accepted a mismatched checksum.") {
            throw
        }
    }
    if (Test-Path -LiteralPath (Join-Path $InstallDirectory "orifude.exe")) {
        throw "Installer persisted a checksum-mismatched binary."
    }

    Write-Host "PowerShell installer checks passed."
} finally {
    if (Test-Path -LiteralPath $Work) {
        Remove-Item -Recurse -Force -LiteralPath $Work
    }
}
