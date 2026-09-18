# Remove RiscDom's temporary directories from the system temp directory.
#
#   scripts\clean-temp.ps1                          # dry run (safe default)
#   scripts\clean-temp.ps1 -Force                   # delete
#   scripts\clean-temp.ps1 -Force -OlderThanHours 24
#
# Safety: only entries whose name starts with `riscdom-` are considered, so the
# rest of the temp directory is never touched. `<temp>\riscdom` itself (the
# fallback data directory: settings, sessions, toolchains) is excluded explicitly.
[CmdletBinding()]
param(
    [switch]$Force,
    [int]$OlderThanHours = 0
)

$ErrorActionPreference = 'Stop'

$temp = [System.IO.Path]::GetTempPath()
if ([string]::IsNullOrWhiteSpace($temp) -or $temp.Length -lt 4) {
    throw "refusing to run: the temp directory looks wrong ($temp)"
}

$cutoff = if ($OlderThanHours -gt 0) { (Get-Date).AddHours(-$OlderThanHours) } else { $null }
$targets = @(
    Get-ChildItem -LiteralPath $temp -Filter 'riscdom-*' -Force -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Name -ne 'riscdom' -and ($null -eq $cutoff -or $_.LastWriteTime -lt $cutoff)
        }
)

if ($targets.Count -eq 0) {
    Write-Host "clean-temp: nothing to clean under $temp"
    exit 0
}

$bytes = 0
foreach ($t in $targets) {
    if ($t.PSIsContainer) {
        $bytes += (Get-ChildItem -LiteralPath $t.FullName -Recurse -File -Force -ErrorAction SilentlyContinue |
            Measure-Object Length -Sum).Sum
    } else {
        $bytes += $t.Length
    }
}
$kb = [math]::Round($bytes / 1KB, 1)

if (-not $Force) {
    Write-Host "clean-temp: DRY RUN — $($targets.Count) entry(ies), $kb KB under $temp"
    $targets | Select-Object -First 20 | ForEach-Object { Write-Host "  $($_.FullName)" }
    if ($targets.Count -gt 20) { Write-Host "  … and $($targets.Count - 20) more" }
    Write-Host "clean-temp: re-run with -Force to delete"
    exit 0
}

$removed = 0
foreach ($t in $targets) {
    try {
        Remove-Item -LiteralPath $t.FullName -Recurse -Force -ErrorAction Stop
        $removed++
    } catch {
        Write-Warning "could not remove $($t.FullName): $($_.Exception.Message)"
    }
}
$left = @(Get-ChildItem -LiteralPath $temp -Filter 'riscdom-*' -Force -ErrorAction SilentlyContinue).Count
Write-Host "clean-temp: removed $removed of $($targets.Count) entry(ies) ($kb KB); $left riscdom-* entry(ies) left"
