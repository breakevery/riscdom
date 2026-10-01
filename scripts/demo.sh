#!/usr/bin/env sh
#
# RiscDom demo driver -- audit package, item 6.
#
#   scripts/demo.sh [-p PORT] [-s]
#
# The Unix twin of `scripts/demo.ps1`. It runs the parts of docs/audit-package/demo.md
# that can be run honestly: it builds, starts a throwaway `riscdom-server`, checks step 1
# (health, and a wrong token refused with exit 4), drives step 4 (the audit read, the
# export, and the "break a copy" check) and steps 5-6 (snapshot save/resume), and
# **prints** the commands for the steps that need a human -- step 2 (your API key), step 3
# (the model run, run for real only if a key is already in the environment) and step 7 (a
# second node).
#
# It is **semi-automatic on purpose**: a key, a second node and a human reading the serial
# pane cannot be scripted, so this script does not pretend to. Every automated step is run
# for real; a step that cannot run says MANUAL or SKIP, never OK. Nothing is written
# outside target/demo/, so your real settings.json and audit.db are never touched.
set -u

cd "$(dirname "$0")/.."

PORT=7821
SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    -p|--port) PORT="$2"; shift 2 ;;
    -s|--skip-build) SKIP_BUILD=1; shift ;;
    -h|--help) echo "usage: scripts/demo.sh [-p PORT] [-s]"; exit 0 ;;
    *) echo "unknown option: $1"; exit 2 ;;
  esac
done

DEMO_ROOT="target/demo"
WORKSPACE="$DEMO_ROOT/ws"
DATA_DIR="$DEMO_ROOT/data"
SERVER_LOG="$DEMO_ROOT/server.log"
TOKEN_FILE="$DATA_DIR/token"
REMOTE="127.0.0.1:$PORT"

step()   { echo ""; echo "=== $1 -- $2 ==="; }
ok()     { echo "  OK      $1"; }
manual() { echo "  MANUAL  $1"; }
skip()   { echo "  SKIP    $1"; }
fail()   { echo "  FAIL    $1"; }
show()   { echo "          \$ $1"; }

find_bin() {
  for profile in debug release; do
    if [ -x "target/$profile/$1" ]; then echo "target/$profile/$1"; return 0; fi
  done
  return 1
}

mkdir -p "$WORKSPACE" "$DATA_DIR"

# -- Step 1: build and start a control plane -----------------------------------------
step "Step 1" "install / verify the environment"
if [ "$SKIP_BUILD" -eq 1 ]; then
  skip "build skipped (-s); using whatever is already in target/"
else
  echo "  building the workspace (this is the slow part)..."
  cargo build --workspace || { fail "cargo build failed"; exit 1; }
  ok "cargo build --workspace"
fi

SERVER_BIN="$(find_bin riscdom-server)" || { fail "no riscdom-server in target/ -- build first"; exit 1; }
CLI_BIN="$(find_bin riscdom)" || { fail "no riscdom in target/ -- build first"; exit 1; }
VERIFY_BIN="$(find_bin audit-verify)" || true

echo "  starting $SERVER_BIN --bind $REMOTE"
"$SERVER_BIN" --bind "$REMOTE" --workspace "$WORKSPACE" --data-dir "$DATA_DIR" --log-level info >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT INT TERM

