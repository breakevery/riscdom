# Bilingual documentation check (reports only -- never rewrites files).
#
# Every *.md in the repository must carry a language switcher on its first line and have a
# counterpart in the other language:
#   X.md        -> first line "[ZH](X.zh-CN.md) | English"   (ZH = U+4E2D U+6587)
#   X.zh-CN.md  -> first line "[English](X.md) | ZH"
#
# Excluded:
#   - LICENSE: kept as English legal text by design.
#   - IDENTITY.md / SOUL.md / USER.md: agent-workspace identity files that are read by the AI,
#     not human-facing user documentation; they are deliberately single-language.
#
# NOTE: the Chinese token is built from code points on purpose. Windows PowerShell 5.1 reads
# a BOM-less script as ANSI, which would mangle a literal CJK string on the comparison side.
$ErrorActionPreference = "Stop"

$zh = [string]::Concat([char]0x4E2D, [char]0x6587)
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$excludeNames = @("LICENSE", "LICENSE.md", "LICENSE.txt", "IDENTITY.md", "SOUL.md", "USER.md")
$skipDirs = @("node_modules", "target", ".git", ".cowork-temp", "dist")

$problems = New-Object System.Collections.Generic.List[string]
$checked = 0

function Get-FirstLine([string]$path) {
    $reader = New-Object System.IO.StreamReader($path, [System.Text.Encoding]::UTF8)
    try { return $reader.ReadLine() } finally { $reader.Close() }
}

$files = Get-ChildItem -Path $root -Recurse -File -Filter *.md | Where-Object {
    $rel = $_.FullName.Substring($root.Length + 1)
    $parts = $rel -split '[\\/]'
    $inSkipped = $false
    foreach ($p in $parts) { if ($skipDirs -contains $p) { $inSkipped = $true } }
    (-not $inSkipped) -and ($excludeNames -notcontains $_.Name)
}

foreach ($file in $files) {
    $rel = ($file.FullName.Substring($root.Length + 1)) -replace '\\', '/'
    $first = Get-FirstLine $file.FullName

    if ($file.Name.EndsWith(".zh-CN.md")) {
        $enName = $file.Name.Substring(0, $file.Name.Length - ".zh-CN.md".Length) + ".md"
        $want = "[English]($enName) | $zh"
        $pair = Join-Path $file.DirectoryName $enName
        if (-not (Test-Path -LiteralPath $pair)) {
            $problems.Add("${rel}:1: missing English counterpart $enName")
            continue
        }
    } else {
        $zhName = $file.Name.Substring(0, $file.Name.Length - 3) + ".zh-CN.md"
        $want = "[$zh]($zhName) | English"
        $pair = Join-Path $file.DirectoryName $zhName
        if (-not (Test-Path -LiteralPath $pair)) {
            $problems.Add("${rel}:1: missing Chinese counterpart $zhName")
            continue
        }
    }

    if ($first -ne $want) {
        $problems.Add("${rel}:1: expected '$want', found '$first'")
    } else {
        $checked++
    }
}

if ($problems.Count -gt 0) {
    Write-Output "bilingual links: FAILED"
    foreach ($p in $problems) { Write-Output "  $p" }
    exit 1
}

Write-Output "bilingual links: OK ($checked files checked)"
