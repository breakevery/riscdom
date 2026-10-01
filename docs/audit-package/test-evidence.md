[中文](test-evidence.zh-CN.md) | English

# RiscDom — test evidence

> **Where the numbers come from.** A real `scripts/gate.sh` run on the machine in
> [ENVIRONMENT.md](../../ENVIRONMENT.md) (Windows, QEMU 11.1.0 and xPack RISC-V GCC 15.2.0 on
> `PATH`), over commit `54f1f09` plus this package's own documents. Nothing here is quoted
> from an older run. The gate is the **one** list of what "green" means: CI runs the same
> file, verbatim.

## 1. Overview

| What | Value |
|---|---|
| Gate result | **`gate: OK`** (26 steps, every one green) |
| Tests run by the gate | **1018 passed; 0 failed** |
| Test binaries / suites reporting | **146** `test result:` lines (unit tests, integration tests and doc-tests) |
| Command the gate used | `cargo test --no-fail-fast -- --include-ignored --skip real_deepseek_writes_and_runs_hello_world --skip real_api_streams_content_deltas --skip os_keyring_persists_to_credential_manager` |
| Tests skipped by name | **3** (they need a model API key or the OS credential store) |
| `#[ignore]` attributes on disk | **63**, across **38** files |
| Bilingual pairs | **142** files when this section's run happened; **144** once this package is complete (this document is the last pair) |

The gate ran with **`--include-ignored`** because this machine has the guest tools, so the
tests that are normally `#[ignore]`d **did run** — except the three named skips above. That
is why `ignored: 0` everywhere in the log.

## 2. What the gate actually runs (26 steps, in order)

1. `cargo fmt --all -- --check`
2. `cargo clippy -p audit -p sandbox -p agent --all-targets -- -D warnings`
3. `cargo check -p audit -p sandbox -p agent`
4. `cargo clippy -p cli -p server -p host-core -p host-tauri -p worker -p net -p riscdom-backup -p riscdom-sdk --all-targets --no-deps -- -D warnings`
5. `cargo clippy --manifest-path ui/src-tauri/Cargo.toml --all-targets -- -D warnings`
6. `cargo check --manifest-path ui/src-tauri/Cargo.toml`
7. `cargo test` (the line above)
8. `npm run build` (the UI)
9. **16 UI probes** — scroll, width, runs, snapshot, dialog, preflight, theme, i18n, api, login, sse, read-only, node panel, network tab, LAN, executor picker (the last is `probe-ui-remote`)
10. `node --test sdk/typescript/test/*.test.ts`
11. `node scripts/check-mirrored-constants.mjs`
12. `python scripts/scan-encoding.py --check` (mojibake + BOM)
13. `node scripts/check-tool-schema.mjs`
14. `python examples/python/dispatch.py --self-test`
15. `python examples/python/supervisor.py --self-test`
16. `cargo run -q -p worker --example remote_executor -- --self-test`
17. `cargo run -q -p net --example identity -- --self-test`
18. `cargo run -q -p net --example sign -- --self-test`
19. `cargo run -q -p net --example transport -- --self-test`
20. `cargo run -q -p net --example discovery -- --self-test`
21. `cargo run -q -p net --example rooms -- --self-test`
22. `cargo run -q -p net --example relay -- --self-test`
23. `node scripts/check-wix-version.mjs`
24. `node scripts/check-ui-strings.mjs`
25. `sh -n scripts/pack.sh` (packaging script syntax)
26. `sh scripts/check-bilingual.sh` — the bilingual pairs

(The gate prints a **skip, with the reason**, for anything a platform cannot run — e.g.
the Python self-tests and the encoding scan on a machine without Python. Nothing is skipped
silently.)

## 3. Real QEMU vs mock

RiscDom's tests come in two shapes, and the difference matters to a reviewer:

- **A real QEMU guest.** The sandbox tests, and the host-core end-to-end tests, actually
  start `qemu-system-riscv64`, boot a bare-metal image, and read serial output. These are
  the tests that prove the product works, and they are the ones a machine without QEMU
  cannot run.
- **A mock model, but a real guest.** Most of `host-core`'s end-to-end tests use a **mock
  LLM** (so no key and no network) while still booting a real guest. `golden_path.rs` is the
  named one; the shape recurs in `dispatch`, `session_integration`, `vm_*`, `qemu_*` and the
  snapshot tests.
