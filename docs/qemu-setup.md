[中文](qemu-setup.zh-CN.md) | English

# QEMU setup

RiscDom boots the guest with **`qemu-system-riscv64`**. It is not bundled and it is not downloaded
for you (unlike the RISC-V GCC): install it, and RiscDom finds it — or you point the app at it.

## 1. Install QEMU

**Windows**

```text
winget install SoftwareFreedomConservancy.QEMU
```

Or download an installer from <https://www.qemu.org/download/#windows>. Either way the executable
ends up as `qemu-system-riscv64.exe` (typically `C:\Program Files\qemu\`).

**macOS**

```text
brew install qemu
```

**Linux**

Use your distribution's package (`qemu-system-misc` on Debian/Ubuntu provides the RISC-V system
emulator; `qemu` on Arch/Fedora), or build from <https://www.qemu.org/download/>.

## 2. Let RiscDom find it

`sandbox::qemu_discover` resolves the binary in this order and stops at the first hit:

1. `RISCDOM_QEMU` — full path to the executable.
2. `QEMU_SYSTEM_RISCV64` — the same thing under the generic name.
3. Well-known install locations: `C:\Program Files\qemu\`, `C:\Program Files (x86)\qemu\`,
   `C:\msys64\{mingw64,ucrt64}\bin\`, `C:\tools\**` / `D:\tools\**`, `/opt/homebrew/bin/`,
   `/usr/local/bin/`, `/usr/bin/`.
4. `PATH`.

An environment variable that is set but does not point at an existing file is reported as an
error — RiscDom never silently falls through to a weaker source.

In the desktop app: **Settings → Toolchain → QEMU** shows the resolved path with a source badge
(`EnvVar` / `KnownPath` / `Path` / `Manual`), offers **probe again** and **set path manually**, and
expands to the full search record. A manual path is stored in `settings.json` (app data
directory) and survives restarts.

```text
set RISCDOM_QEMU=C:\Program Files\qemu\qemu-system-riscv64.exe
```

## 3. Verify

```text
qemu-system-riscv64 --version
```

The output should name QEMU and a version. In the app the QEMU row shows a green dot.
Running the guest needs the toolchain too — see [toolchain-setup.md](toolchain-setup.md).

## Troubleshooting

- **“QEMU (qemu-system-riscv64) not found.”** — nothing matched: install it (step 1), or set
  `RISCDOM_QEMU`, then restart RiscDom. The message lists every location that was searched.
- **“set, but that file does not exist”** — the environment variable points at a stale path; fix
  it or clear it so auto-discovery can run.
- **“not runnable”** — the file exists but does not start (wrong architecture, or a wrapper
  missing its DLLs). Reinstall QEMU.
- **The VM starts but nothing boots** — check the RISC-V toolchain as well; a missing compiler
  fails before QEMU is ever involved (`toolchain_missing`).
- **Rare QEMU start failures under heavy load** — ports are picked, released, and only then bound
  by QEMU (a known TOCTOU). RiscDom retries with fresh ports
  (`host.toolchain.download.*` and `sandbox.snapshot.resume.retry` audit events show it); a
  persistent failure means something else is wrong.
