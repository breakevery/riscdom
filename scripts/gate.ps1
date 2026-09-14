#!/usr/bin/env pwsh
# Local quality gate -- run before every commit:  scripts/gate.ps1
#
# Requires QEMU (`qemu-system-riscv64`) and `riscv64-unknown-elf-gcc` on PATH:
# some tests boot a real guest.
#
# Each step fails fast with a non-zero exit code.

$ErrorActionPreference = "Continue"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Fail([string]$what) {
    Write-Host "gate: FAILED at $what"
    exit 1
}

Write-Host "==> cargo fmt --all -- --check"
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { Fail "cargo fmt" }

Write-Host "==> cargo clippy (portable crates)"
cargo clippy -p audit -p sandbox -p agent --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { Fail "cargo clippy" }

Write-Host "==> cargo check (portable crates)"
cargo check -p audit -p sandbox -p agent
if ($LASTEXITCODE -ne 0) { Fail "cargo check" }

Write-Host "==> cargo test"
cargo test
if ($LASTEXITCODE -ne 0) { Fail "cargo test" }

Write-Host "==> cargo check (ui/src-tauri)"
cargo check --manifest-path ui/src-tauri/Cargo.toml
if ($LASTEXITCODE -ne 0) { Fail "cargo check ui/src-tauri" }

Write-Host "==> npm run build (ui)"
Push-Location ui
npm run build
$buildCode = $LASTEXITCODE
Pop-Location
if ($buildCode -ne 0) { Fail "npm run build" }

Write-Host "==> bilingual doc links"
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check-bilingual.ps1
if ($LASTEXITCODE -ne 0) { Fail "bilingual links" }

Write-Host "gate: OK"
exit 0
