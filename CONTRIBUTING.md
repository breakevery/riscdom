[中文](CONTRIBUTING.zh-CN.md) | English

# Contributing

Thanks for your interest in RiscDom. This project is built step by step, and every step must
be verifiable and rollback-able — contributions follow the same discipline.

## Development environment

- Windows 10/11 (the MVP is verified on Windows; QMP/serial go over TCP)
- QEMU (`qemu-system-riscv64`, verified with 11.1.0) — installed by you and found by RiscDom
  (`RISCDOM_QEMU` / `QEMU_SYSTEM_RISCV64` → known paths → `PATH`); `winget install
  SoftwareFreedomConservancy.QEMU` on Windows. See [docs/qemu-setup.md](docs/qemu-setup.md).
- RISC-V bare-metal GCC (`riscv64-unknown-elf-gcc`, verified with xPack 15.2.0)
- Rust / cargo (verified with 1.98.1) + the MSVC toolchain (Tauri needs it)
- Node / npm (verified with 24.11.1 / 11.16.0)

Exact paths and platform limits: [ENVIRONMENT.md](ENVIRONMENT.md).

**Never read or write a source file with PowerShell.** Use your editor's or agent's `edit` /
`write` path, as UTF-8 without a BOM: Windows PowerShell 5.1 decodes a BOM-less UTF-8 file as the
ANSI code page and re-encodes it, which turns `E2 80 xx` (an em dash, an ellipsis) into `U+9225`
plus a lost byte, `C2 A7` (`§`) into `U+6402`, and adds a BOM that was never there. PowerShell is
for *commands* (`git`, `gh`, `cargo`, `npm`, `node`, `python`). The gate's encoding scan looks for
that damage; see [docs/decisions.md](docs/decisions.md) §59.

## Local checks (the gate)

Run the gate before every commit:

```powershell
scripts\gate.ps1      # Windows
```

```text
sh scripts/gate.sh    # Unix
```

The gate is the **single list of what "green" means**: CI runs the same file
(`sh scripts/gate.sh` in `.github/workflows/ci.yml`), so a check cannot drift between CI and a
developer machine. In order: `cargo fmt --all -- --check` → `cargo clippy -D warnings` (the
workspace crates `audit` / `sandbox` / `agent` / `cli` / `server` / `host-core` / `host-tauri` /
`worker`, and `ui/src-tauri`) → `cargo check` →
`cargo test` → `cargo check` for `ui/src-tauri` → `npm run build` →
the ui regression probes (`node ui/scripts/probe-ui-*.mjs`) → the mirror guard
(`node scripts/check-mirrored-constants.mjs`) → the encoding scan
(`python scripts/scan-encoding.py --check` — mojibake and BOM; skipped, loudly, without Python) →
the wix-version guard
(`node scripts/check-wix-version.mjs`) → the ui string registry guard
(`node scripts/check-ui-strings.mjs`) → the bilingual-link check
(`scripts/check-bilingual.ps1` / `.sh`).

Platform differences are **printed, never skipped silently**: a test that needs a QEMU guest or a
RISC-V GCC carries `#[ignore = "<what it needs>; run with --include-ignored"]`, so a plain
`cargo test --workspace --no-fail-fast` runs the portable set everywhere. On a machine that has
the tools, run all of it with
`cargo test --no-fail-fast -- --include-ignored --skip real_deepseek_writes_and_runs_hello_world --skip real_api_streams_content_deltas --skip os_keyring_persists_to_credential_manager`
— the three `--skip`s are the tests that need a `DEEPSEEK_API_KEY` or write a real OS-keyring
entry, and `--skip` matches the **test name** (for an integration test, the function name, not
the file name). Two of `agent`'s unit tests compile C for real and print a skip when no GCC is
there. `--no-fail-fast` keeps one failing test binary from hiding the rest of the workspace.

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

### Keep the message ASCII — the `-m` path is lossy on Windows

**Measured** (2026-09-18, this machine): a message written in Chinese and passed as
`git commit -m "…"` never reaches the commit object intact. The command line crosses the
console's ANSI code page, so every non-ASCII character is replaced by `?` (`0x3F`) *before git
ever sees it*. A probe subject `test: 中文正文测试` was stored as `test: ?????????`, byte for byte
`74 65 73 74 3a 20 3f 3f 3f 3f 3f 3f 3f 3f 3f`.

Therefore:

- Write `git commit -m "…"` messages in ASCII (English). That is the norm in this repository.
- When a message must contain non-ASCII text, **do not use `-m`**: write the message to a file as
  **UTF-8 without BOM** and commit it with `git commit -F <file>`.
