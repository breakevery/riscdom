#!/usr/bin/env sh
# Local quality gate -- run before every commit:  scripts/gate.sh
#
# Requires QEMU (`qemu-system-riscv64`) and `riscv64-unknown-elf-gcc` on PATH:
# some tests boot a real guest.
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

echo "==> cargo clippy (portable crates)"
cargo clippy -p audit -p sandbox -p agent --all-targets -- -D warnings || fail "cargo clippy"

echo "==> cargo check (portable crates)"
cargo check -p audit -p sandbox -p agent || fail "cargo check"

echo "==> cargo test"
cargo test || fail "cargo test"

echo "==> cargo check (ui/src-tauri)"
cargo check --manifest-path ui/src-tauri/Cargo.toml || fail "cargo check ui/src-tauri"

echo "==> npm run build (ui)"
(cd ui && npm run build) || fail "npm run build"

echo "gate: OK"
