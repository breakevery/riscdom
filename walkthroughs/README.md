# Walkthroughs

Evidence for the v0.5 release gate: one file per walk of the golden path, recorded as it happened.

- **What goes here.** A filled copy of `docs/golden-path-checklist.md` (plus anything the walker had
  to add) from a real machine — the developer's own machine or an external tester's.
- **Why it is not in `docs/`.** These are records, not documentation. `check-bilingual.sh` requires
  every document to exist in both languages; a record of what one person saw on one machine cannot
  be translated without inventing a second one, so this directory is excluded from that check.
- **This directory is outside the bilingual gate.** `scripts/check-bilingual.sh` skips
  `./walkthroughs/*` explicitly (nothing else is excluded by that rule — `docs/` and the repository
  root are still checked). So a file here is **not** verified at all: if you ever put a translated
  document in this directory, you maintain it by hand, and the gate will neither see the file nor
  check that its language switcher points at anything.
- **What the gate needs.** `docs/handoff.md` §8: v0.5 ships when steps 1–2 have been walked on a
  clean machine with a real API key and recorded against the checklist. A walk nobody wrote down is
  a walk nobody can check.
- **Naming.** `YYYY-MM-DD-<release>-<who>.md`, e.g. `2026-09-19-preview1-local.md`.

Files so far:

| File | Machine | Verdict |
|---|---|---|
| [2026-09-19-preview1-local.md](2026-09-19-preview1-local.md) | the developer's machine (**not** a clean environment: QEMU and GCC were already installed), installed MSI, real model and key | seven steps passed; findings listed in the record |
