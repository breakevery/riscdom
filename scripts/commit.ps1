# Gated commit wrapper: runs the gate first and commits only when it is green.
# Use this instead of `git commit`.
#
#   scripts\commit.ps1 "feat: something"
#   scripts\commit.ps1 "feat: something" -AllowUntracked
#
# `git add -A` picks up *untracked* files too, so a stray draft or an unrelated
# document can end up inside a commit it has nothing to do with (v0.4 batches 7/8
# shipped docs into an unrelated commit exactly that way). Untracked files are
# therefore refused by default: `git add` the ones you mean, or pass
# `-AllowUntracked` to say "yes, these belong in this commit".
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Message,
    [switch]$AllowUntracked
)

$ErrorActionPreference = "Continue"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Get-UntrackedFiles {
    @(git status --porcelain --untracked-files=all | Where-Object { $_ -match '^\?\? ' })
}

$untracked = Get-UntrackedFiles
if ($untracked.Count -gt 0 -and -not $AllowUntracked) {
    Write-Host "commit: REFUSED -- 'git add -A' would also add these untracked file(s):"
    foreach ($line in $untracked) { Write-Host "  $line" }
    Write-Host "commit: add the ones you mean with 'git add <path>', or re-run with -AllowUntracked"
    exit 1
}
if ($untracked.Count -gt 0) {
    Write-Host "commit: -AllowUntracked given, adding $($untracked.Count) untracked file(s):"
    foreach ($line in $untracked) { Write-Host "  $line" }
}

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
