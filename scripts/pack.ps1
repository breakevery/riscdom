#!/usr/bin/env pwsh
# Package the relay, the connection layer's server (v1.0 M7b-1; relay-only since v1.0 M8-4c).
#
#   scripts\pack.ps1 [-OutputDir <dir>]
#
# It builds the release binary and assembles **one** archive in the host's own platform format:
#
#   riscdom-relay-<version>-<platform>.zip    the connection layer's server
#
# What the package holds is docs/server-distribution.md's business; this script only builds what that
# document says. The **single-node control plane** (`riscdom-server`) left this repository in v1.0
# M8-4a and is packaged in its own repository (<https://github.com/breakevery/riscdom-server>), so this
# packer no longer builds it and no longer copies a front end into a package.
#
# It is the **Windows twin** of `scripts/pack.sh` (the same split `gate` and `commit`
# keep, so the platform the project verifies on has a native implementation): this half writes `.zip`
# with PowerShell's own `Compress-Archive`, the other writes `.tar.gz` with the system `tar`.
#
# It never tags, never publishes, and never copies a data
# directory or a credential into a package: the relay's key is minted by the program on
# its first start, where it belongs.
param(
    [string]$OutputDir = "target/dist"
)

$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Write-Usage {
    Write-Host "usage: scripts\pack.ps1 [-OutputDir <dir>]"
    Write-Host ""
    Write-Host "  -OutputDir <dir>   where the archive goes (default: target/dist)"
}

# The version comes from the workspace, which is where `[workspace.package] version` lives: the one
# place a release bumps, so a package cannot disagree with the binary inside it.
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

Write-Host "==> cargo build --release (riscdom-relay)"
cargo build --release -p net --bin riscdom-relay
if ($LASTEXITCODE -ne 0) { Write-Host "pack: cargo build (relay) failed"; exit 1 }

$staging = Join-Path $OutputDir ".staging-$version"
if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
New-Item -ItemType Directory -Path $staging -Force | Out-Null

# ---- the connection layer's server ----------------------------------------
$relayName = "riscdom-relay-$version-$platform"
$relayDir = Join-Path $staging $relayName
New-Item -ItemType Directory -Path (Join-Path $relayDir "examples") -Force | Out-Null
Copy-Item "target/release/riscdom-relay.exe" (Join-Path $relayDir "riscdom-relay.exe")
Copy-Item "net/README.md" (Join-Path $relayDir "README.md")
# `File.WriteAllText` writes UTF-8 **without** a BOM, on both PowerShell 5.1 and 7 (`Set-Content
# -Encoding utf8` would add one, and a BOM is not valid JSON).
[System.IO.File]::WriteAllText((Join-Path $relayDir "examples/peers.example.json"), "{`n  `"schema_version`": 1,`n  `"peers`": []`n}`n")
[System.IO.File]::WriteAllText((Join-Path $relayDir "examples/rooms.example.json"), "{`n  `"schema_version`": 1,`n  `"rooms`": []`n}`n")

Write-Host "==> zip"
$relayZip = Join-Path $OutputDir "$relayName.zip"
Compress-Archive -Path $relayDir -DestinationPath $relayZip -Force
Remove-Item -LiteralPath $staging -Recurse -Force

Write-Host "pack: ok"
$bytes = (Get-Item -LiteralPath $relayZip).Length
Write-Host "  $relayZip ($bytes bytes)"
