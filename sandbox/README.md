[中文](README.zh-CN.md) | English

# sandbox

The RiscDom **QEMU RISC-V sandbox layer**.

It boots a RISC-V bare-metal ELF on the QEMU `virt` machine, controls it over QMP, captures
serial output, and offers (MVP-fallback) snapshot/rollback. Every outbound operation is
audited.

## Modules

- `vm` — `RiscVVirtualMachine` / `VMConfig`: QEMU lifecycle, serial capture, snapshots
- `platform` — `QmpEndpoint` / `SerialEndpoint`: platform endpoint abstraction
  (endpoint → QEMU arguments)
- `qmp` — `QmpClient`: minimal QMP client (greeting / `qmp_capabilities` / `stop` / `cont` /
  `quit`)
- `error` — `SandboxError`

## Auditing (the audit crate)

Auditing comes from the **`audit` crate** (append-only SQLite + SHA-256 hash chain). The
dependency direction is `sandbox → audit`; `sandbox` defines no audit types of its own.

- Constructors take `Arc<Mutex<dyn AuditSink>>` (`AuditSink::record` takes `&mut self`).
- It writes SQLite directly (append-only); no JSONL placeholder any more.
- The event `actor` is always `"sandbox"`; current types:
  `vm.start` / `vm.stop` / `vm.snapshot.save` / `vm.snapshot.load` / `serial.read` /
  `serial.write`.

`audit::FileAuditSink` is kept in the `audit` crate as an **example only**; use
`audit::SqliteAuditSink` in production.

## Platform limits

1. **Windows: everything goes over TCP.** QMP uses `tcp:host:port,server=on,wait=off` and
   the serial uses `tcp:host:port,server=on,wait=on`. No Unix domain sockets, no `mon:stdio`.
2. **Unix:** `QmpEndpoint::UnixSocket` is reserved at the type level (`#[cfg(unix)]`) but is
   not implemented in the MVP; connecting returns `SandboxError::Unsupported`. Only Windows
   is currently tested.
3. **Why serial `wait=on`:** QEMU blocks until the host connects to the serial socket before
   running the guest, which guarantees early boot output is not lost (this is the fix for the
   race between the capture thread and the guest).
4. **`-bios none` is required:** otherwise the default OpenSBI firmware occupies
   `0x80000000`. The guest ELF entry must be linked first in the image (see `.text.start` in
   `tests/fixtures/link.ld`), because with `-bios none` QEMU jumps to `0x80000000`.
5. The host must have `qemu-system-riscv64` installed. Set `RISCDOM_QEMU` to its absolute
   path; otherwise common install locations / `PATH` are used.

## MVP snapshot fallback

`save_snapshot` / `load_snapshot` are **not real virtual-machine snapshots**:

- `save_snapshot(name)`: serialises `(kernel path, memory_mb, qemu args, timestamp)` to JSON
  under `<snapshot_dir>/<name>.json` with `mode: "mvp-reboot"`.
- `load_snapshot(name)`: stops the current VM, reads the JSON and restarts with the same
  parameters.

In other words, an MVP "rollback" equals "restart with the same parameters". Device state and
memory are not saved.

### Real snapshots: `[BLOCKED]` (stage 18a measurement)

Path A — QMP `migrate` to a file — was attempted and is **unusable in this environment**:

- `migrate` → `file:<path>`: `Failed to set FD nonblocking: Input/output error` (0-byte file)
- `exec:` variants: `Failed to execute helper program` (no `cat` on Windows; QEMU does no
  PATH resolution)
- `file:/<path>` form: `Could not create ... Invalid argument`
- Control group: `migrate` → `tcp:` **succeeded** (495,403 bytes, destination
  `status: running`) → the migration engine works; **only the Windows file/exec transport
  channel fails**

The full experiment log is in [`docs/snapshot-experiment.md`](docs/snapshot-experiment.md).
As agreed, path B (virtio-blk + qcow2) was not forced; the **reboot fallback is kept**.

## Tests

```text
cargo test -p sandbox
```

- `tests/smoke.rs` (3a): boot QEMU → capture `HELLO RISCV` → stop → check the audit chain is
  Intact
- `tests/snapshot.rs` (3b): boot → save snapshot → stop → load → boot again successfully
- `tests/fixtures/`: a minimal RISC-V bare-metal guest (`hello.c` + `link.ld`)

Compiling the fixture needs `riscv64-unknown-elf-gcc` (set `RISCDOM_RISCV_GCC` to override the
path).

## Example

```text
cargo run -p sandbox --example run_hello
```

It boots the guest, prints the serial output, writes the audit log to SQLite and calls
`verify_chain`, printing the result.

You can double-check it with a standalone tool:

```text
cargo run -p audit --bin audit-verify -- <path-to-db>
```

## v0.2 TODO

- **Real snapshots `[BLOCKED]`**: path A (`migrate` → file) is unusable on Windows + QEMU
  11.1.0; v0.3 evaluates **plan A′** (migrate over TCP plus a host-side file relay, no new
  dependency, `-kernel` start path untouched) or plan B (virtio-blk + qcow2 +
  `savevm`/`loadvm`, which needs the boot chain reworked)
- Unix socket support (macOS / Linux, `QmpEndpoint::UnixSocket`)
- virtio devices (block / network)
- ~~sandbox-driven serial callbacks~~ ? done (stage 15a)
