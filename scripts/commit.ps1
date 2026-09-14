# Gated commit wrapper: runs scripts/gate.ps1 first and commits only when it is
# green. Use this instead of `git commit`.
#
#   scripts\commit.ps1 "feat: something"
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Message
)

$ErrorActionPreference = "Continue"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

Write-Host "== commit: running gate =="
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\gate.ps1
if ($LASTEXITCODE -ne 0) {
    Write-Host "commit: gate FAILED -- nothing committed"
    exit 1
}

git add -A
if ($LASTEXITCODE -ne 0) { Write-Host "commit: git add failed"; exit 1 }
git commit -m $Message
if ($LASTEXITCODE -ne 0) { Write-Host "commit: git commit failed"; exit 1 }
Write-Host "commit: ok ($Message)"
