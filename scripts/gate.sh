#!/usr/bin/env sh
# Local quality gate -- run before every commit:  scripts/gate.sh
#
# THIS FILE IS THE ONE LIST OF WHAT "GREEN" MEANS.
# CI runs it verbatim (`.github/workflows/ci.yml` -> `sh scripts/gate.sh`), so a
# check can never drift apart between CI and a developer machine again. If a check
# belongs in CI, it belongs here -- not in the workflow.
#
# Requirements: Rust (rustfmt + clippy) and Node >= 22.6 (the UI probes import the
# `.ts` modules and rely on type stripping: default from Node 23.6, needs
# `--experimental-strip-types` on 22.6-23.5). On Windows, QEMU
# (`qemu-system-riscv64`) and a RISC-V bare-metal GCC must be on PATH, because
# several tests boot a real guest.
#
# Platform differences are printed, never skipped silently:
#   - non-Windows: `host` / `ui/src-tauri` lint and check are skipped (Tauri needs
#     the webkit2gtk / gtk / librsvg system libraries);
#   - without QEMU + a RISC-V GCC: the guest-booting tests are skipped and the
#     portable library tests run instead.
#
# Each step fails fast with a non-zero exit code.
set -eu

cd "$(dirname "$0")/.."

fail() {
  echo "gate: FAILED at $1"
  exit 1
}

skip() {
  echo "gate: skipped on this platform -- $1"
}

have() {
  command -v "$1" >/dev/null 2>&1
}

have_guest_tools() {
  have qemu-system-riscv64 || return 1
  if have riscv64-unknown-elf-gcc || have riscv-none-elf-gcc; then
    return 0
  fi
  return 1
}

case "$(uname -s 2>/dev/null || echo unknown)" in
  MINGW*|MSYS*|CYGWIN*) host_os="windows" ;;
  *) host_os="unix" ;;
esac

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check || fail "cargo fmt"

echo "==> cargo clippy (portable crates audit sandbox agent)"
cargo clippy -p audit -p sandbox -p agent --all-targets -- -D warnings || fail "cargo clippy"

echo "==> cargo check (portable crates audit sandbox agent)"
cargo check -p audit -p sandbox -p agent || fail "cargo check"

if [ "$host_os" = "windows" ]; then
  echo "==> cargo clippy (host)"
  cargo clippy -p host --all-targets -- -D warnings || fail "cargo clippy host"

  echo "==> cargo clippy (ui/src-tauri)"
  cargo clippy --manifest-path ui/src-tauri/Cargo.toml --all-targets -- -D warnings || fail "cargo clippy ui/src-tauri"

  echo "==> cargo check (ui/src-tauri)"
  cargo check --manifest-path ui/src-tauri/Cargo.toml || fail "cargo check ui/src-tauri"
else
  skip "host + ui/src-tauri lint and check (Tauri needs webkit2gtk / gtk / librsvg; they are linted on Windows)"
fi

if have_guest_tools; then
  echo "==> cargo test"
  cargo test || fail "cargo test"
else
  skip "the guest-booting tests (no qemu-system-riscv64 + RISC-V GCC on PATH)"
  echo "==> cargo test --lib (audit / sandbox)"
  cargo test -p audit -p sandbox --lib || fail "cargo test --lib"
fi

echo "==> npm run build (ui)"
(cd ui && npm run build) || fail "npm run build"

echo "==> ui probes (scroll / layout / runs / dialog / preflight / theme)"
node ui/scripts/probe-ui-scroll.mjs || fail "ui probe (chat scroll)"
node ui/scripts/probe-ui-width.mjs || fail "ui probe (pane layout)"
node ui/scripts/probe-ui-runs.mjs || fail "ui probe (run list)"
node ui/scripts/probe-ui-dialog.mjs || fail "ui probe (file picker)"
node ui/scripts/probe-ui-preflight.mjs || fail "ui probe (preflight)"
node ui/scripts/probe-ui-theme.mjs || fail "ui probe (theme)"

echo "==> mirrored constants (host/src)"
node scripts/check-mirrored-constants.mjs || fail "mirrored constants"

echo "==> wix version guard"
node scripts/check-wix-version.mjs || fail "wix version guard"

echo "==> bilingual doc links"
sh scripts/check-bilingual.sh || fail "bilingual links"

echo "gate: OK"
