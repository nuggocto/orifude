param(
    [string]$Version = "0.2.0",
    [string]$InstallDirectory = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ReleaseVersion = "0.2.0"
$Repository = "nuggocto/orifude"
if ($Version -cne $ReleaseVersion) {
    throw "This installer only supports v$ReleaseVersion."
}
if (-not $InstallDirectory) {
    $InstallDirectory = if ($env:ORIFUDE_INSTALL_DIR) {
        $env:ORIFUDE_INSTALL_DIR
    } else {
        Join-Path $env:LOCALAPPDATA "Programs\orifude\bin"
    }
}

$Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
$Arch = switch ($Architecture) {
    "X64" { "amd64" }
    "Arm64" { "arm64" }
    default { throw "Unsupported architecture: $Architecture" }
}

$Archive = "orifude_${Version}_windows_${Arch}.zip"
$ReleaseUrl = "https://github.com/${Repository}/releases/download/v${Version}"
$Temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("orifude-install-" + [guid]::NewGuid())
$ArchivePath = Join-Path $Temporary $Archive
$ChecksumPath = Join-Path $Temporary "checksums.txt"

try {
    New-Item -ItemType Directory -Path $Temporary | Out-Null
    Invoke-WebRequest -UseBasicParsing -Uri "${ReleaseUrl}/${Archive}" -OutFile $ArchivePath
    Invoke-WebRequest -UseBasicParsing -Uri "${ReleaseUrl}/checksums.txt" -OutFile $ChecksumPath

    $MatchesForArchive = @(Get-Content -LiteralPath $ChecksumPath | Where-Object {
        $_ -match "^([0-9a-f]{64})  $([regex]::Escape($Archive))$"
    })
    if ($MatchesForArchive.Count -ne 1) {
        throw "Release checksum is missing, malformed, or ambiguous."
    }
    $Expected = ($MatchesForArchive[0] -split "  ", 2)[0]
    $Actual = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -cne $Expected) {
        throw "Checksum verification failed."
    }

    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $Temporary
    $Binary = Join-Path $Temporary "orifude.exe"
    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
        throw "Verified archive does not contain orifude.exe."
    }

    New-Item -ItemType Directory -Force -Path $InstallDirectory | Out-Null
    $Staged = Join-Path $InstallDirectory (".orifude." + [guid]::NewGuid() + ".tmp")
    Copy-Item -LiteralPath $Binary -Destination $Staged
    Move-Item -Force -LiteralPath $Staged -Destination (Join-Path $InstallDirectory "orifude.exe")

    Write-Host "Installed Orifude v$Version to $InstallDirectory\orifude.exe"
} finally {
    if (Test-Path -LiteralPath $Temporary) {
        Remove-Item -Recurse -Force -LiteralPath $Temporary
    }
}
