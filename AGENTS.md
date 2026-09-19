[中文](AGENTS.zh-CN.md) | English

# RiscDom — AGENTS.md

> Cross-conversation handoff: [docs/handoff.md](docs/handoff.md) — start there when you take this
> project over. Section 1 is a volatile snapshot; sections 2–12 are the constraints that must hold.

## Project identity

English name: RiscDom
What it is: a desktop application. The AI holds virtual kernel-level privilege inside a
RISC-V virtual sandbox, where it can write C/assembly and drive virtual hardware. The whole
process is auditable and rollback-able. Humans keep root privilege. Edge capabilities are
plugins.

## Motto

Freedom inside boundaries, audit outside the AI, root privilege with humans.

## Project constitution

1. The host monitoring layer must not be modifiable by the AI.
2. The audit log lives outside the AI: append-only, cannot be disabled.
3. Capabilities are denied by default; plugins declare their permissions.
4. Humans always keep the right to pause, roll back, disconnect and terminate.
5. AI democracy is an experiment variable, not an MVP requirement.
6. During MVP the AI inside the sandbox may only generate C and RISC-V assembly.
7. AI access starts with API keys.

## Development discipline

- Every action writes an audit event.
- Touch only the given directories; do not cross boundaries.
- Show tests and diffs.
- Prefer slow over wrong: every step must be verifiable and rollback-able.

## Language limits

During MVP the AI inside the sandbox may only generate: C11
(`-ffreestanding -nostdlib -march=rv64gc -mabi=lp64d`) and RISC-V RV64GC assembly.
Forbidden: C++, Rust, Zig, Python.

## Red lines

- Never exfiltrate private data.
- Never run destructive commands without asking.
- Before changing configuration, inspect the current state; preserve/merge by default.
- Prefer trash over rm.
- When in doubt, ask first.
