#!/usr/bin/env sh
# Local quality gate -- run before every commit:  scripts/gate.sh
#
# THIS FILE IS THE ONE LIST OF WHAT "GREEN" MEANS.
# CI runs it verbatim (`.github/workflows/ci.yml` -> `sh scripts/gate.sh`), so a
# check can never drift apart between CI and a developer machine again. If a check
# belongs in CI, it belongs here -- not in the workflow.
#
# Requirements: Rust (rustfmt + clippy) and Node (the TypeScript SDK tests import the
# `.ts` modules and rely on type stripping: default from Node 23.6, needs
# `--experimental-strip-types` on 22.6-23.5). On Linux `host-core` needs
# `libdbus-1-dev` through `keyring`; CI installs it (`.github/workflows/ci.yml`).
# On Windows, QEMU (`qemu-system-riscv64`) and a RISC-V bare-metal GCC must be on PATH,
# because several tests boot a real guest. Python 3 is optional: it runs
# `examples/python`'s two self-tests, which print a skip when no interpreter is there.
#
# Platform differences are printed, never skipped silently:
#   - without QEMU + a RISC-V GCC: the tests that need them are `#[ignore]`d, so a plain
#     `cargo test --workspace --no-fail-fast` runs the portable set. A machine that has the
#     tools runs everything with `--include-ignored`, minus the three tests that need an API
#     key or the OS keyring (the `--skip` flags below) -- `--skip` matches the *test name*,
#     which for an integration test is the function name, not the file name.
#   - Two of `agent`'s unit tests compile C for real and print a skip when no GCC is there.
#   - without python3/python: the two reference self-tests under `examples/python` (the
#     supervisor's and the dispatcher's), and the encoding scan, are skipped.
#
# `--no-fail-fast`: one failing test binary must not hide the rest of the workspace.
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
# `host-core` needs `dbus-1` (through `keyring`) on Linux; CI installs it. `worker` was
# missing from every clippy list before that batch, on both platforms.
echo "==> cargo clippy (cli + host-core + worker + net + riscdom-backup + riscdom-sdk)"
# `--no-deps`: the crates we own are linted, their dependencies are only built.
cargo clippy -p cli -p host-core -p worker -p net -p riscdom-backup -p riscdom-sdk --all-targets --no-deps -- -D warnings || fail "cargo clippy cli + host-core + worker + net + riscdom-backup + riscdom-sdk"

# `cli/tests/{admin,control,read_only}.rs` are end-to-end: they drive a real `riscdom-server`,
# found through `RISCDOM_SERVER_BIN` or beside the test binaries. The control plane left for its
# own repository in v1.0 M8-4c, so with neither present those tests **skip** (and still count as
# passed) — which is normal here; see cli/README.md.
if have_guest_tools; then
  echo "==> cargo test (--include-ignored, minus the ones that need a key or the OS keyring)"
  cargo test --no-fail-fast -- --include-ignored --skip real_deepseek_writes_and_runs_hello_world --skip real_api_streams_content_deltas --skip os_keyring_persists_to_credential_manager || fail "cargo test"
else
  skip "the QEMU- and GCC-dependent tests (no qemu-system-riscv64 + RISC-V GCC on PATH)"
  echo "==> cargo test --workspace --no-fail-fast"
  cargo test --workspace --no-fail-fast || fail "cargo test --workspace"
fi

echo "==> typescript sdk (endpoint tables, client, stream)"
# No install step: the tests run on Node's own test runner with type stripping, so this needs no
# `node_modules`.
node --test sdk/typescript/test/*.test.ts || fail "typescript sdk"

echo "==> mirrored constants (host-core/src)"
node scripts/check-mirrored-constants.mjs || fail "mirrored constants"

echo "==> encoding scan (mojibake + BOM)"
scan_python="$(have_python)"
if [ -n "$scan_python" ]; then
  "$scan_python" scripts/scan-encoding.py --check || fail "encoding scan"
else
  skip "the encoding scan (no python3/python on PATH)"
fi

echo "==> tool schema documents (executor + control plane)"
node scripts/check-tool-schema.mjs || fail "tool schema"

python_bin="$(have_python)"
if [ -n "$python_bin" ]; then
  echo "==> python reference supervisor self-test (examples/python)"
  "$python_bin" examples/python/dispatch.py --self-test || fail "python reference supervisor"
  echo "==> python reference dispatcher self-test (examples/python)"
  "$python_bin" examples/python/supervisor.py --self-test || fail "python reference dispatcher"
else
  skip "the python reference self-tests (no python3/python on PATH)"
fi

echo "==> remote executor example self-test (worker)"
cargo run -q -p worker --example remote_executor -- --self-test || fail "remote executor example"

echo "==> node key example self-test (net)"
cargo run -q -p net --example identity -- --self-test || fail "node key example"

echo "==> signed message example self-test (net)"
cargo run -q -p net --example sign -- --self-test || fail "signed message example"

echo "==> transport example self-test (net)"
cargo run -q -p net --example transport -- --self-test || fail "transport example"

echo "==> discovery example self-test (net)"
cargo run -q -p net --example discovery -- --self-test || fail "discovery example"

echo "==> rooms example self-test (net)"
cargo run -q -p net --example rooms -- --self-test || fail "rooms example"

echo "==> relay example self-test (net)"
cargo run -q -p net --example relay -- --self-test || fail "relay example"

echo "==> packaging script syntax (scripts/pack.sh)"
# The packer is run by the `relay-bundle` CI job (batch BF, renamed in v1.0 M8-4c); a syntax
# error would only show up there, on a runner, so the gate parses it here on every commit.
sh -n scripts/pack.sh || fail "pack.sh syntax"

echo "==> bilingual doc links"
sh scripts/check-bilingual.sh || fail "bilingual links"

echo "gate: OK"
