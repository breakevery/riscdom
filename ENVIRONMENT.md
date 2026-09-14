[中文](ENVIRONMENT.zh-CN.md) | English

# ENVIRONMENT.md — RiscDom development environment

> This file records the toolchain and platform limits verified on this machine (Windows).
> Nothing is written down until it has been proven by a real run.

## Platform

- OS: Windows (Windows_NT 10.0.22631 x64)
- Shell: PowerShell
- Host name: Z0624145651262
- Local repository path: `D:\codeagent\breakevery\riscdom`

## Toolchain versions (verified)

- QEMU: 11.1.0
  - Path: `C:\Program Files\qemu\`
  - Executable: `qemu-system-riscv64.exe`
  - Added to the system PATH
- RISC-V GCC: xPack GNU RISC-V Embedded GCC 15.2.0
  - Real prefix: `riscv-none-elf-`
  - Real path: `D:\tools\xpack-riscv-none-elf-gcc-15.2.0-1\bin`
  - Alias: `riscv64-unknown-elf-*` → `D:\tools\riscv64-unknown-elf\bin` (SymbolicLink)
  - Both paths are on the system PATH
- Rust: 1.98.1 (rustc / cargo), default toolchain `stable-x86_64-pc-windows-msvc`
  - Path: `C:\Users\cloud_user\.cargo\bin` (added to the user PATH)
- Node: v24.11.1
- npm: 11.16.0
- MSVC: BuildTools 2022, MSVC 14.44.35207
  - `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`
- git: 2.55.0.3 (`C:\Program Files\Git\cmd`)

## Known local environment traps (packaging / CLI)

- **`ELECTRON_RUN_AS_NODE=1`**: on this machine `node` is actually LobsterAI's Electron
  (as-node), so `process.argv[0]` is `LobsterAI.exe`. The `@tauri-apps/cli` wrapper therefore
  treats it as a subcommand and reports `unrecognized subcommand '<...LobsterAI.exe>'`.
  Workaround: use an argv-normalising launcher (rewrite `process.argv[0]` to `node`) and then
  require `node_modules/@tauri-apps/cli/tauri.js`. `build` works this way, verified.
- The first Tauri packaging run downloads WiX3 and NSIS from GitHub, so it needs network access.

## Windows platform limits (important)

- QMP (QEMU Monitor Protocol): use **TCP**, not a Unix domain socket.
  Example: `-qmp tcp:127.0.0.1:<port>,server=nowait`
- Serial: use a **TCP socket** or **file** redirection, not `mon:stdio`.
  Example: `-serial file:<path>` or `-serial tcp:127.0.0.1:<port>,server=nowait`
- Do not use `-nographic` + `mon:stdio` for automated capture (it is interactive and cannot be
  scripted).
- GDB / debug ports likewise go over TCP.

## Verified bare-metal boot pattern (smoke test)

```text
qemu-system-riscv64 \
  -machine virt -cpu rv64 -bios none \
  -kernel <elf> -display none \
  -serial file:<serial.log>
```

Key constraints:

1. Add `-mcmodel=medany` when cross-compiling (the 0x80000000 target is outside the medlow
   range).
2. `_start` must set up `sp` explicitly, otherwise the first stack push faults.
3. The `_start` entry must be linked first in the image (`.text.start` placed first,
   `ENTRY(_start)`), because with `-bios none` QEMU jumps straight to `0x80000000` and does
   not read the ELF `e_entry` offset.
4. The guest can exit on its own via the SiFive test finisher: write `0x5555` to `0x100000`.
