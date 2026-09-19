[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.6.0-preview.1

> **This is a preview, and nothing in it has been walked by a human.** It has not been verified on a
> clean environment, and its **interface has not been walked on any machine at all** — the new block
> below is covered by automated tests and a source-level probe, not by a person clicking through it.
> The installers are **unsigned**, so Windows SmartScreen warns the first time you run one ("More
> info → Run anyway"). That is what `preview.1` in the version says.

**The golden path's eighth step in one sentence:** change one configuration field, run the task a
second time, select both runs in *Settings → Audit*, and read — field by field, in the fingerprint's
own order — exactly which configuration values differ. v0.5 made the first seven steps real; this
preview adds the comparison that had been left to v0.6.

## What this preview adds

- **Two runs, compared field by field** (*Settings → Audit*): selecting two runs already showed their
  fingerprints side by side; **under them** there is now a collapsed block whose header counts the
  fields and the differences — `字段级差异 · 7 个字段 · 3 处不同`, and `· 0 处不同` for two runs
  configured identically. Opening it lists **every field the two fingerprints carry** — the field
  name, the first run's value and the second run's value — with the changed rows highlighted and the
  unchanged ones dimmed. Values are shown whole (monospace, wrapped, never truncated), and the rows
  keep the order the fingerprint declares its fields in; the panel re-sorts nothing.
- **The comparison is the host's, not the interface's.** The two configuration documents are read off
  the audit chain's `run.start` events, compared as whole top-level fields, and judged equal by the
  same canonical JSON the fingerprint digest hashes — so key order inside a value is not a
  difference. The chain itself is untouched: the comparison is read-only and writes no event.

## What was verified — and what was not

**Verified**

- The gate: `cargo fmt`, `cargo clippy -D warnings`, `cargo check`, the full `cargo test`
  (**291 passed / 0 failed / 8 ignored**, 80 suites), `npm run build`, the seven UI probes, the
  mirror guard, the wix-version guard and the bilingual-documentation check. Green locally and in CI.
- The comparison's **data layer and API**: six unit tests and four integration tests — identical
  fingerprints, one changed field, several changed fields, a field only one side carries, nested
  values compared as a whole, `key order inside a value` and the guard that the declared field list
  cannot drift from the fingerprint the host actually builds.
- The comparison's **presentation rules**, as far as a probe can pin them: the header counts, the row
  states, how a value is rendered, the block being collapsed by default, and the CSS contract
  (highlight, dim, monospace, wrapping instead of truncation).
- The end-to-end walk of **steps 3–7** against a real QEMU guest, unchanged since v0.5.0
  (`cargo test -p host --test golden_path -- --ignored`).

**Not verified**

- **The interface itself.** No human has walked the new block on any machine. It has automated
  coverage (unit and integration tests, and a probe that pins source-level rules), but nobody has
  yet opened the app, run two runs that differ, selected them and read the diff. **This is the first
  thing a walk should do.**
- **A clean-machine walk.** Still the v0.5.x strengthening item it was: [docs/golden-path-checklist.md](docs/golden-path-checklist.md)
  is the form, `walkthroughs/` is where a filled one goes.
- **Step 8 against real guests.** `compare_run_fingerprints` is exercised with synthetic
  configurations; there is no `--ignored` walk that runs two real guests and compares them.
- **macOS and Linux.** Windows only; QMP and the serial console go over TCP.
- **The installers are unsigned.** Windows SmartScreen warns the first time you run one; "More info
  → Run anyway" is expected. Nothing is downloaded; the app ships no key.
- Chinese text entry through the UI is still verified only indirectly.

## What a tester needs

- **Windows 10/11**, **QEMU** (`qemu-system-riscv64`, installed by you — RiscDom guides you to
  `winget` or the official page and never bundles or downloads it), **a RISC-V bare-metal GCC**
  (xPack `riscv-none-elf-gcc` or an equivalent `riscv64-unknown-elf-gcc`), and **an API key** for the
  provider you choose (or a local provider such as Ollama / LM Studio).
- For the diff specifically: **two finished runs whose fingerprints differ** — the simplest way is to
  change one configuration field (the model, say) between them. A run that has not ended still has a
  fingerprint and can be compared; only the export refuses those.

## How to report back

1. Walk the steps with **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** open and
   fill it in as you go. For this preview, add what the diff did: whether the block opened, whether
   it listed every field, whether the rows that differ were the ones you changed, and what the header
   counted.
2. Open an issue at <https://github.com/breakevery/riscdom/issues> and **paste the filled checklist**;
   put it in `walkthroughs/` if you prefer the repository.
3. A walk nobody wrote down is a walk nobody can check — the filled template is the report.

## Known limitations

- **A pre-release takes no Latest marker**: GitHub still shows `v0.5.0` as the Latest release, so
  "the latest release" downloads v0.5.0, not this.
- **The interface is Chinese-only** (no language switch yet), and the diff block is one level deep:
  it compares top-level fingerprint fields as whole values, not the keys inside them.
- **Windows only**, and the interface's Chinese text entry has not been verified with a real keyboard.
- **No incremental or encrypted snapshots**; session storage is plain local SQLite.
- **One VM at a time.**
- **The audit log has no retention policy yet**: it grows with use and is never pruned.

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your
machine, the audit log lives outside the AI workspace and is append-only, and nothing is uploaded
anywhere. QEMU and the RISC-V toolchain are separate programs under their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full statement and the reporting process are in
[SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
