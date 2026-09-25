[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.9.1

> **This is a fix release, with the same two caveats as v0.9.0.** The **macOS and Linux packages are
> built by CI and have never been launched on a real machine**, and they are **unsigned** (macOS
> Gatekeeper blocks a first launch; Windows SmartScreen warns on the installer). And **Windows remains
> the platform the golden path has been verified on**: the clean-machine walk by somebody else still
> has not happened.

**v0.9.1 in one sentence:** it fixes the one thing that made the v0.9.0 desktop application unusable —
it opened on a login screen no input could pass.

## What this fixes

- **The desktop application stopped at the login screen** (v0.9.0). The front door asked **every**
  runtime for a token and drew the login screen when it found none — but the desktop holds no token at
  all: it never speaks to the control plane, because its host runs in the same process. The token it
  asked you to type could never be accepted either, because verifying one calls a `/v0/health` endpoint
  no desktop process serves. The desktop is now answered **before** any token is read, and the token
  gate lives in a separate component the desktop never reaches. Verified by hand on Windows: the shell
  opens, every settings tab switches, the chat and serial panels are there, the audit view lists the
  chain (223 events, intact) and the appearance tab's language and theme changes take effect at once.

## What is unchanged

Everything else is exactly as v0.9.0 shipped: the control plane and its CLI, Zig and Rust in the
sandbox, the management program and its Web board, and the multi-agent interface. **The known
limitations are the ones v0.9.0's release lists** and are not repeated here — see
<https://github.com/breakevery/riscdom/releases/tag/v0.9.0>. The packages are the same shape and the
same six assets, renamed for `0.9.1`.

## Where we are going

**Connecting out** — v1.0 will let the desktop app connect to an in-network RiscDom server, so one
board can oversee several nodes. (Today the desktop app runs the node it embeds.)

**Serving in** — v1.0 will also let the desktop app act as a local admin host, exposing the same
read-only board to phones and other devices on the LAN. (Today that board is served by
`riscdom-server --web-root`.)

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your
machine, the audit log lives outside the AI workspace and is append-only, and nothing is uploaded
anywhere. The control plane binds to loopback by default and requires a token unless it is started with
`--no-auth`; the browser board reads the same token from the data directory. QEMU and the RISC-V
toolchain are separate programs under their own licences; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
The full statement and the reporting process are in [SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
