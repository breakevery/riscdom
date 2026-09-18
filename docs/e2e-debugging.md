[中文](e2e-debugging.zh-CN.md) | English

# Debugging an end-to-end run

The end-to-end test drives a real run (mock LLM → write → compile → boot → read serial) and
needs QEMU plus a RISC-V GCC. It is `--ignored`, so it only runs when you ask for it:

```text
cargo test -p host --test e2e_ui -- --ignored --nocapture
```

It prints a **run diagnosis** on every run, pass or fail:

```text
--- run diagnosis (stage 5c-3) ---
outcome    : final after 6 iteration(s)
first failure: (none)
steps      : 4
  [ok ] write_source: wrote 353 bytes to hello.c
  [ok ] compile: compiled hello.c -> hello.elf (ok)
  [ok ] start_vm: VM started (qmp=54808, serial=54809)
  [ok ] read_serial: HELLO RISCV
serial     : 21 byte(s), tail "HELLO RISCV\n"
vm         : running
audit chain: Intact { length: 42 }
events     : agent:iteration x6, agent:tool_call x4, agent:tool_result x4, agent:final x1, serial:chunk x1, vm:state x2, preflight:progress x9
```

## How to read it

1. **`first failure`** answers "why did this fail". In order: the host refused the run
   (e.g. `qemu_missing`, with the search diagnostics), the run ended as something other than
   `final` (with its reason), or the first tool step that reported an error. `(none)` means
   nothing failed.
2. **`steps`** lists every tool execution in chain order, each with the first line of what it
   returned. The failing one is marked `[ERR]`, and a `compile` failure quotes the compiler's
   own output — that is usually the whole story.
3. **`serial`** reports how much the guest printed and the tail of it. `0 byte(s)` with
   *"the guest never printed anything"* is the signature of a guest that never wrote to the
   UART (it did not boot, or the kernel is wrong) — the plumbing is fine.
4. **`vm`** and **`audit chain`**: whether a guest is still in the host slot, and whether the
   log is intact. A broken chain means the log was tampered with; it is never the cause of a
   failed run.
5. **`events`** counts the host's own events; `serial:chunk x0` while `start_vm` reported
   success points at the guest, not at the sandbox.

One caveat worth remembering: a run can end as `final` **while a step failed** — the model
simply reports the failure in prose. The outcome kind is not the verdict; `first failure` is.

## Where it lives

- `host/tests/diagnosis/mod.rs` — pairing tool calls with results, naming the first failure,
  rendering the report.
- `host/tests/e2e_ui.rs` — prints it around the assertions (which are unchanged).
- `host/tests/run_diagnosis.rs` — pins the wording, on failure paths that need no QEMU.
