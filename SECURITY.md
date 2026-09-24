[中文](SECURITY.zh-CN.md) | English

# Security policy

## Supported versions

Only the latest release and the `main` branch receive security updates.

## Reporting a vulnerability

Please report privately through a **GitHub Security Advisory**; do not open a public issue.

Never attach a real API key to a report. If you need a demo, use a revocable temporary key.

## Key handling conventions

- This project **provides no** API key; all model access is bring-your-own-key (BYOK).
- The audit log **never contains** keys (`agent.llm.request` records a hash and token counts
  only; keyring events record the `provider_id` only).
- The frontend **never persists** keys (`localStorage` holds only the boolean "remember key"
  preference).
- The OS keyring is used for persistence (from v0.2); when it is unavailable the app silently
  degrades to memory only.
- The `DEEPSEEK_API_KEY` environment variable is adopted into memory at startup and is
  **never** written to the keyring automatically.

## CI scope

`.github/workflows/ci.yml` runs only checks that work cross-platform:

- secret scanning (gitleaks over the **full history**; it uses the community binary instead
  of `gitleaks-action`, because that action needs `GITLEAKS_LICENSE` for private
  organisation repositories)
- Rust: `fmt --check`, `clippy -D warnings`, `check`, `audit --lib` tests
  (**portable crates only**: `audit` / `sandbox` / `agent`)
- Frontend: `npm ci` + `npm run build`

`.gitleaksignore` exempts exactly one historical hit by **fingerprint**: a **fake
placeholder key** used in an early unit test to exercise the masking logic (already replaced
with a non-key string in commit `d5daf4f`; it only survives in history). That is a
single known false positive and does not weaken the rest of the full-history scan.

**Complete tests** (end-to-end for `sandbox` / `agent` / `host-core`) need a local QEMU
(`qemu-system-riscv64`) and a RISC-V cross compiler (`riscv64-unknown-elf-gcc`), which the
standard runners do not have; developers run `cargo test` locally.

`host-tauri` depends on Tauri and needs system libraries on Linux (webkit2gtk / gtk), so CI does
not build `host-tauri`; the MVP targets Windows, where it is linted and checked locally. The
crates that carry no Tauri — `cli`, `server` and `host-core` — are linted on both platforms.
