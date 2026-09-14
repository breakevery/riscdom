#!/usr/bin/env pwsh
<#
Pre-flight checks (read-only). Run before committing.

Checks:
  1. git tracks no sensitive files (.db / .db-journal / .sqlite / *.jsonl / .env)
  2. no secret-shaped strings in tracked files
     (sk-..., ghp_, github_pat_, AKIA..., AIza...)
  3. the live $env:DEEPSEEK_API_KEY value never appears in tracked files

Exit code 0 = OK, 1 = something was found.
Only file:line is printed — never the matched value.
#>

$ErrorActionPreference = "Continue"

$repo = (& git rev-parse --show-toplevel 2>$null)
if (-not $repo) {
    Write-Host "preflight: not a git repository"
    exit 1
}
Set-Location $repo

$failures = New-Object System.Collections.Generic.List[string]

function Add-Hits {
    param([string]$Label, [string[]]$GrepArgs)
    $out = & git grep -n -I @GrepArgs -- . 2>$null
    foreach ($line in @($out)) {
        if (-not $line) { continue }
        $parts = $line -split ':', 3
        if ($parts.Count -ge 2) {
            $failures.Add("$Label -> $($parts[0]):$($parts[1])")
        }
    }
}

# 1. sensitive files must not be tracked
foreach ($f in @(& git ls-files)) {
    $name = Split-Path $f -Leaf
    if ($f -match '\.(db|db-journal|sqlite|jsonl)$' -or $name -eq '.env') {
        $failures.Add("tracked sensitive file -> $f")
    }
}

# 2. secret-shaped strings
Add-Hits 'api-key-shaped (sk-)' @('-E', 'sk-[A-Za-z0-9]{16,}')
Add-Hits 'github-token (ghp_)' @('-E', 'ghp_[A-Za-z0-9]{20,}')
Add-Hits 'github-pat (github_pat_)' @('-E', 'github_pat_[A-Za-z0-9_]{20,}')
Add-Hits 'aws-access-key (AKIA)' @('-E', 'AKIA[0-9A-Z]{16}')
Add-Hits 'google-api-key (AIza)' @('-E', 'AIza[0-9A-Za-z_\-]{30,}')

# 3. the live environment key must never appear in the tree
if ($env:DEEPSEEK_API_KEY -and $env:DEEPSEEK_API_KEY.Trim().Length -gt 0) {
    Add-Hits 'live $env:DEEPSEEK_API_KEY value' @('-F', '-e', $env:DEEPSEEK_API_KEY)
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host $_ }
    Write-Host "preflight: FAILED ($($failures.Count))"
    exit 1
}

Write-Host "preflight: OK"
exit 0
