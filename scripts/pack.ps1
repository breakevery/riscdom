#!/usr/bin/env pwsh
# Package the two deployable programs (v1.0 M7b-1).
#
#   scripts\pack.ps1 [-OutputDir <dir>] [-SkipUiBuild]
#
# It builds the release binaries and assembles **two** archives in the host's own platform format:
#
#   riscdom-server-<version>-<platform>.zip   the single-node control plane
#   riscdom-relay-<version>-<platform>.zip    the connection layer's server
#
# What each package holds is docs/server-distribution.md's business; this script only builds what that
# document says. It is the **Windows twin** of `scripts/pack.sh` (the same split `gate` and `commit`
# keep, so the platform the project verifies on has a native implementation): this half writes `.zip`
# with PowerShell's own `Compress-Archive`, the other writes `.tar.gz` with the system `tar`.
#
# It never runs in CI (that is a later batch), never tags, never publishes, and never copies a data
# directory or a credential into a package: the token and the node key are minted by the programs on
# their first start, where they belong.
param(
    [string]$OutputDir = "target/dist",
    [switch]$SkipUiBuild
)

$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Write-Usage {
    Write-Host "usage: scripts\pack.ps1 [-OutputDir <dir>] [-SkipUiBuild]"
    Write-Host ""
    Write-Host "  -OutputDir <dir>   where the archives go (default: target/dist)"
    Write-Host "  -SkipUiBuild       reuse the front end already built at ui/dist/app"
}

# The version comes from the workspace, which is where `[workspace.package] version` lives: the one
# place a release bumps, so a package cannot disagree with the binaries inside it.
$versionLine = Get-Content Cargo.toml | Where-Object { $_ -match '^version = "(.+)"$' } | Select-Object -First 1
if (-not $versionLine) {
    Write-Host "pack: cannot read the version from Cargo.toml"
    exit 1
}
$version = [regex]::Match($versionLine, '^version = "(.+)"$').Groups[1].Value

$architecture = $env:PROCESSOR_ARCHITECTURE
switch ($architecture) {
    "AMD64" { $arch = "x64" }
    "ARM64" { $arch = "arm64" }
    "x86" { $arch = "x86" }
    default { $arch = if ($architecture) { $architecture.ToLower() } else { "unknown" } }
}
$platform = "win-$arch"

Write-Host "==> cargo build --release (riscdom-server, riscdom-relay)"
cargo build --release -p server --bin riscdom-server
if ($LASTEXITCODE -ne 0) { Write-Host "pack: cargo build (server) failed"; exit 1 }
cargo build --release -p net --bin riscdom-relay
if ($LASTEXITCODE -ne 0) { Write-Host "pack: cargo build (relay) failed"; exit 1 }

$webRoot = "ui/dist/app"
if (-not $SkipUiBuild) {
    Write-Host "==> npm run build (ui)"
    # `npm run build`, not `npm ci`: the gate assumes the frontend dependencies are installed, and a
    # packaging script that reached the network would be a different thing than the one the project runs.
    Push-Location ui
    npm run build
    $uiExit = $LASTEXITCODE
    Pop-Location
    if ($uiExit -ne 0) { Write-Host "pack: npm run build failed"; exit 1 }
}
if (-not (Test-Path -LiteralPath $webRoot -PathType Container)) {
    Write-Host "pack: the built front end is not at $webRoot -- build it first (drop -SkipUiBuild)"
    exit 1
}

$staging = Join-Path $OutputDir ".staging-$version"
if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
New-Item -ItemType Directory -Path $staging -Force | Out-Null

# ---- the single-node control plane ----------------------------------------
$serverName = "riscdom-server-$version-$platform"
$serverDir = Join-Path $staging $serverName
New-Item -ItemType Directory -Path (Join-Path $serverDir "web") -Force | Out-Null
Copy-Item "target/release/riscdom-server.exe" (Join-Path $serverDir "riscdom-server.exe")
Copy-Item (Join-Path $webRoot "*") (Join-Path $serverDir "web") -Recurse
Copy-Item "server/README.md" (Join-Path $serverDir "README.md")
# `File.WriteAllText` writes UTF-8 **without** a BOM, on both PowerShell 5.1 and 7 (`Set-Content
# -Encoding utf8` would add one, and a BOM is not valid JSON).
[System.IO.File]::WriteAllText((Join-Path $serverDir "settings.example.json"), "{`n  `"version`": 2`n}`n")

# ---- the connection layer's server ----------------------------------------
$relayName = "riscdom-relay-$version-$platform"
$relayDir = Join-Path $staging $relayName
New-Item -ItemType Directory -Path (Join-Path $relayDir "examples") -Force | Out-Null
Copy-Item "target/release/riscdom-relay.exe" (Join-Path $relayDir "riscdom-relay.exe")
Copy-Item "net/README.md" (Join-Path $relayDir "README.md")
[System.IO.File]::WriteAllText((Join-Path $relayDir "examples/peers.example.json"), "{`n  `"schema_version`": 1,`n  `"peers`": []`n}`n")
[System.IO.File]::WriteAllText((Join-Path $relayDir "examples/rooms.example.json"), "{`n  `"schema_version`": 1,`n  `"rooms`": []`n}`n")

Write-Host "==> zip"
$serverZip = Join-Path $OutputDir "$serverName.zip"
$relayZip = Join-Path $OutputDir "$relayName.zip"
Compress-Archive -Path $serverDir -DestinationPath $serverZip -Force
Compress-Archive -Path $relayDir -DestinationPath $relayZip -Force
Remove-Item -LiteralPath $staging -Recurse -Force

Write-Host "pack: ok"
foreach ($archive in @($serverZip, $relayZip)) {
    $bytes = (Get-Item -LiteralPath $archive).Length
    Write-Host "  $archive ($bytes bytes)"
}
