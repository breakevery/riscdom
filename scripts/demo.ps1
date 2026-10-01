#!/usr/bin/env pwsh
#
# RiscDom demo driver -- audit package, item 6.
#
#   scripts\demo.ps1 [-Port 7821] [-SkipBuild]
#
# The Windows twin of `scripts/demo.sh` (the same split gate and commit keep). It runs
# the parts of docs/audit-package/demo.md that can be run honestly: it builds, starts a
# throwaway `riscdom-server`, checks step 1 (health, and a wrong token refused with exit
# 4), drives step 4 (the audit read, the export, and the "break a copy" check) and steps
# 5-6 (snapshot save/resume), and **prints** the commands for the steps that need a human
# -- step 2 (your API key), step 3 (the model run, run for real only if a key is already
# in the environment) and step 7 (a second node).
#
# It is **semi-automatic on purpose**: a key, a second node and a human reading the serial
# pane cannot be scripted, so this script does not pretend to. Every automated step is run
# for real and its exit code is checked; a step that cannot run says MANUAL or SKIP, never
# OK. Nothing is written outside target/demo/, so your real settings.json and audit.db are
# never touched.
param(
    [int]$Port = 7821,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Continue"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$DemoRoot  = "target/demo"
$Workspace = Join-Path $DemoRoot "ws"
$DataDir   = Join-Path $DemoRoot "data"
$ServerLog = Join-Path $DemoRoot "server.log"
$TokenFile = Join-Path $DataDir "token"
$Remote    = "127.0.0.1:$Port"

function Step  { param([string]$Id, [string]$Title) Write-Host ""; Write-Host "=== $Id -- $Title ===" }
function Ok    { param([string]$Msg) Write-Host "  OK      $Msg" }
function Manual{ param([string]$Msg) Write-Host "  MANUAL  $Msg" }
function Skip  { param([string]$Msg) Write-Host "  SKIP    $Msg" }
function Fail  { param([string]$Msg) Write-Host "  FAIL    $Msg" }
function Show  { param([string]$Cmd)  Write-Host "          $ $Cmd" }

function Find-Bin {
    param([string]$Name)
    foreach ($candidate in @("$Name.exe", $Name)) {
        $cmd = Get-Command $candidate -ErrorAction SilentlyContinue
        if ($cmd -and $cmd.Source) { return $cmd.Source }
    }
    foreach ($profile in @("debug", "release")) {
        foreach ($ext in @(".exe", "")) {
            $candidate = Join-Path "target/$profile" "$Name$ext"
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { return (Resolve-Path $candidate).Path }
        }
    }
    return $null
}

New-Item -ItemType Directory -Force -Path $Workspace, $DataDir | Out-Null

# -- Step 1: build and start a control plane -----------------------------------------
Step "Step 1" "install / verify the environment"
if ($SkipBuild) {
    Skip "build skipped (-SkipBuild); using whatever is already in target/"
} else {
    Write-Host "  building the workspace (this is the slow part)..."
    cargo build --workspace
    if ($LASTEXITCODE -ne 0) { Fail "cargo build failed"; exit 1 }
    Ok "cargo build --workspace"
}

$serverBin = Find-Bin "riscdom-server"
$cliBin    = Find-Bin "riscdom"
$verifyBin = Find-Bin "audit-verify"
if (-not $serverBin) {
    Skip "no riscdom-server binary on PATH or in target/"
    Write-Host "SKIP: riscdom-server binary not found; obtain from https://github.com/breakevery/riscdom-server"
    exit 0
}
if (-not $cliBin)    { Fail "no riscdom in target/ -- build the workspace first";    exit 1 }

Write-Host "  starting $serverBin --bind $Remote"
$server = Start-Process -FilePath $serverBin -PassThru -NoNewWindow `
    -ArgumentList @("--bind", $Remote, "--workspace", $Workspace, "--data-dir", $DataDir, "--log-level", "info") `
    -RedirectStandardOutput $ServerLog -RedirectStandardError "$ServerLog.err"

# Wait for the token file and a first successful health call.
$ready = $false
for ($i = 0; $i -lt 40; $i++) {
    Start-Sleep -Milliseconds 500
    if ((Test-Path $TokenFile) -and -not $server.HasExited) {
        & $cliBin --remote $Remote --token-file $TokenFile health *> $null
        if ($LASTEXITCODE -eq 0) { $ready = $true; break }
    }
}
try {
    if (-not $ready) { throw "the control plane did not answer health on $Remote" }

    Write-Host "  health:"
    & $cliBin --remote $Remote --token-file $TokenFile health
    if ($LASTEXITCODE -eq 0) { Ok "health answered" } else { Fail "health exit $LASTEXITCODE" }

    # A wrong token must be refused with exit 4 (the server's own refusal, not a guess).
    & $cliBin --remote $Remote --token "definitely-wrong" health *> $null
    if ($LASTEXITCODE -eq 4) { Ok "a wrong token is refused with exit 4" }
    else { Fail "a wrong token gave exit $LASTEXITCODE (expected 4)" }

    # -- Step 2: configure the model -------------------------------------------------
    Step "Step 2" "configure the model (BYOK)"
    $haveKey = [bool]$env:DEEPSEEK_API_KEY
    if ($haveKey) {
        Write-Host "  DEEPSEEK_API_KEY is present; configuring the node from it"
        & $cliBin --remote $Remote --token-file $TokenFile llm set --provider deepseek --model deepseek-chat
        if ($LASTEXITCODE -eq 0) { Ok "llm set accepted" } else { Fail "llm set exit $LASTEXITCODE" }
    } else {
        Manual "no DEEPSEEK_API_KEY in this shell -- set one, then run:"
        Show "riscdom --remote $Remote --token-file $TokenFile llm set --provider deepseek --model deepseek-chat"
        Show "  (offline alternative: --provider ollama --base-url http://127.0.0.1:11434)"
    }

    # -- Step 3: run a task ----------------------------------------------------------
    Step "Step 3" "run a task (compile + boot + read serial)"
    if ($haveKey) {
        Write-Host "  running the demo task..."
        & $cliBin --remote $Remote --token-file $TokenFile run "Write a RISC-V bare-metal hello, compile it, run it, and print hello over the serial console."
        if ($LASTEXITCODE -eq 0) { Ok "the run started and finished -- READ THE SERIAL OUTPUT for the guest's hello" }
        else { Fail "run exit $LASTEXITCODE" }
    } else {
        Manual "needs a model; with a key set, run:"
        Show ('riscdom --remote {0} --token-file {1} run "Write a RISC-V bare-metal hello, compile it, run it, and print hello over the serial console."' -f $Remote, $TokenFile)
    }

    # -- Step 4: the audit chain -----------------------------------------------------
    Step "Step 4" "read the audit chain"
    & $cliBin --remote $Remote --token-file $TokenFile audit status
    if ($LASTEXITCODE -eq 0) { Ok "audit status" } else { Fail "audit status exit $LASTEXITCODE" }

    & $cliBin --remote $Remote --token-file $TokenFile audit events --limit 10
    if ($LASTEXITCODE -eq 0) { Ok "audit events" } else { Fail "audit events exit $LASTEXITCODE" }

    $Export = "demo-audit.jsonl"   # resolved by the server against ITS workspace (target/demo/ws)
    & $cliBin --remote $Remote --token-file $TokenFile export audit-jsonl --out $Export
    if ($LASTEXITCODE -eq 0) { Ok "export audit-jsonl -> $Workspace\$Export" } else { Fail "export exit $LASTEXITCODE" }

    if ($verifyBin) {
        $Db = Join-Path $Workspace ".riscdom/audit.db"
        if (Test-Path $Db) {
            Write-Host "  independent checker on the real store:"
            $verifyOut = (& $verifyBin $Db --runs 2>&1) | Out-String
            Write-Host $verifyOut.TrimEnd()
            if ($LASTEXITCODE -eq 0) { Ok "audit-verify agrees the chain is intact" } else { Fail "audit-verify exit $LASTEXITCODE" }

            if ($verifyOut -match 'length: 0') {
                Manual "the chain is empty (no run yet) -- the break-a-copy check needs step 3 to have run"
            } else {
                # Break a *copy* and watch the checker call it Broken. The real file is untouched.
                $Copy = Join-Path (Get-Location) (Join-Path $DemoRoot "demo-audit-copy.db")
                Copy-Item -Force $Db $Copy
                foreach ($sidecar in @("-wal", "-shm")) {
                    if (Test-Path "$Db$sidecar") { Copy-Item -Force "$Db$sidecar" "$Copy$sidecar" }
                }
                $py = $null
                if (Get-Command python3 -ErrorAction SilentlyContinue) { $py = "python3" }
                elseif (Get-Command python -ErrorAction SilentlyContinue) { $py = "python" }
                if ($py) {
                    $pyScript = @'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("DROP TRIGGER audit_no_update")
c.execute("UPDATE audit_events SET action='evil' WHERE id=2")
c.commit()
'@
                    & $py -c $pyScript $Copy *> $null
                    Write-Host "  after breaking the copy (drop trigger + update), audit-verify says:"
                    & $verifyBin $Copy --runs
                    if ($LASTEXITCODE -ne 0) { Ok "the broken copy is reported Broken (the real file still verifies)" }
                    else { Fail "the broken copy still verified -- investigate" }
                } else {
                    Manual "no python to break the copy; do it by hand, then: audit-verify $Copy --runs"
                }
            }
        } else {
            Skip "no audit.db yet (run a task first, step 3)"
        }
    } else {
        Skip "no audit-verify binary in target/"
    }

    # -- Step 5: snapshot ------------------------------------------------------------
    Step "Step 5" "snapshot"
    & $cliBin --remote $Remote --token-file $TokenFile snapshots save demo-before
    if ($LASTEXITCODE -eq 0) { Ok "snapshots save demo-before" } else { Manual "saving needs a running VM (step 3 must have run)" }
    & $cliBin --remote $Remote --token-file $TokenFile snapshots list
    if ($LASTEXITCODE -eq 0) { Ok "snapshots list" } else { Fail "snapshots list exit $LASTEXITCODE" }

    # -- Step 6: roll back -----------------------------------------------------------
    Step "Step 6" "roll back"
    & $cliBin --remote $Remote --token-file $TokenFile snapshots resume demo-before
    if ($LASTEXITCODE -eq 0) { Ok "snapshots resume demo-before" } else { Manual "resuming needs the snapshot from step 5" }

    # -- Step 7: dispatch across nodes -----------------------------------------------
    Step "Step 7" "dispatch across nodes"
    Manual "a second node is a network decision -- start one with its own data dir, then dispatch:"
    Show "riscdom-server --bind 127.0.0.1:7822 --data-dir $DemoRoot/data-b --workspace $DemoRoot/ws-b"
    Show "riscdom --remote $Remote --token-file $TokenFile peers        # find the peer's node_id"
    Show ('riscdom --remote {0} --token-file {1} tasks dispatch --target <peer_node_id> --input "boot a hello"' -f $Remote, $TokenFile)
    Write-Host "          acceptance: the same task_id appears on both nodes' audit rows."
}
finally {
    if ($server -and -not $server.HasExited) {
        Write-Host ""
        Write-Host "  stopping the demo control plane (pid $($server.Id))"
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    }
}

Write-Host ""
Write-Host "done. server log: $ServerLog"
