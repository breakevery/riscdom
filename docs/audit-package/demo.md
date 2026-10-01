[中文](demo.zh-CN.md) | English

# RiscDom — a 30-minute demo

> **What this is.** Seven steps that show the whole product: install, configure a model, run a
> task, read the audit chain, snapshot, roll back, and dispatch across nodes. Each step gives
> the **purpose**, the **command**, the **expected output**, and an **acceptance point** you
> can check. Two scripts drive the parts that can be driven:
> [`scripts/demo.ps1`](../../scripts/demo.ps1) (Windows) and
> [`scripts/demo.sh`](../../scripts/demo.sh) (Unix).
>
> **The scripts are semi-automatic, on purpose.** Three things in this walk cannot be
> scripted honestly: **your model API key** (a secret), **a second node's configuration**
> (a decision about your network), and **whether the guest really printed `hello`** (a human
> reading the serial pane). The scripts run every automated step for real — they build, start
> a control plane, call the read-only and audit surfaces, and save/resume a snapshot — and
> **print the exact command** for each step that needs you to act. Nothing is faked to make a
> screen look green.

## Before you start

- **QEMU** (`qemu-system-riscv64`) and a **RISC-V bare-metal GCC** on `PATH`, and the Rust
  toolchain. The one-command check is step 1.
- **A control plane** to point at. Since v1.0 the CLI is a **pure client** — `--remote` is
  required and the CLI starts nothing. The scripts start a `riscdom-server` for you.
- **A model.** Either an API key for a hosted provider, or a local model (Ollama / LM Studio)
  for a fully offline walk.

Throughout, the scripts use a throwaway workspace and data directory under the repository's
`target/`, so a demo never touches your real `settings.json` or `audit.db`.

---

## Step 1 — Install / verify the environment

**Purpose.** Prove the machine can run the product at all.

**Command.** Build, then start a control plane on a loopback port and read its liveness:

```sh
cargo build --workspace
riscdom-server --bind 127.0.0.1:7821 --workspace target/demo-ws --data-dir target/demo-data
# in another console:
riscdom --remote 127.0.0.1:7821 health
```

**Expected output.** The server prints the address it bound and writes
`<data-dir>/token` on first start; `health` prints the node's status line.

**Acceptance point.** `health` answers, and `riscdom --remote 127.0.0.1:7821 --token wrong
health` exits **`4`** — a wrong token is refused with the server's own message, not a guess.

**Automated?** Yes — the script builds, starts the server, and checks the exit codes.

---

## Step 2 — Configure the model (BYOK)

**Purpose.** Point the node at a model. No key is provided, hosted or embedded by the project.

**Command.**

```sh
export DEEPSEEK_API_KEY="sk-..."
riscdom --remote 127.0.0.1:7821 llm set --provider deepseek --model deepseek-chat
# or a local model:
#   riscdom --remote 127.0.0.1:7821 llm set --provider ollama --base-url http://127.0.0.1:11434
```

**Expected output.** The node reports the configuration is active; `GET /v0/status` shows a
configured agent.

**Acceptance point.** With **no** model configured, `riscdom run "say hi"` fails cleanly with
the control plane's `unavailable` and exit code **`3`** — the node refuses rather than
guessing.

**Automated?** **No** — the key is yours. The script prints the command and waits; the
offline path (Ollama) needs no key.

---

## Step 3 — Run a task, end to end

**Purpose.** The whole loop in one command: the model writes code, it is compiled for RISC-V,
booted under QEMU, and the serial output is read back.

**Command.**

```sh
riscdom --remote 127.0.0.1:7821 run \
  "Write a RISC-V bare-metal hello, compile it, run it, and print hello over the serial console."
```

**Expected output.** The agent loop runs; the compiler produces an ELF; QEMU boots it; the
**serial pane prints the guest's `hello`**. `riscdom runs list` then shows the run.

**Acceptance point.** The serial output contains the word the task asked for, and the run
appears in `GET /v0/runs`.

**Automated?** **Partly.** The script runs the command when a key is present; **you** confirm
the guest's output.

---

## Step 4 — Read the audit chain

**Purpose.** See that every act was recorded, and that the record verifies.

**Command.**

```sh
riscdom --remote 127.0.0.1:7821 audit status
riscdom --remote 127.0.0.1:7821 audit events --limit 20
riscdom --remote 127.0.0.1:7821 export audit-jsonl --out target/demo-audit.jsonl
audit-verify target/demo-data/audit.db --runs
```

**Expected output.** `audit status` reports the event count and the chain's verdict;
`audit events` lists them newest first; `audit-verify` prints `Intact { length: N }` and
`RunIndex { findings: 0 }`.

**Acceptance point.** The independent checker (a *separate* binary, not the running app)
agrees the chain is intact. Then break a **copy**:

```sh
cp target/demo-data/audit.db target/demo-audit-copy.db
# in the copy only:
#   DROP TRIGGER audit_no_update; UPDATE audit_events SET action = 'evil' WHERE id = 2;
audit-verify target/demo-audit-copy.db --runs     # -> Broken, naming the id
```

The **real** file still verifies `Intact` — the triggers refuse the edit there. This is the
append-only property, shown rather than asserted.

**Automated?** Yes — the script runs the reads, the export and the checker, and runs the
"break a copy" experiment.

---

## Step 5 — Snapshot

**Purpose.** Capture the running guest's state.

**Command.**

```sh
riscdom --remote 127.0.0.1:7821 snapshots save demo-before
riscdom --remote 127.0.0.1:7821 snapshots list
```

**Expected output.** A snapshot named `demo-before` appears in the list.

**Acceptance point.** Saving **without** a running VM is an explicit error, not a silent
no-op — so this step also proves a VM is actually up.

**Automated?** Yes — the script saves and lists.

---

## Step 6 — Roll back

**Purpose.** Restore the guest to the snapshot.

**Command.**

```sh
riscdom --remote 127.0.0.1:7821 snapshots resume demo-before
```

**Expected output.** The guest is restored; the serial pane continues.

**Acceptance point.** The restore names the snapshot it used, and the run's history is still
readable.

**Automated?** Yes — the script resumes.

---

## Step 7 — Dispatch across nodes

**Purpose.** Show the third layer: a task running on **another node**.

**Command.** Start a second node with its **own** data directory (same directory means the
same token, and the check would pass trivially), then dispatch:

```sh
riscdom-server --bind 127.0.0.1:7822 --data-dir target/demo-data-b --workspace target/demo-ws-b
riscdom --remote 127.0.0.1:7821 tasks dispatch --target <peer_node_id> --input "boot a hello"
```

**Expected output.** The near node hands the task to the peer; the `task_id` the near node
gave it **travels with it**, so the two nodes' audit rows line up.

**Acceptance point.** The task runs on the second node, and `riscdom tasks` / the audit rows
on both nodes name the **same** `task_id`.

**Automated?** **No** — a second node is a network decision. The script prints both commands
and explains the `task_id` continuity to check for.

---

## Where the detail lives

- The mapping from capability to command: [capabilities.md](capabilities.md).
- The API endpoints behind every command: [control-plane-api.md](../control-plane-api.md) §5.
- The independent checker's contract: the `audit` crate's `audit-verify` binary.
- The manual walk a release goes through: [manual-acceptance.md](../manual-acceptance.md) and
  [golden-path-checklist.md](../golden-path-checklist.md).
