#!/usr/bin/env pwsh
# Local quality gate -- run before every commit:  scripts/gate.ps1
#
# Requires QEMU (`qemu-system-riscv64`) and `riscv64-unknown-elf-gcc` on PATH:
# some tests boot a real guest. The ui probes need Node >= 22.6 (`node`, type
# stripping of the imported .ts modules; default since Node 23.6).
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

Write-Host "==> cargo clippy (audit / sandbox / agent / host)"
cargo clippy -p audit -p sandbox -p agent -p host --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { Fail "cargo clippy" }

Write-Host "==> cargo clippy (ui/src-tauri)"
cargo clippy --manifest-path ui/src-tauri/Cargo.toml --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { Fail "cargo clippy ui/src-tauri" }

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

Write-Host "==> ui probes (scroll / layout / runs / dialog / preflight / theme)"
node ui/scripts/probe-ui-scroll.mjs
if ($LASTEXITCODE -ne 0) { Fail "ui probe (chat scroll)" }
node ui/scripts/probe-ui-width.mjs
if ($LASTEXITCODE -ne 0) { Fail "ui probe (pane layout)" }
node ui/scripts/probe-ui-runs.mjs
if ($LASTEXITCODE -ne 0) { Fail "ui probe (run list)" }
node ui/scripts/probe-ui-dialog.mjs
if ($LASTEXITCODE -ne 0) { Fail "ui probe (file picker)" }
node ui/scripts/probe-ui-preflight.mjs
if ($LASTEXITCODE -ne 0) { Fail "ui probe (preflight)" }
node ui/scripts/probe-ui-theme.mjs
if ($LASTEXITCODE -ne 0) { Fail "ui probe (theme)" }

Write-Host "==> mirrored constants (host/src)"
node scripts/check-mirrored-constants.mjs
if ($LASTEXITCODE -ne 0) { Fail "mirrored constants" }

Write-Host "==> bilingual doc links"
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check-bilingual.ps1
if ($LASTEXITCODE -ne 0) { Fail "bilingual links" }

Write-Host "gate: OK"
exit 0
