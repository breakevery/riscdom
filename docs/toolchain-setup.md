[中文](toolchain-setup.zh-CN.md) | English

# RISC-V toolchain setup

RiscDom compiles the AI's C / assembly with a **RISC-V bare-metal GCC**. Without one, the
sandbox can boot nothing: the app will tell you so and point here.

You need a `riscv64-unknown-elf-gcc` (or the xPack equivalent `riscv-none-elf-gcc`).

## 0. One-click download (recommended)

Open **Settings → Toolchain** and press **one-click download**. RiscDom then:

1. downloads the official xPack RISC-V GCC for your platform (~200 MB) from the GitHub release;
2. verifies its **SHA-256** against the checksum published next to the archive (mandatory — there
   is no skip-verification option);
3. extracts it under the app data directory (`<app data>/toolchain/<version>/`) and adopts it as
   the active toolchain, remembering it across restarts.

While it runs you get a progress bar, a state line (`downloading` / `verifying` / `extracting`)
and a **cancel** button; cancelling removes the partial download and leaves nothing behind. If
something fails, the error is shown with a **retry** button. Nothing is downloaded until you press
the button.

Prefer to install it yourself (or need a mirror / a different version)? Steps 1–3 below still
apply — RiscDom auto-detects what you installed.

## 1. Install a toolchain manually

**Windows (recommended: xPack)**

1. Download the latest `xpack-riscv-none-elf-gcc-*-win32-x64.zip` from
   <https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases>.
2. Extract it to `C:\Program Files\` (or anywhere you like, for example `D:\tools\`).
3. The executable is `<extract-dir>\bin\riscv-none-elf-gcc.exe`. Either add that `bin`
   directory to `PATH`, or point RiscDom at the file (step 2 below).

Windows alternatives: the MSYS2 package `mingw-w64-x86_64-riscv64-unknown-elf-gcc` (installs to
`C:\msys64\mingw64\bin` or `C:\msys64\ucrt64\bin`), or the official SiFive / GNU toolchain
build for Windows.

**macOS**

```text
brew tap riscv-software-src/riscv
brew install riscv-tools
# or: brew install riscv64-elf-gcc
```

**Linux**

Use your distribution's package (`gcc-riscv64-unknown-elf` on Debian/Ubuntu,
`riscv64-unknown-elf-gcc` on Arch/Fedora; `apt install gcc-riscv64-unknown-elf`), or build from
<https://github.com/riscv-collab/riscv-gnu-toolchain>.

## 2. Let RiscDom find it

RiscDom resolves the toolchain in this order and stops at the first hit:

1. `RISCDOM_RISCV_GCC` — full path to the compiler (e.g.
   `C:\Program Files\xpack-riscv-none-elf-gcc-15.2.0-1\bin\riscv-none-elf-gcc.exe`).
2. `RISCV_GCC` — the same thing under the generic name.
3. Well-known install locations: `C:\Program Files\xpack-riscv-none-elf-gcc-*\bin\`,
   `C:\Program Files (x86)\...`, `C:\msys64\{mingw64,ucrt64}\bin\`, `C:\tools\**\bin\`,
   `D:\tools\**\bin\`, `~/.local/bin/`, `/opt/riscv/bin/`, `/usr/local/bin/`, `/usr/bin/`.
4. `PATH` — `riscv64-unknown-elf-gcc`, then `riscv-none-elf-gcc`.

If an environment variable is set but does not point at an existing file, RiscDom **reports
that** instead of silently falling through — you asked for that compiler explicitly.

In the desktop app you can also set it by hand: **Settings → Toolchain → “手动指定 / Set
path”**, paste the full path to the compiler, and RiscDom checks it with `--version` before
accepting it. “Clear” goes back to auto-discovery. **Settings → Toolchain → “探测详情 /
diagnostics”** shows every location that was tried.

## 3. Verify

```text
riscv64-unknown-elf-gcc --version
# or, for xPack builds:
riscv-none-elf-gcc --version
```

The output should name the target `riscv64-unknown-elf` (or `riscv64-none-elf`) and a version.
In the app, the toolchain row should show a green dot with a source badge
(`EnvVar` / `KnownPath` / `Path` / `Manual`).

## Troubleshooting

- **“RISC-V GCC not found.”** — nothing matched: install a toolchain (step 1) or set
  `RISCDOM_RISCV_GCC`, then restart RiscDom. The message lists every path that was searched.
- **“set, but that file does not exist”** — the environment variable points at a stale path;
  fix it or clear it so auto-discovery can run.
- **“`--version` exited with …”** — the file exists but is not runnable (wrong architecture, or
  a wrapper without its support files). Install a fresh toolchain.
- **Compiles fail with relocation/`medlow` errors** — RiscDom already passes
  `-mcmodel=medany`; check that you are not overriding flags through an unusual wrapper script.

See also: [ENVIRONMENT.md](ENVIRONMENT.md) for the versions verified on this project's
development machine.

## Environment preflight

Once a compiler is configured, RiscDom checks the whole environment by **using** it: it
compiles a tiny guest and boots it with your QEMU, then reports which step failed. What it
covers, when it runs, and the "continue anyway" escape hatch: [preflight.md](preflight.md).