- **No guest at all.** Unit tests of parsing, storage, signing, routing, the route table and
  the capability vocabulary run everywhere.

Classified by what each `#[ignore]`d **file** touches (from its own content — a `qemu`
/ `gcc` / `mock` / `key` marker scan, not a per-test audit, so treat it as a map and not a
census):

| Group | Files (with an `#[ignore]` test) |
|---|---|
| Boots a real guest | `sandbox/tests/{smoke,snapshot,snapshot_real,qemu_discover,serial_observer,port_race}.rs`; `host-core/tests/{qemu_commands,qemu_injection,qemu_path_snapshot,vm_lifecycle,vm_poweroff,vm_status,run_diagnosis,run_provenance,golden_path,dispatch,e2e_ui,session_integration,snapshot_commands,serial_subscription,sandbox_request_tools,stream_forwarding}.rs`; `agent/tests/{vm_injection,zig_vm,read_serial_continuous,read_serial_empty,serial_subscribe,e2e_mock,stream_real}.rs` |
| Compiles C for real (a RISC-V GCC) | `agent/tests/{compiler_cleanup,compiler_discovery,compiler_parallel,tools}.rs` |
| Needs a model key or the OS keyring | `agent/tests/{real_api,stream_real}.rs`; `host-core/tests/{keyring_os,settings,preflight,toolchain_commands}.rs` |

Two `agent` unit tests compile C for real and print a skip when no GCC is present; the three
by-name skips are the ones that need a key or the credential store.

## 4. `#[ignore]` tests, by crate

**63** `#[ignore]` attributes sit in **38** files: **agent 15**, **host-core 40**,
**sandbox 8**. They are `#[ignore]`d because they need a real guest, a real toolchain or a
real key — a machine that has the first two runs all but three of them (as this one did).

- **`agent` (15)**: `compiler_cleanup` (1), `compiler_discovery` (1), `compiler_parallel` (1),
  `e2e_mock` (1), `read_serial_continuous` (1), `read_serial_empty` (1), `real_api` (1),
  `serial_subscribe` (3), `stream_real` (1), `tools` (1), `vm_injection` (2), `zig_vm` (1).
- **`host-core` (40)**: `session_integration` (6), `run_provenance` (4), `toolchain_commands`
  (4), `preflight` (3), `qemu_commands` (3), `run_diagnosis` (2), `sandbox_request_tools` (2),
  `serial_subscription` (2), `snapshot_commands` (2), `vm_status` (2), `dispatch` (1),
  `e2e_ui` (1), `golden_path` (1), `keyring_os` (1), `qemu_injection` (1),
  `qemu_path_snapshot` (1), `settings` (1), `stream_forwarding` (1), `vm_lifecycle` (1),
  `vm_poweroff` (1).
- **`sandbox` (8)**: `serial_observer` (2), `snapshot_real` (2), `port_race` (1),
  `qemu_discover` (1), `smoke` (1), `snapshot` (1).

## 5. What CI runs — and what it does not

CI has **one** gate job, and it runs **the very same `scripts/gate.sh`**. The deliberate
differences (recorded at the top of [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)):

- **The guest-booting tests do not run in CI.** A standard runner has no
  `qemu-system-riscv64` and no RISC-V bare-metal GCC, so the gate detects that and runs the
  **portable** `cargo test --workspace` instead.
- **`--ignored` tests never run in CI** — by design; they are run by hand.
- **Linux installs the system libraries** the gate needs (`libdbus-1-dev` for `keyring`;
  WebKitGTK and friends for the Tauri crates).

> **Read this before trusting a green badge.** **CI green does not mean the QEMU tests
> passed.** CI proves the portable code compiles, lints and passes its tests, and that the
> front end builds — it does **not** boot a guest. The guest-booting evidence is the local
> gate run in §1, on a machine with the guest tools.

## 6. The flake, stated plainly

The repo's oldest open item is the **QEMU and gate parallel flakes** — the QMP `10054`
family and a port race in the `net` loopback tests ([roadmap §12](../roadmap-v1.0.md), and
[known-issues.md](known-issues.md) §2). They are reported every time and never papered over.
In the runs behind this package the `net` transport/relay tests took the direct path where a
relay was expected once or twice, and passed on the next run — the same family. **No test is
`#[ignore]`d to hide a flake**; the flakes are timing races in the fixtures, and the gate is
re-run rather than weakened.
