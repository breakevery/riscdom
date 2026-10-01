[中文](dependencies.zh-CN.md) | English

# RiscDom — dependencies, and which ones are trusted for what

> Gathered from the workspace's `Cargo.toml` files, [ENVIRONMENT.md](../../ENVIRONMENT.md) and
> [README.md](../../README.md). Versions are the ones this project is **verified** against on
> the machine in `ENVIRONMENT.md`; "(verified)" means a real run happened, not that a range
> was chosen. At `v1.0.0`.

## 1. Runtime dependencies (what a user must have)

| Dependency | Version this is verified with | Why it is needed |
|---|---|---|
| **QEMU** (`qemu-system-riscv64`) | **11.1.0** | The sandbox. RiscDom **never bundles or downloads it** (a decision, not an oversight). RiscDom finds it via `RISCDOM_QEMU` / `QEMU_SYSTEM_RISCV64`, well-known install locations, then `PATH`; a user can point at a copy in *Settings → Toolchain → QEMU*. Windows: `winget install SoftwareFreedomConservancy.QEMU`. |
| **RISC-V bare-metal GCC** | **xPack 15.2.0** (`riscv-none-elf-gcc`), or an equivalent `riscv64-unknown-elf-gcc` | Compiles the guest. RiscDom **can fetch it for you**: *Settings → Toolchain → one-click download* pulls the official xPack build (~200 MB), **checks its SHA-256** and installs it under the app data directory. Auto-detected via `RISCDOM_RISCV_GCC` / `RISCV_GCC`, well-known paths, then `PATH`. |
| **The OS credential store** | platform's own | Holds the API key (when "store" is ticked) and the node's `node.key`: Windows Credential Manager / macOS Keychain / Linux Secret Service, through the `keyring` crate. |

A **local model** (Ollama / LM Studio) can replace a hosted provider entirely — **no API key
needed** for fully offline operation.

## 2. Build dependencies (what a contributor must have)

| Dependency | Version (verified) | Notes |
|---|---|---|
| **Rust / cargo** | **1.98.1** | Default toolchain `stable-x86_64-pc-windows-msvc`. |
| **MSVC toolchain** | BuildTools 2022 (MSVC **14.44**.35207) | Required by Tauri. |
| **Node / npm** | **24.11.1** / **11.16.0** | The UI probes and the TypeScript SDK tests import `.ts` directly (Node ≥ 22.6 with `--experimental-strip-types`; default from 23.6). |
| **Tauri** | **2** | The desktop shell (`host-tauri` + `ui/src-tauri`). |
| **Python 3** *(optional)* | any | Runs `examples/python`'s two self-tests and the encoding scan; the gate **prints a skip** when it is absent rather than failing. |
| **Linux-only dev packages** | — | `webkit2gtk` / `gtk` / `librsvg` / `libsoup` for the two Tauri crates, and `libdbus-1-dev` for `host-core` through `keyring`. CI installs them. |

## 3. Key Rust crates (direct dependencies)

Grouped by what they are for. "No new package" notes in the manifests mean an edge was added
to a crate already in `Cargo.lock` — the lock does not grow.

**Control plane (`server`)**

| Crate | Version | For |
|---|---|---|
| `hyper` | 1 (`server`, `http1`) | the HTTP/1.1 server |
| `tokio` | 1 (`net`, `rt`, `io-util`, `sync`) | the IO driver and the broadcast channel (no timer feature; the heartbeat runs on its own thread) |
| `http-body-util`, `futures-util` | 0.1 / 0.3 | the SSE body bridge |
| `getrandom`, `subtle` | 0.4 / 2 | the bearer token's randomness and its constant-time comparison |

**Audit & storage**

| Crate | Version | For |
|---|---|---|
| `rusqlite` | 0.32 (`bundled`) | the append-only store (SQLite, compiled from the upstream amalgamation — **no system SQLite**) |
| `sha2` | 0.10 | the hash chain |
| `hex` | 0.4 | hex encoding of hashes |

**Agent & HTTP egress**

| Crate | Version | For |
|---|---|---|
| `reqwest` | 0.12 (`blocking`, `json`, `rustls-tls`) | the LLM client and the CLI/SDK client (**rustls**, so no system OpenSSL) |
| `uuid` | 1 (`v7`) | sortable run ids |

**Connection layer (`net`)**

| Crate | Version | For |
|---|---|---|
| `ed25519-dalek` | 2 | the node identity's Ed25519 signing (pure Rust) |
| `base64` | 0.22 | base64url for the JWK members (RFC 8037) |
| `getrandom` | 0.4 | the OS random source, for minting a key |

**Archive / portability**

| Crate | Version | For |
|---|---|---|
| `zip`, `flate2`, `tar`, `xz2` | 2 / 1 / 0.4 / 0.1 | workspace import/export and the backup package |
| `ring` | 0.17 | the backup package's AEAD (AES-256-GCM) and PBKDF2 |

**Desktop**

| Crate | Version | For |
|---|---|---|
| `tauri` (+ `tauri-build`, `tauri-plugin-dialog`) | 2 | the desktop shell |

**Supporting, everywhere**: `serde`, `serde_json` (1), `thiserror` (1), `keyring` (3, with
the per-OS native backend chosen explicitly — a bare `keyring = "3"` would compile to a
silent no-op).

**Count**: **25** direct external crates plus the workspace-internal path dependencies.

## 4. The trusted ones — and why

These are the dependencies whose correctness the security story stands on. The test is:
**standard, mature, and not invented here.**

| Dependency | Trusted for | Why it is trusted |
|---|---|---|
| **QEMU** | the guest boundary — everything the AI writes runs inside it | A mature, widely used emulator, separate from this project and under its own licence; the project neither vendors nor downloads it, so the boundary is the user's own QEMU. |
| **RISC-V bare-metal GCC (xPack)** | the compiler that turns the model's C into the guest | The official xPack build; the in-app download **verifies its SHA-256** before installing. |
| **`ed25519-dalek`** | the node identity and every signed frame | The reference Rust implementation of Ed25519 (dalek-cryptography); the JWK's raw 32-byte keys are exposed directly, so no bespoke curve code exists here. |
| **`rusqlite` (bundled SQLite)** | the audit store | SQLite is the most widely deployed embedded database; `bundled` compiles the upstream amalgamation, so what is trusted is SQLite itself, not a wrapper. |
| **`ring`** | the backup package's encryption | A widely used, audited cryptography library based on BoringSSL; the project chooses **standard primitives** (AES-256-GCM, PBKDF2-HMAC-SHA256) rather than designing its own. |
| **`rustls`** (through `reqwest`) | TLS for model and control-plane traffic | Deliberately chosen over a system OpenSSL so the TLS stack is a Rust one the project can pin. |
| **The OS credential store** (`keyring`) | the API key and the node key at rest | The platform's own store (Credential Manager / Keychain / Secret Service); no bespoke key file is invented for secrets. |

**What is *not* here**: no bespoke cryptography. `compute_hash`, the signature scheme, the
AEAD and the KDF are all standard primitives or standard crates — the project's own code is
about *what to record and how to link it*, not about inventing primitives.
