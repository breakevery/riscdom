#!/usr/bin/env pwsh
# Thin wrapper: locate Git's `sh.exe` and run `scripts/gate.sh`, which is THE one
# list of gate steps (CI runs the very same file, so nothing can drift).
#
#   scripts\gate.ps1
#
# The wrapper only forwards: it finds a real `sh.exe`, runs the gate script from
# the repository root, and exits with the gate's own exit code. If no `sh.exe`
# exists it fails loudly -- it never falls back to a second, hand-maintained
# command list.
$ErrorActionPreference = "Continue"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Find-Sh {
    $candidates = New-Object System.Collections.Generic.List[string]

    # Derive from `where git`: <root>\cmd\git.exe -> <root>\bin\sh.exe (or usr\bin).
    $git = Get-Command git.exe -ErrorAction SilentlyContinue
    if (-not $git) { $git = Get-Command git -ErrorAction SilentlyContinue }
    if ($git -and $git.Source) {
        $gitDir = Split-Path -Parent $git.Source
        $root = Split-Path -Parent $gitDir
        $candidates.Add((Join-Path $root "bin\sh.exe"))
        $candidates.Add((Join-Path $root "usr\bin\sh.exe"))
        $candidates.Add((Join-Path $gitDir "sh.exe"))
    }

    # Well-known install locations.
    foreach ($base in @($env:ProgramFiles, ${env:ProgramFiles(x86)}, (Join-Path $env:LOCALAPPDATA "Programs"))) {
        if ($base) {
            $candidates.Add((Join-Path $base "Git\bin\sh.exe"))
            $candidates.Add((Join-Path $base "Git\usr\bin\sh.exe"))
        }
    }

    # Whatever the PATH already resolves.
    $onPath = Get-Command sh.exe -ErrorAction SilentlyContinue
    if ($onPath -and $onPath.Source) { $candidates.Add($onPath.Source) }

    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            return $candidate
        }
    }
    return $null
}

$sh = Find-Sh
if (-not $sh) {
    Write-Host "gate: FAILED -- no sh.exe found (Git for Windows provides one)."
    Write-Host "gate: looked for sh.exe next to git.exe and under Program Files\Git; install Git or add sh.exe to PATH."
    exit 1
}

Write-Host "gate: running scripts/gate.sh via $sh"
# Merge stderr into stdout: the toolchain writes progress there, and letting
# PowerShell render it as error records only adds noise. The exit code is kept.
& $sh "scripts/gate.sh" 2>&1 | ForEach-Object { Write-Host $_ }
exit $LASTEXITCODE
