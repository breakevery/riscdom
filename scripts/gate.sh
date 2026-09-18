#!/usr/bin/env sh
# Local quality gate -- run before every commit:  scripts/gate.sh
#
# Requires QEMU (`qemu-system-riscv64`) and `riscv64-unknown-elf-gcc` on PATH:
# some tests boot a real guest. The ui probes need Node >= 22.6 (`node`, type
# stripping of the imported .ts modules; default since Node 23.6).
#
# Each step fails fast with a non-zero exit code.
set -eu

cd "$(dirname "$0")/.."

fail() {
  echo "gate: FAILED at $1"
  exit 1
}

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check || fail "cargo fmt"

echo "==> cargo clippy (audit / sandbox / agent / host)"
cargo clippy -p audit -p sandbox -p agent -p host --all-targets -- -D warnings || fail "cargo clippy"

echo "==> cargo clippy (ui/src-tauri)"
cargo clippy --manifest-path ui/src-tauri/Cargo.toml --all-targets -- -D warnings || fail "cargo clippy ui/src-tauri"

echo "==> cargo check (portable crates)"
cargo check -p audit -p sandbox -p agent || fail "cargo check"

echo "==> cargo test"
cargo test || fail "cargo test"

echo "==> cargo check (ui/src-tauri)"
cargo check --manifest-path ui/src-tauri/Cargo.toml || fail "cargo check ui/src-tauri"

echo "==> npm run build (ui)"
(cd ui && npm run build) || fail "npm run build"

echo "==> ui probes (scroll / layout / runs / dialog / preflight)"
node ui/scripts/probe-ui-scroll.mjs || fail "ui probe (chat scroll)"
node ui/scripts/probe-ui-width.mjs || fail "ui probe (pane layout)"
node ui/scripts/probe-ui-runs.mjs || fail "ui probe (run list)"
node ui/scripts/probe-ui-dialog.mjs || fail "ui probe (file picker)"
node ui/scripts/probe-ui-preflight.mjs || fail "ui probe (preflight)"

echo "==> mirrored constants (host/src)"
node scripts/check-mirrored-constants.mjs || fail "mirrored constants"

echo "==> bilingual doc links"
sh scripts/check-bilingual.sh || fail "bilingual links"

echo "gate: OK"
