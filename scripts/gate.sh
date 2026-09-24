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
# `--experimental-strip-types` on 22.6-23.5). On Linux the two Tauri crates need the
# webkit2gtk / gtk / librsvg / libsoup development packages, and `host-core` needs
# `libdbus-1-dev` through `keyring`; CI installs them (`.github/workflows/ci.yml`).
# On Windows, QEMU (`qemu-system-riscv64`) and a RISC-V bare-metal GCC must be on PATH,
# because several tests boot a real guest. Python 3 is optional: it runs
# `examples/python`'s self-test, which prints a skip when no interpreter is there.
#
# Platform differences are printed, never skipped silently:
#   - without QEMU + a RISC-V GCC: the guest-booting tests are skipped and every crate's
#     unit tests run instead (`cargo test --workspace --lib`).
#   - without python3/python: the reference supervisor's self-test is skipped.
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

# The Python interpreter for the reference supervisor's self-test, or nothing. Python is
# not a build dependency of anything else here, so the check prints a skip when it is
# absent rather than failing the gate on a machine that never had it.
have_python() {
  if have python3; then
    echo python3
  elif have python; then
    echo python
  else
    echo ""
  fi
}

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check || fail "cargo fmt"

echo "==> cargo clippy (portable crates audit sandbox agent)"
cargo clippy -p audit -p sandbox -p agent --all-targets -- -D warnings || fail "cargo clippy"

echo "==> cargo check (portable crates audit sandbox agent)"
cargo check -p audit -p sandbox -p agent || fail "cargo check"

# Every crate is linted and checked on **every** platform, so there is no OS branch here.
# The two Tauri crates need webkit2gtk / gtk / librsvg on Linux and `host-core` needs
# `dbus-1` (through `keyring`); CI installs those. `worker` was missing from every clippy
# list before this batch, on both platforms.
echo "==> cargo clippy (cli + server + host-core + host-tauri + worker)"
# `--no-deps`: the crates we own are linted, their dependencies are only built.
cargo clippy -p cli -p server -p host-core -p host-tauri -p worker --all-targets --no-deps -- -D warnings || fail "cargo clippy cli + server + host-core + host-tauri + worker"

echo "==> cargo clippy (ui/src-tauri)"
cargo clippy --manifest-path ui/src-tauri/Cargo.toml --all-targets -- -D warnings || fail "cargo clippy ui/src-tauri"

echo "==> cargo check (ui/src-tauri)"
cargo check --manifest-path ui/src-tauri/Cargo.toml || fail "cargo check ui/src-tauri"

if have_guest_tools; then
  echo "==> cargo test"
  cargo test || fail "cargo test"
else
  skip "the guest-booting tests (no qemu-system-riscv64 + RISC-V GCC on PATH)"
  echo "==> cargo test --workspace --lib"
  cargo test --workspace --lib || fail "cargo test --workspace --lib"
fi

echo "==> npm run build (ui)"
(cd ui && npm run build) || fail "npm run build"

echo "==> ui probes (scroll / layout / runs / snapshot / dialog / preflight / theme / i18n)"
node ui/scripts/probe-ui-scroll.mjs || fail "ui probe (chat scroll)"
node ui/scripts/probe-ui-width.mjs || fail "ui probe (pane layout)"
node ui/scripts/probe-ui-runs.mjs || fail "ui probe (run list)"
node ui/scripts/probe-ui-snapshot.mjs || fail "ui probe (snapshot naming)"
node ui/scripts/probe-ui-dialog.mjs || fail "ui probe (file picker)"
node ui/scripts/probe-ui-preflight.mjs || fail "ui probe (preflight)"
node ui/scripts/probe-ui-theme.mjs || fail "ui probe (theme)"
node ui/scripts/probe-ui-i18n.mjs || fail "ui probe (i18n)"

echo "==> mirrored constants (host-core/src + host-tauri/src)"
node scripts/check-mirrored-constants.mjs || fail "mirrored constants"

echo "==> tool schema documents (executor + control plane)"
node scripts/check-tool-schema.mjs || fail "tool schema"

python_bin="$(have_python)"
if [ -n "$python_bin" ]; then
  echo "==> python reference supervisor self-test (examples/python)"
  "$python_bin" examples/python/dispatch.py --self-test || fail "python reference supervisor"
else
  skip "the python reference supervisor self-test (no python3/python on PATH)"
fi

echo "==> remote executor example self-test (worker)"
cargo run -q -p worker --example remote_executor -- --self-test || fail "remote executor example"

echo "==> wix version guard"
node scripts/check-wix-version.mjs || fail "wix version guard"

echo "==> ui string registry"
node scripts/check-ui-strings.mjs || fail "ui string registry"

echo "==> bilingual doc links"
sh scripts/check-bilingual.sh || fail "bilingual links"

echo "gate: OK"
