[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.5.0-preview.1

> **This is a preview, and it has not been verified on a clean environment.** It is the build this
> repository's own gate and end-to-end test cover — not one a stranger has walked yet. That walk is
> exactly what this preview is for, and it is the reason the version says `preview.1`.

**The golden path in one sentence:** install RiscDom on Windows, point it at QEMU and a RISC-V
compiler, configure a model, run an agent task, save a snapshot, export that run's audit record,
roll back to the snapshot, then change one configuration field and run again — seven steps, by hand,
in the app. v0.5 makes all seven real; the automatic comparison of the two runs is v0.6.

## What this preview adds

- **Get the audit record** (*Settings → Audit*): export one run as JSONL. The file is
  **self-contained** — it runs from the chain's first event to the event that closes the run — so
  you can put it in an empty database and have `audit-verify` judge it without this machine.
- **Roll back and see where you came from**: a run that restored a snapshot says so
  ("恢复自 <snapshot>"), and two runs can be selected for a side-by-side view of their fingerprints,
  start time and status.
- **A manual checklist** for the first two steps, and an `--ignored` end-to-end test that walks
  steps 3–7 against a real QEMU guest.
- **A contributor licence agreement**, and a README whose licence note is two lines again.

## What a tester needs

- **Windows 10/11** (the only verified platform).
- **QEMU** — `qemu-system-riscv64`, installed by you. RiscDom guides you to
  `winget install SoftwareFreedomConservancy.QEMU` or the official download page; it neither bundles
  nor downloads QEMU.
- **A RISC-V bare-metal GCC** — xPack `riscv-none-elf-gcc` or an equivalent `riscv64-unknown-elf-gcc`.
  RiscDom auto-detects it, or you point it there by hand.
- **An API key** for the model provider you choose (or a local provider such as Ollama / LM Studio;
  RiscDom ships no key and none leaves your machine).

## How to report back

1. Walk the steps with **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** open, and
   **fill it in as you go** — it asks for the machine, the OS build, the QEMU and GCC versions *and
   where each came from*, the provider and model, the preflight's four steps, the two runs' short
   fingerprints, the exported file's path and its `audit-verify` verdict, and anything that failed
   (which step, the raw error, whether it was recoverable).
2. Open an issue at <https://github.com/breakevery/riscdom/issues> and **paste the filled checklist**.
3. A walk nobody wrote down is a walk nobody can check — the filled template is the report.

## Verification

- `cargo test`: **281 passed / 0 failed / 8 ignored** across **79 test suites**. The ignored tests are
  the ones designed to need a real API key or a real QEMU boot; the steps 3–7 walk is one of them and
  passes on this machine (`cargo test -p host --test golden_path -- --ignored`).
- The gate — `cargo fmt --check`, `cargo clippy -D warnings`, `cargo check`, the full `cargo test`,
  `npm run build`, six UI probes, the mirror guard, the bilingual-documentation check — passes locally
  and in CI (`scripts/gate.sh` is the single list of what "green" means).
- **Not verified:** a real first walk on a clean machine with a real API key. That is this preview's
  open item, and the checklist is how it gets closed.

## Known limitations

- **Windows only.** macOS and Linux are not supported or verified yet; QMP and the serial console go
  over TCP.
- **A preview has no upgrade promise.** Configuration formats and the audit schema are the ones the
  final v0.5.0 is expected to keep, but a preview exists to find out.
- **The interface is Chinese-only**, and there is no language switch yet.
- **No incremental or encrypted snapshots**; session storage is plain local SQLite.
- **One VM at a time** — the GUI drives a single host-owned VM.
- The MSI's installer version is pinned separately (`0.5.0.1`) because `preview.1` is not a valid MSI
  `ProductVersion`; the artifact names still carry `0.5.0-preview.1`.

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your
machine, the audit log lives outside the AI and is append-only, and nothing is uploaded anywhere.
QEMU and the RISC-V toolchain are separate programs under their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full statement and the reporting process are in
[SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
