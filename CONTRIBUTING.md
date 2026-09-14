[中文](CONTRIBUTING.zh-CN.md) | English

# Contributing

Thanks for your interest in RiscDom. This project is built step by step, and every step must
be verifiable and rollback-able — contributions follow the same discipline.

## Development environment

- Windows 10/11 (the MVP is verified on Windows; QMP/serial go over TCP)
- QEMU (`qemu-system-riscv64`, verified with 11.1.0)
- RISC-V bare-metal GCC (`riscv64-unknown-elf-gcc`, verified with xPack 15.2.0)
- Rust / cargo (verified with 1.98.1) + the MSVC toolchain (Tauri needs it)
- Node / npm (verified with 24.11.1 / 11.16.0)

Exact paths and platform limits: [ENVIRONMENT.md](ENVIRONMENT.md).

## Local checks (the gate)

Run the gate before every commit:

```powershell
scripts\gate.ps1      # Windows
```

```text
sh scripts/gate.sh    # Unix
```

The gate runs, in order: `cargo fmt --all -- --check` → `cargo clippy -D warnings` →
`cargo check` → `cargo test` → `cargo check` for `ui/src-tauri` → `npm run build` →
the bilingual-link check (`scripts/check-bilingual.ps1` / `.sh`).

There is also a lighter preflight: `scripts/preflight.ps1` (Windows) /
`scripts/preflight.sh` (Unix).

## Gated commits

**Do not call `git commit` directly.** Use the wrapper, which runs the gate first and commits
only when it is green:

```text
scripts/commit.ps1 "feat(host): stage 20b persistent vm"     # Windows
./scripts/commit.sh "feat(host): stage 20b persistent vm"    # Unix
```

If the gate exits non-zero the wrapper exits 1 and **nothing is committed**.

## Commit messages

`type: subject`, where `type` is one of `feat` / `fix` / `docs` / `test` / `chore` /
`refactor` / `perf` / `build` / `ci`. Write the subject in the imperative mood, keep it under
about 70 characters, and mention the stage tag when the work belongs to a staged plan
(for example `docs: stage 22c contributing, coc, bilingual check`). One stage, one commit.

## Pull requests

1. Fork the repository (or create a branch if you have write access).
2. Keep the change scoped; do not touch unrelated files, and never modify the host monitoring
   layer from AI-generated code paths.
3. Run the gate locally. **Every pull request must pass the gate in CI.**
4. Describe what changed, how you verified it (test output, screenshots), and any residual
   limits or follow-ups.
5. Security issues go through [SECURITY.md](SECURITY.md) — never a public issue.

## Never commit

- API keys, tokens or credentials of any kind (this project is BYOK and ships no key; audit
  events, logs, Debug output and the frontend must never contain one)
- workspace or build outputs: `target/`, `node_modules/`, `*.db`, `*.jsonl`, `.env*`
- audit databases or session databases

`.gitignore` already covers most of this; CI also runs secret scanning (gitleaks, full
history).

## Documentation

Docs are bilingual: the English file is the main document (for example `README.md`) and the
Chinese translation lives alongside it as `*.zh-CN.md`, with a language switcher on the first
line:

```markdown
[中文](README.zh-CN.md) | English
```

Keep code blocks, commands, paths, configuration keys and API names untranslated. The gate's
bilingual check verifies that every pair exists and that the switcher lines point at each
other; it only reports, it never rewrites files.