READY=0
i=0
while [ "$i" -lt 40 ]; do
  i=$((i + 1))
  sleep 0.5
  if [ -f "$TOKEN_FILE" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    if "$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" health >/dev/null 2>&1; then
      READY=1; break
    fi
  fi
done
[ "$READY" -eq 1 ] || { fail "the control plane did not answer health on $REMOTE"; exit 1; }

echo "  health:"
"$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" health
[ $? -eq 0 ] && ok "health answered" || fail "health failed"

"$CLI_BIN" --remote "$REMOTE" --token "definitely-wrong" health >/dev/null 2>&1
[ $? -eq 4 ] && ok "a wrong token is refused with exit 4" || fail "a wrong token did not give exit 4"

# -- Step 2: configure the model -------------------------------------------------------
step "Step 2" "configure the model (BYOK)"
if [ -n "${DEEPSEEK_API_KEY:-}" ]; then
  echo "  DEEPSEEK_API_KEY is present; configuring the node from it"
  "$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" llm set --provider deepseek --model deepseek-chat \
    && ok "llm set accepted" || fail "llm set failed"
  HAVE_KEY=1
else
  HAVE_KEY=0
  manual "no DEEPSEEK_API_KEY in this shell -- set one, then run:"
  show "riscdom --remote $REMOTE --token-file $TOKEN_FILE llm set --provider deepseek --model deepseek-chat"
  show "  (offline alternative: --provider ollama --base-url http://127.0.0.1:11434)"
fi

# -- Step 3: run a task -----------------------------------------------------------------
step "Step 3" "run a task (compile + boot + read serial)"
if [ "$HAVE_KEY" -eq 1 ]; then
  "$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" run \
    "Write a RISC-V bare-metal hello, compile it, run it, and print hello over the serial console." \
    && ok "the run finished -- READ THE SERIAL OUTPUT for the guest's hello" || fail "run failed"
else
  manual "needs a model; with a key set, run:"
  show "riscdom --remote $REMOTE --token-file $TOKEN_FILE run \"Write a RISC-V bare-metal hello, compile it, run it, and print hello over the serial console.\""
fi

# -- Step 4: the audit chain -------------------------------------------------------------
step "Step 4" "read the audit chain"
"$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" audit status && ok "audit status" || fail "audit status"
"$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" audit events --limit 10 && ok "audit events" || fail "audit events"
"$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" export audit-jsonl --out demo-audit.jsonl \
  && ok "export audit-jsonl -> $WORKSPACE/demo-audit.jsonl" || fail "export"

if [ -n "$VERIFY_BIN" ] && [ -f "$WORKSPACE/.riscdom/audit.db" ]; then
  echo "  independent checker on the real store:"
  VERIFY_OUT="$("$VERIFY_BIN" "$WORKSPACE/.riscdom/audit.db" --runs 2>&1)"
  VERIFY_RC=$?
  echo "$VERIFY_OUT"
  [ "$VERIFY_RC" -eq 0 ] && ok "audit-verify agrees the chain is intact" || fail "audit-verify"

  case "$VERIFY_OUT" in
    *"length: 0"*)
      manual "the chain is empty (no run yet) -- the break-a-copy check needs step 3 to have run"
      ;;
    *)
      cp -f "$WORKSPACE/.riscdom/audit.db" "$DEMO_ROOT/demo-audit-copy.db"
      for sidecar in -wal -shm; do
        [ -f "$WORKSPACE/.riscdom/audit.db$sidecar" ] && \
          cp -f "$WORKSPACE/.riscdom/audit.db$sidecar" "$DEMO_ROOT/demo-audit-copy.db$sidecar"
      done
      if command -v python3 >/dev/null 2>&1 || command -v python >/dev/null 2>&1; then
        PY="$(command -v python3 || command -v python)"
        "$PY" - "$DEMO_ROOT/demo-audit-copy.db" <<'PYEOF' >/dev/null 2>&1
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("DROP TRIGGER audit_no_update")
c.execute("UPDATE audit_events SET action='evil' WHERE id=2")
c.commit()
PYEOF
        echo "  after breaking the copy (drop trigger + update), audit-verify says:"
        "$VERIFY_BIN" "$DEMO_ROOT/demo-audit-copy.db" --runs && fail "the broken copy still verified" \
          || ok "the broken copy is reported Broken (the real file still verifies)"
      else
        manual "no python to break the copy; do it by hand, then: audit-verify $DEMO_ROOT/demo-audit-copy.db --runs"
      fi
      ;;
  esac
else
  skip "no audit-verify binary, or no audit.db yet (run a task first, step 3)"
fi

# -- Step 5: snapshot ---------------------------------------------------------------------
step "Step 5" "snapshot"
"$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" snapshots save demo-before \
  && ok "snapshots save demo-before" || manual "saving needs a running VM (step 3 must have run)"
"$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" snapshots list && ok "snapshots list" || fail "snapshots list"

# -- Step 6: roll back ----------------------------------------------------------------------
step "Step 6" "roll back"
"$CLI_BIN" --remote "$REMOTE" --token-file "$TOKEN_FILE" snapshots resume demo-before \
  && ok "snapshots resume demo-before" || manual "resuming needs the snapshot from step 5"

# -- Step 7: dispatch across nodes ---------------------------------------------------------
step "Step 7" "dispatch across nodes"
manual "a second node is a network decision -- start one with its own data dir, then dispatch:"
show "riscdom-server --bind 127.0.0.1:7822 --data-dir $DEMO_ROOT/data-b --workspace $DEMO_ROOT/ws-b"
show "riscdom --remote $REMOTE --token-file $TOKEN_FILE peers        # find the peer's node_id"
show "riscdom --remote $REMOTE --token-file $TOKEN_FILE tasks dispatch --target <peer_node_id> --input \"boot a hello\""
echo "          acceptance: the same task_id appears on both nodes' audit rows."

echo ""
echo "done. server log: $SERVER_LOG"
