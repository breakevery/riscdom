[中文](THIRD_PARTY_NOTICES.zh-CN.md) | English

# Third-party notices

RiscDom itself is Apache-2.0 ([LICENSE](LICENSE)). It **links none of the software below**: each
runs as a separate process, started by RiscDom and talked to over a pipe or a TCP socket. This file
states what those programs are licensed under. It is a statement of facts, not a legal opinion, and
it does not decide whether any distribution model is compliant — that question belongs to
[docs/qemu-distribution.md](docs/qemu-distribution.md) §4.

## QEMU

- **Licence: GPL-2.0.** Components inside a given QEMU build may carry other licences; the
  `COPYING` / `LICENSE` files that build ships are the authority for that build.
- **How RiscDom uses it**: `qemu-system-riscv64` is started as a child process and driven over QMP
  and a serial connection. No QEMU code is linked into RiscDom, and no QEMU source is modified.- **How it reaches the user**: the user installs QEMU themselves ([docs/qemu-setup.md](docs/qemu-setup.md)) — guided to `winget` or the official download page. RiscDom does not download, host, mirror or redistribute QEMU.

## RISC-V toolchain (xPack `riscv-none-elf-gcc`)

- **Licence: GPL-3.0-or-later, with the GCC Runtime Library Exception.** That exception covers what
  a program compiled *with* GCC has to do with GCC's own runtime libraries; it does not relicense
  GCC itself.
- **How RiscDom uses it**: invoked as a separate program to compile a freestanding guest. RiscDom
  links none of it.
- **How it reaches the user**: RiscDom's in-app download fetches the archive from the xPack release
  and verifies the published SHA-256 (`host-core/src/toolchain_download.rs`). Any other RISC-V GCC can be
  installed by hand and pointed at instead.

## Rust dependencies

Each crate carries its own licence; `Cargo.lock` plus the crates' own metadata is the source of
truth. They are not listed here because the list changes with every dependency bump, and a copy in
this file would go stale without anyone noticing.
