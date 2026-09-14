[中文](snapshot-experiment.zh-CN.md) | English

# Snapshot feasibility experiment (stage 18a)

> Bottom line first: **path A (QMP `migrate` to `file:` + `-incoming file:`) is unusable in
> this environment.** The migration engine itself works (a TCP transfer completes), and the
> failure is confined to the **file/exec transport channel in the Windows build**. This step
> therefore keeps the "reboot fallback" snapshot and leaves real snapshots open (candidate
> plans at the end).

## Environment

- OS: Windows (10.0.22631)
- QEMU: 11.1.0 (`v11.1.0-12130-ge470268ff4`),
  `C:\Program Files\qemu\qemu-system-riscv64.exe`
- Invocation matches the sandbox:
  `-machine virt -cpu rv64 -m 128M -bios none -display none -kernel hello.elf`
  - Serial: the experiment used `-serial file:<log>` (tcp vs file does not affect migration)
  - QMP: `tcp:127.0.0.1:<port>,server=on,wait=off`

## Experiment 1: `migrate` → `file:`

```text
uri: file:D:/.../snap1
reply: {"return": {}}
query-migrate: {"return": {"status": "failed",
                "error-desc": "Failed to set FD nonblocking: Input/output error"}}
snapshot file: 0 bytes
```

The source VM's serial output is fine (`HELLO RISCV`), so boot and the serial path are not the
problem.

## Experiment 2: variants

| variant | URI | result |
| --- | --- | --- |
| `exec:` (cat) | `exec:cat > D:/.../snap2` | `Failed to execute helper program (No such file or directory)` (no `cat` on Windows) |
| `exec:` (cmd) | `exec:cmd.exe /c more > D:/.../snap2` | same (QEMU does no PATH resolution / no shell redirection) |
| `file:` (leading slash) | `file:/D:/.../snap2` | `Could not create '/D:/.../snap2': Invalid argument` |
| `fd:` | `fd:<n>` | **not testable**: requires passing an inheritable file handle to QEMU; PowerShell cannot construct one, a native launcher would be needed |

## Experiment 3: control group — `migrate` → `tcp:` (isolating the failure)

Start a QEMU with `-incoming tcp:127.0.0.1:<port>` on the other side, then migrate from the
source VM:

```text
reply:        {"return": {}}
query-migrate: {"return": {"status": "setup"}}
event:         {"event": "STOP"}
query-migrate: {"return": {"status": "completed", "total-time": 66,
                 "ram": {"transferred": 495403, "mbps": 60.97, ...}}}
destination query-status: {"return": {"status": "running", "running": true}}
```

**The migration succeeded.** That shows:

- QEMU's migration engine, QMP control and RAM transfer all work fine here;
- the failure localises precisely to **the Windows build writing the migration stream to the
  file/exec channel** (`Failed to set FD nonblocking`), while the TCP channel is unaffected.

## Conclusion

1. Path A "migrate to a file" is **unusable** in this environment; the `exec:` variants are
   unusable too; `fd:` cannot be verified here.
2. Migrating to **TCP** works, but state lands in the peer QEMU's memory and is **not written
   to disk**, so it is not a file snapshot.
3. As agreed beforehand: **path B is not forced** (virtio-blk + qcow2 would require reworking
   the boot chain). This step keeps the reboot fallback and marks `[BLOCKED]` in
   `sandbox/README.md` and `PROJECT_CONSTITUTION.md`.

## Candidate follow-up plans (for decision; not implemented in this step)

- **Plan A′ (recommended, cheap)**: migrate over TCP and let a **local file relay** on the
  host side persist the stream — on save, accept QEMU's connection and write the bytes to a
  file; on restore, start a QEMU with `-incoming tcp:` and feed it the file. No new
  dependencies (`std::net` + `std::fs`) and **the `-kernel` start path is untouched** (only
  the restore adds `-incoming tcp:`).
- **Plan B (expensive)**: introduce virtio-blk + qcow2 and switch to QEMU
  `savevm`/`loadvm`; this requires reworking the boot chain and image management.

## Plan A′ results (stages 19b/19c)

**Implemented and verified.** The implementation differs slightly from plan A′ above
(corrected by measurement):

- **Save**: host/sandbox opens a local TCP listener (`MigrationRelay`) and issues `migrate`
  to that address; QEMU connects as a client and we persist the byte stream to
  `<snapshot_dir>/<name>.mig`. QEMU does not always close the socket after a migration, so
  the relay carries a **stop signal**: once QMP reports `completed` we finish up.
- **Restore**: `-incoming tcp:<addr>` means the **destination listens**, so our thread
  **connects to QEMU** and pushes the file (`relay::send_file_to`), then waits for
  `query-status` to report `running` before returning.
- Measured (`sandbox/tests/snapshot_real.rs`, fixture `hello_phases.c`): snapshot ~3.5 MB;
  after restore the serial continues with `PHASE2` past the migration point; serial and audit
  chain are both fine.
- A snapshot name that already exists is **refused** (an error is returned); the old snapshot
  is never silently replaced.

Residual limit: the host does not currently own a resident VM, so the UI can only **list /
delete** snapshots; "save/restore" waits for v0.3 (it needs the `AppState.vm` slot and
cross-run residency).

## Reproducing

Experiment scripts (not committed; they live in the session workspace): `experiment.ps1` /
`variants.ps1` / `tcp-variant.ps1`.