- Check what was actually stored before pushing: `git log -1 --format=%B`, and for the raw bytes
  `git log -1 --format=%B | Format-Hex`.

Example: commit `363e5ab` (*docs: add v0.3.1 release notes (post-tag)*) states its body in English
because of exactly this constraint — the Chinese wording would have been stored as `?`.

## Contributor License Agreement (CLA)

**If you contribute to this repository, you need to accept the [CLA](CLA.md) before a pull request
can be merged.** You keep the copyright in your contribution; the rights the CLA grants over it are
stated in CLA.md §3. (Contributions may be taken in somewhere other than this repository in the
future, so this section speaks only for the flow that exists here today.)

Sign it by commenting on the pull request with exactly this sentence, **in English**:

```text
I have read the CLA Document and I hereby sign the CLA
```

(In Chinese, for reference: 我已阅读 CLA 文档，并在此签署该 CLA。) The sentence is not translated —
the bot matches the English text exactly.

The [CLA Assistant](.github/workflows/cla.yml) bot verifies the signature and records it in
[`signatures/version1/cla.json`](signatures/version1/cla.json).

- **In this repository, a pull request whose author has not signed the CLA is not merged**; the
  check stays red until they do.
- Contributing as an employee, or on behalf of a company, is a corporate contribution: contact the
  project owner through the repository's issue tracker before the first pull request.
- A trivial fix (a typo, a broken link, a small documentation correction) may be accepted without a
  signature — CLA.md §9. Anything larger needs a recorded one.

## Pull requests

1. Fork the repository (or create a branch if you have write access).
2. Keep the change scoped; do not touch unrelated files, and never modify the host monitoring
   layer from AI-generated code paths.
3. Run the gate locally. **Every pull request must pass the gate in CI.**
4. Describe what changed, how you verified it (test output, screenshots), and any residual
   limits or follow-ups.
5. Security issues go through [SECURITY.md](SECURITY.md) — never a public issue.
6. The CLA check must be green — see the section above.

## Never commit

- API keys, tokens or credentials of any kind (this project is BYOK and ships no key; audit
  events, logs, Debug output and the frontend must never contain one)
- workspace or build outputs: `target/`, `node_modules/`, `*.db`, `*.jsonl`, `.env*`
- audit databases or session databases

`.gitignore` already covers most of this; CI also runs secret scanning (gitleaks, full
history).

## Debugging flaky tests

Flaky tests cost real time, so diagnose the **layer** before adding any defence.

1. **Get the evidence first.** Capture the failing output verbatim (the assertion message, the
   buffer, the timestamps). In this project a `read_serial` failure printed only `H` of
   `HELLO RISCV` — that single byte identified the bug immediately.
2. **Never assume the first plausible cause.** The same flake was first read as "the wait window
   is too short" and the window was widened (2 s → 5 s). That change was **useless**: the tool
   returned as soon as *any* byte arrived, so the window never applied. Fix the layer the
   evidence points at, not the layer that is convenient.
3. **Make failures self-explaining.** Returning an empty string made the model (and the log)
   guess; returning a notice ("guest may still be booting") made the next failure diagnosable.
   Prefer adding evidence to the failure path over guessing.
4. **Label defensive fixes as such.** A retry that cannot be triggered on demand (e.g. port
   TOCTOU under load) is defence, not proof. Say so in the commit and in the report, and keep the
   pressure test that covers it (`cargo test -p sandbox --test port_race -- --ignored`).
5. **Escalate deliberately.** If a flake survives two honest attempts at its own layer, switch
   designs (for port races that would be stdio transport) instead of adding more retries.
6. **Clean up between runs.** Interrupted test runs leave `target/debug/deps/*.exe` locked, which
   surfaces as `link.exe 1104`; stop the leftovers before re-running a gate.
7. **Read the run diagnosis.** An end-to-end run prints one on every attempt
   (`cargo test -p host-core --test e2e_ui -- --ignored --nocapture`); it names the first failing
   step and quotes that step's own output. What each line means:
   [docs/e2e-debugging.md](docs/e2e-debugging.md).
8. **Clean the temp directory when it piles up.** Every test keeps its workspace under the
   system temp directory, and they are never removed. `scripts/clean-temp.ps1` /
   `scripts/clean-temp.sh` clear RiscDom's entries (dry run by default; `-Force` / `--force`
   deletes). Only the `riscdom-` prefix is ever matched.

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
