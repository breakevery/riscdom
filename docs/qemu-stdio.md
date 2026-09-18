[中文](qemu-stdio.zh-CN.md) | English

# QEMU stdio and the relay-port lease (v0.4 #1)

> **Status: proposal.** This batch writes no code. It answers what "QEMU stdio (option 3)"
> should mean for this codebase, what a unified port lease would be, and how the two relate to
> the TCP transport that ships today. §2 flags one thing that needs a decision before any
> implementation: the label "option 3" is not documented in this repository.

## 1. What we do today

Facts, as the code stands:

- **QMP** goes over TCP: `QmpEndpoint::tcp(host, port)` renders `-qmp tcp:host:port,server=on,wait=off`
  (`sandbox/src/platform.rs`). qmp_capabilities / stop / cont / quit ride on it.
- **Serial** goes over TCP: `SerialEndpoint::tcp(host, port)` renders
  `-serial tcp:host:port,server=on,wait=on` — `wait=on` means QEMU blocks until the host connects,
  which is what gates guest start-up on our reader being attached.
- **Ports** come from `sandbox::relay::free_local_port()`: bind `127.0.0.1:0`, read the assigned
  port, **drop the listener**, and hand the number to QEMU. Between the drop and QEMU's bind, any
  process on the machine may take that port (a *TOCTOU*).
- **What mitigates it today** is retries, not a design change: `start_vm` retries three times with
  fresh ports (`agent/src/tools.rs`), snapshot resume retries three times (`sandbox/src/vm.rs`),
  and the sandbox test helper `two_free_ports()` keeps a process-wide guard so tests do not hand the
  same port to two threads. The v0.3.0 "snapshot relay port retry" is exactly this class of local
  mitigation.
- **Two documented constraints already rule out the naive stdio design**: `ENVIRONMENT.md` says the
  serial must use a TCP socket or a file, *not* `mon:stdio` (interactive, cannot be scripted), and
  `sandbox/README.md` records "No Unix domain sockets, no `mon:stdio`". So "stdio" here cannot mean
  `-nographic` + `mon:stdio`.
- **Snapshot restore** additionally needs a port: QEMU is started with `-incoming tcp:127.0.0.1:…`
  and our relay connects and pushes the `.mig` stream (`sandbox/src/vm.rs`, `sandbox/src/relay.rs`).
- **Serial by file** is already a supported endpoint variant (`SerialEndpoint::file`), though the
  serial *reader* currently attaches only for the TCP variant.

## 2. What "option 3" means — needs confirmation

The label appears once, in `PROJECT_CONSTITUTION.md` §10 (v0.4 item 1): *"QEMU stdio (option 3) and
a unified relay-port lease: remove the TCP port dependency for QMP/serial altogether and hand out
relay ports through a single lease."* There is **no enumeration of options 1/2/3** anywhere in this
repository — no doc, no changelog entry, no comment.

The reading this proposal uses (to be confirmed or corrected before implementation):

- **QMP moves onto the child's own standard streams.** QEMU supports `-qmp stdio`; as the parent we
  already own the child's pipes, so the QMP channel becomes a pipe we read and write directly. No
  port, therefore no TOCTOU, for the control channel.
- **Serial stops needing a port too**, by using the chardev/file form (already supported as an
  endpoint variant) and a reader that tails that file. This is what makes the roadmap's "remove the
  TCP port dependency for QMP **and** serial" achievable without `mon:stdio`.
- **The ports that remain — the snapshot migration listener — move under one lease** (§3).

One consequence worth stating up front: a single process has a single stdout, so QMP-over-stdio and
a stdio serial cannot both be used; serial must take a different channel. That is a fact about the
mechanism, not a preference.

## 3. The relay-port lease

**Granularity: one lease per host process.** Not per run and not per VM configuration. Reasons:

- The VM outlives a run (host-owned `vm_slot`), and the snapshot relay's lifetime is a migration,
  not a run.
- The failure this prevents is *two parts of the same program* choosing the same number at the same
  moment (or a part choosing a number another part still holds). A process-wide allocator is exactly
  the scope of that problem; a global/OS-wide lease would need privileges we do not have and would
  not survive a second instance of the app.
- Today's mitigation is already process-wide in tests (`two_free_ports()` in the sandbox test
  helpers) — the lease generalises what the tests already do.

Shape of the lease (proposal): one allocator that (a) binds `127.0.0.1:0` to have the OS pick a free
port, (b) records the port as *held* while the holder still needs it, and (c) refuses to hand the
same port to a second holder until the first releases it. It does **not** eliminate the TOCTOU by
itself — we still drop the listener before QEMU binds — so it is a *narrowing* of the window plus a
single place to retry, not a proof. Removing the QMP port (stdio) is the part that removes a port
from the equation entirely.

## 4. Interaction with the snapshot relay

- The restore path is the one that must keep a TCP listener on QEMU's side: `-incoming tcp:` makes
  **QEMU** listen, and our relay connects and pushes. So the port has to be chosen before QEMU
  starts, and QEMU then binds it — the same TOCTOU as the QMP/serial case.
- Under the lease, that port is taken from the allocator and released when the migration finishes
  (success or failure), so a snapshot restore can no longer collide with a QMP/serial port choice,
  nor with a concurrent second restore attempt.
- The existing three-attempt retry stays: the lease narrows the race, the retry covers what is left,
  and both are needed until QEMU can be handed a pre-bound socket (it cannot, with today's flags).

## 5. Backward compatibility

- **Keep the TCP path.** It is the shipping transport, it is what every existing test exercises, and
  the project's platform story is "Windows + TCP only" — replacing it outright would be a rewrite
  whose only beneficiary is the port race.
- **Make stdio opt-in first** (a configuration switch, default off), then consider it the default in
  a later version once the e2e suite has run both paths for a while.
- Snapshot `.mig` files, audit events and the run fingerprint are unaffected: the transport is not
  part of any persisted format. (The fingerprint does not currently record the transport; if the
  transport becomes configurable, it should — a run's result depends on it.)

## 6. Phasing

**v0.4 (proposal):**

1. The lease: one allocator in `sandbox` (or `host`, if the host should own the policy) that tracks
   held ports; `start_vm` and snapshot resume take ports from it. Small, testable, no protocol change.
2. QMP over stdio behind a config switch, with the serial on the file variant, plus a reader for it.
3. Tests: the existing port-race pressure test keeps passing; a new test starts a guest with stdio
   QMP and reads the serial from the file.

**Later:** make stdio the default, drop the QMP TCP path, and (only then) retire the retries.

## 7. Crates this touches

- **`sandbox` — yes.** Endpoints (`platform.rs`), the VM's QMP/serial wiring (`vm.rs`), the relay
  (`relay.rs`).
- **`agent` — yes, minimally.** `start_vm` builds the VM config, so it must pass the transport
  choice through.
- **`host` — yes, minimally.** The resume path builds its own config, and the serial forwarder reads
  the serial stream.
- `audit` — untouched.

Because this touches `sandbox`, it needs the same authorisation the project requires for that crate.
