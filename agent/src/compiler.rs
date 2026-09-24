//! Freestanding RISC-V compiler wrapper.
//!
//! The AI writes C; this turns it into a bootable bare-metal ELF.
//!
//! **Deviation from the spec command (reported):** the spec suggested
//! `-Ttext=0x80000000` alone. In this environment that does not boot, because
//! (a) the `medlow` code model truncates relocations at `0x80000000` and
//! (b) `-bios none` makes QEMU jump to `0x80000000` regardless of the ELF
//! entry point. We therefore add `-mcmodel=medany` and link with a generated
//! script that places an injected `crt0` (`_start`) first and sets `sp`.
//! The AI only needs to write `int main(void) { ... }`.
//!
//! The cross compiler is resolved by [`CompilerConfig::discover`] (stage 24a):
//! `RISCDOM_RISCV_GCC` → `RISCV_GCC` → well-known install locations → `PATH`.
//!
//! **Zig (v0.9 F3a).** A second language shares this wrapper: a `.zig` source is
//! compiled by `zig build-exe -target riscv64-freestanding` instead of GCC
//! ([`ZigConfig`]). Zig brings its own cross linker, so the freestanding target needs
//! no external toolchain and no sysroot. The generated `link.ld` is reused verbatim —
//! it is language-agnostic (`OUTPUT_ARCH`, `ENTRY(_start)`, `_stack_top`). Nothing is
//! injected for Zig: the model writes its own `_start`, because the `-bios none` guest
//! jumps to the load address rather than to the ELF entry point.

use crate::error::AgentError;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the toolchain path came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainSource {
    /// `RISCDOM_RISCV_GCC` / `RISCV_GCC` environment variable.
    EnvVar,
    /// A well-known install location.
    KnownPath,
    /// Found on `PATH`.
    Path,
    /// Configured explicitly by the user (host `set_toolchain_path`).
    Manual,
}

impl ToolchainSource {
    /// Stable identifier (also used by the host / UI).
    pub fn as_str(self) -> &'static str {
        match self {
            ToolchainSource::EnvVar => "EnvVar",
            ToolchainSource::KnownPath => "KnownPath",
            ToolchainSource::Path => "Path",
            ToolchainSource::Manual => "Manual",
        }
    }
}

impl std::fmt::Display for ToolchainSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Toolchain discovery failures (always actionable, never a bare "not found").
#[derive(Debug, thiserror::Error)]
pub enum ToolchainError {
    #[error("{0}")]
    NotFound(String),
    #[error("RISC-V GCC not found at {0}: the path does not exist")]
    NotAFile(String),
    #[error("RISC-V GCC at {0} is not runnable:\n{1}")]
    NotRunnable(String, String),
}

impl From<ToolchainError> for AgentError {
    fn from(e: ToolchainError) -> Self {
        AgentError::Tool(e.to_string())
    }
}

/// Compiler configuration.
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// RISC-V cross compiler (`RISCDOM_RISCV_GCC` or a discovered path).
    pub gcc: PathBuf,
    /// How [`Self::gcc`] was resolved.
    pub source: ToolchainSource,
    /// `-march` value.
    pub march: String,
    /// `-mabi` value.
    pub mabi: String,
    /// Load address the image is linked at.
    pub link_addr: String,
    /// Zig (v0.9 F3a): what a `.zig` source is compiled by, while [`Self::gcc`]
    /// compiles a `.c` source. The host replaces the discovered value when
    /// `settings.zig_path` is set, and both paths coexist — one sandbox can build
    /// either language.
    pub zig: ZigConfig,
    /// Rust (v0.9 F3b-1): what a `.rs` source is compiled by, and the `--sysroot` it needs.
    /// The host fills the sysroot from `settings.rust_sysroot`; unlike Zig and GCC, either
    /// half may be absent, and the compile says which half is missing.
    pub rust: RustConfig,
}

/// Executable names we accept, in preference order (xPack ships the `riscv-none-elf-` prefix).
///
/// Exported so that a consumer which has to recognise one of these executables (the
/// host, when it installs a downloaded archive) matches the same list instead of
/// keeping its own copy (v0.4 1e-followup).
pub const GCC_NAMES: [&str; 2] = ["riscv64-unknown-elf-gcc", "riscv-none-elf-gcc"];

/// Environment variables consulted, in priority order.
const GCC_ENV_VARS: [&str; 2] = ["RISCDOM_RISCV_GCC", "RISCV_GCC"];

/// Where to tell users to get a toolchain.
pub const TOOLCHAIN_URL: &str =
    "https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases";

/// Executable names we accept for Zig, in preference order (v0.9 F3a).
///
/// A single name: a Zig release ships one `zig` executable per platform
/// (`zig.exe` on Windows), and there is no vendor prefix to allow for.
pub const ZIG_NAMES: [&str; 1] = ["zig"];

/// Environment variable consulted for the Zig executable, in priority order.
///
/// `RISCDOM_ZIG` only: a bare `ZIG` is a common enough name that reading it would
/// adopt an unrelated value on a machine that happens to export one.
const ZIG_ENV_VARS: [&str; 1] = ["RISCDOM_ZIG"];

/// Where to tell users to get Zig.
pub const ZIG_URL: &str = "https://ziglang.org/download/";

/// The Zig target triple a `.zig` source is built for (v0.9 F3a).
const ZIG_TARGET: &str = "riscv64-freestanding";

/// The Zig optimisation mode a `.zig` source is built with (v0.9 F3a).
const ZIG_OPTIMIZE: &str = "ReleaseSmall";

/// Executable names we accept for `rustc`, in preference order (v0.9 F3b-1).
///
/// A single name: a rustup install puts one shim on `PATH`, and a hand install uses the
/// same name everywhere Rust runs.
pub const RUSTC_NAMES: [&str; 1] = ["rustc"];

/// Environment variable consulted for the `rustc` executable.
const RUSTC_ENV_VARS: [&str; 1] = ["RISCDOM_RUSTC"];

/// Environment variable that names a Rust sysroot, for a host that has not pinned one.
const RUST_SYSROOT_ENV: [&str; 1] = ["RISCDOM_RUST_SYSROOT"];

/// The Rust target triple a `.rs` source is built for (v0.9 F3b-1).
///
/// Public because the host's `rust-std` spec names its asset after it
/// (`rust-std-<version>-<target>.tar.xz`), exactly as the GCC and Zig names are shared rather
/// than copied.
pub const RUST_TARGET: &str = "riscv64gc-unknown-none-elf";

/// The Rust edition a `.rs` source is built with.
const RUST_EDITION: &str = "2021";

/// Where to tell users to get Rust.
pub const RUST_URL: &str = "https://rustup.rs/";

impl CompilerConfig {
    /// Build from the environment, discovering the toolchain when possible.
    ///
    /// Never fails: when nothing is found the config carries a bare executable
    /// name so the eventual compile reports the full diagnostics.
    pub fn from_env() -> Self {
        let (found, _log) = search();
        let (gcc, source) =
            found.unwrap_or((PathBuf::from(exe_name(GCC_NAMES[0])), ToolchainSource::Path));
        Self {
            gcc,
            source,
            march: "rv64gc".into(),
            mabi: "lp64d".into(),
            link_addr: "0x80000000".into(),
            zig: ZigConfig::from_env(),
            rust: RustConfig::from_env(),
        }
    }

    /// Locate the RISC-V cross compiler.
    ///
    /// Priority: `RISCDOM_RISCV_GCC` → `RISCV_GCC` → well-known install
    /// locations → `PATH`. An environment variable that is set but does not
    /// point at an existing file is an explicit error (never silently ignored).
    pub fn discover() -> Result<PathBuf, ToolchainError> {
        let (found, log) = search();
        match found {
            Some((path, _)) => Ok(path),
            None => Err(ToolchainError::NotFound(not_found_message(&log))),
        }
    }

    /// Build explicitly from a user-chosen path (`Manual` source).
    ///
    /// Used by the host when the user points the app at a toolchain by hand.
    pub fn manual(gcc: PathBuf) -> Self {
        Self {
            gcc,
            source: ToolchainSource::Manual,
            march: "rv64gc".into(),
            mabi: "lp64d".into(),
            link_addr: "0x80000000".into(),
            zig: ZigConfig::from_env(),
            rust: RustConfig::from_env(),
        }
    }

    /// Human-readable record of the search: where we looked and what happened.
    pub fn diagnostics() -> String {
        let (found, log) = search();
        let mut out = String::from("RISC-V GCC search:\n");
        for line in &log {
            out.push_str("  - ");
            out.push_str(line);
            out.push('\n');
        }
        match found {
            Some((path, source)) => out.push_str(&format!(
                "  => found: {} (source: {source})\n",
                path.display()
            )),
            None => out.push_str("  => not found\n"),
        }
        out
    }
}

/// Zig compiler configuration (v0.9 F3a: the second language).
///
/// A separate type rather than a second `CompilerConfig`: the two languages share only
/// the discovery *shape* (`env var → known locations → PATH`), and Zig's knobs
/// (`-target` / `-O`) are not GCC's (`-march` / `-mabi`). It rides inside
/// [`CompilerConfig`] so that `compile_freestanding` keeps its signature while a host
/// can still pin both executables (one sandbox, either language).
#[derive(Debug, Clone)]
pub struct ZigConfig {
    /// The `zig` executable (`RISCDOM_ZIG` or a discovered path).
    pub zig: PathBuf,
    /// How [`Self::zig`] was resolved.
    pub source: ToolchainSource,
    /// `-target` value ([`ZIG_TARGET`]).
    pub target: String,
    /// `-O` value ([`ZIG_OPTIMIZE`]).
    pub optimize: String,
}

impl ZigConfig {
    /// Build from the environment, discovering Zig when possible.
    ///
    /// Never fails: when nothing is found the config carries a bare executable name so
    /// the eventual compile reports the full diagnostics — the same forgiving shape as
    /// [`CompilerConfig::from_env`].
    pub fn from_env() -> Self {
        let (found, _log) = search_zig();
        let (zig, source) =
            found.unwrap_or((PathBuf::from(exe_name(ZIG_NAMES[0])), ToolchainSource::Path));
        Self {
            zig,
            source,
            target: ZIG_TARGET.into(),
            optimize: ZIG_OPTIMIZE.into(),
        }
    }

    /// Locate the Zig executable.
    ///
    /// Priority: `RISCDOM_ZIG` → well-known install locations → `PATH`. An environment
    /// variable that is set but does not point at an existing file is an explicit error
    /// (never silently ignored), exactly like the GCC search.
    pub fn discover() -> Result<PathBuf, ToolchainError> {
        let (found, log) = search_zig();
        match found {
            Some((path, _)) => Ok(path),
            None => Err(ToolchainError::NotFound(zig_not_found_message(&log))),
        }
    }

    /// Build explicitly from a user-chosen path (`Manual` source).
    ///
    /// Used by the host when the user points the app at Zig by hand
    /// (`settings.zig_path`).
    pub fn manual(zig: PathBuf) -> Self {
        Self {
            zig,
            source: ToolchainSource::Manual,
            target: ZIG_TARGET.into(),
            optimize: ZIG_OPTIMIZE.into(),
        }
    }

    /// Human-readable record of the Zig search.
    pub fn diagnostics() -> String {
        let (found, log) = search_zig();
        let mut out = String::from("Zig search:\n");
        for line in &log {
            out.push_str("  - ");
            out.push_str(line);
            out.push('\n');
        }
        match found {
            Some((path, source)) => out.push_str(&format!(
                "  => found: {} (source: {source})\n",
                path.display()
            )),
            None => out.push_str("  => not found\n"),
        }
        out
    }
}

/// Well-known Zig install locations (Windows first, Unix covered too).
fn zig_known_candidates() -> Vec<Candidate> {
    let mut out = Vec::new();

    if cfg!(windows) {
        for p in [
            r"C:\Program Files\zig\zig.exe",
            r"C:\Program Files (x86)\zig\zig.exe",
            r"C:\zig\zig.exe",
            r"C:\ProgramData\chocolatey\bin\zig.exe",
        ] {
            out.push(Candidate {
                label: p.to_string(),
                path: Some(PathBuf::from(p)),
            });
        }
        if let Ok(home) = std::env::var("USERPROFILE") {
            out.push(Candidate {
                label: r"%USERPROFILE%\scoop\apps\zig\current\zig.exe".into(),
                path: Some(
                    PathBuf::from(home)
                        .join("scoop")
                        .join("apps")
                        .join("zig")
                        .join("current")
                        .join("zig.exe"),
                ),
            });
        }
        // Developer machines keep toolchains under <drive>:\tools\<name>\bin.
        for base in [r"C:\tools", r"D:\tools"] {
            let label = format!(r"{base}\**\bin\{}", exe_name(ZIG_NAMES[0]));
            let hit = scan_for_bin_named(Path::new(base), 3, &ZIG_NAMES);
            out.push(Candidate { label, path: hit });
        }
    } else {
        if let Ok(home) = std::env::var("HOME") {
            out.push(Candidate {
                label: "~/.local/bin/zig".into(),
                path: Some(PathBuf::from(home).join(".local").join("bin").join("zig")),
            });
        }
        for p in ["/usr/local/bin/zig", "/usr/bin/zig", "/opt/zig/zig"] {
            out.push(Candidate {
                label: p.to_string(),
                path: Some(PathBuf::from(p)),
            });
        }
    }

    out
}

/// The Zig search, returning the hit (if any) plus a human-readable log.
fn search_zig() -> (Option<(PathBuf, ToolchainSource)>, Vec<String>) {
    let mut log = Vec::new();

    for var in ZIG_ENV_VARS {
        match std::env::var(var) {
            Ok(v) if !v.trim().is_empty() => {
                let path = PathBuf::from(v.trim());
                if path.is_file() {
                    log.push(format!("{var} = {} (found)", path.display()));
                    return (Some((path, ToolchainSource::EnvVar)), log);
                }
                log.push(format!(
                    "{var} = {} (set, but that file does not exist)",
                    path.display()
                ));
                return (None, log);
            }
            _ => log.push(format!("{var} (not set)")),
        }
    }

    for candidate in zig_known_candidates() {
        match candidate.path {
            Some(path) if path.is_file() => {
                log.push(format!("{} (found)", candidate.label));
                return (Some((path, ToolchainSource::KnownPath)), log);
            }
            _ => log.push(format!("{} (not found)", candidate.label)),
        }
    }

    for name in ZIG_NAMES {
        match find_on_path(name) {
            Some(path) => {
                log.push(format!("PATH {name} -> {} (found)", path.display()));
                return (Some((path, ToolchainSource::Path)), log);
            }
            None => log.push(format!("PATH {name} (not found)")),
        }
    }

    (None, log)
}

/// Actionable multi-line message shown when no Zig executable was found.
fn zig_not_found_message(log: &[String]) -> String {
    let mut msg = String::from("Zig not found.\nSearched:\n");
    for line in log {
        msg.push_str("  - ");
        msg.push_str(line);
        msg.push('\n');
    }
    msg.push_str(&format!(
        "Install Zig from {ZIG_URL}\n\
         or set RISCDOM_ZIG to the full path of the zig executable and restart RiscDom."
    ));
    msg
}

/// Rust compiler configuration (v0.9 F3b-1: the third language).
///
/// Mirrors [`ZigConfig`], with one deliberate difference: **either half can be absent**.
/// A Rust bare-metal build needs a compiler *and* a sysroot carrying the target's `core`,
/// and this project ships neither (Rust comes from the machine, and the `rust-std`
/// download is F3b-2). So the honest shape is "what we found" plus an explicit refusal at
/// compile time ([`RustConfig::require`]) that names the missing half — never a silent
/// failure and never a fabricated path.
#[derive(Debug, Clone)]
pub struct RustConfig {
    /// The `rustc` executable (`RISCDOM_RUSTC` or a discovered path).
    pub rustc: Option<PathBuf>,
    /// How [`Self::rustc`] was resolved (when it was).
    pub source: ToolchainSource,
    /// The `--sysroot` directory: a `rust-std-<target>/` tree carrying `core`.
    pub sysroot: Option<PathBuf>,
    /// `--target` value ([`RUST_TARGET`]).
    pub target: String,
    /// `--edition` value ([`RUST_EDITION`]).
    pub edition: String,
}

impl RustConfig {
    /// Build from the environment, discovering `rustc` when possible.
    ///
    /// Never fails, and never invents a value: a missing half stays `None` so the eventual
    /// compile can say which one it is.
    pub fn from_env() -> Self {
        let (found, _log) = search_rustc();
        let (rustc, source) = match found {
            Some((path, source)) => (Some(path), source),
            None => (None, ToolchainSource::Path),
        };
        let sysroot = std::env::var(RUST_SYSROOT_ENV[0])
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Self {
            rustc,
            source,
            sysroot,
            target: RUST_TARGET.into(),
            edition: RUST_EDITION.into(),
        }
    }

    /// Locate `rustc`.
    ///
    /// Priority: `RISCDOM_RUSTC` → `PATH`. Rust is expected to be installed by the user
    /// (the same shape as QEMU's "install it yourself" decision), so there is no list of
    /// well-known directories to guess at.
    pub fn discover() -> Result<PathBuf, ToolchainError> {
        let (found, log) = search_rustc();
        match found {
            Some((path, _)) => Ok(path),
            None => Err(ToolchainError::NotFound(rustc_not_found_message(&log))),
        }
    }

    /// Human-readable record of the `rustc` search.
    pub fn diagnostics() -> String {
        let (found, log) = search_rustc();
        let mut out = String::from("rustc search:\n");
        for line in &log {
            out.push_str("  - ");
            out.push_str(line);
            out.push('\n');
        }
        match found {
            Some((path, source)) => out.push_str(&format!(
                "  => found: {} (source: {source})\n",
                path.display()
            )),
            None => out.push_str("  => not found\n"),
        }
        out
    }

    /// The `(rustc, sysroot)` pair a compile needs, or the actionable error that says
    /// which half is missing and where it comes from.
    pub fn require(&self) -> Result<(&Path, &Path), AgentError> {
        let Some(rustc) = self.rustc.as_deref() else {
            let (_found, log) = search_rustc();
            return Err(AgentError::Tool(rustc_not_found_message(&log)));
        };
        let Some(sysroot) = self.sysroot.as_deref() else {
            return Err(AgentError::Tool(format!(
                "no Rust sysroot is configured, so `core` for {target} cannot be found.\n\
                 Set `rust_sysroot` in settings.json (or {env}) to the `rust-std-{target}/` \
                 directory that carries the target's libraries \
                 (`rustup target add {target}` installs one under rustc's own sysroot).",
                target = self.target,
                env = RUST_SYSROOT_ENV[0],
            )));
        };
        Ok((rustc, sysroot))
    }
}

/// Well-known `rustc` locations are deliberately **not** searched (see
/// [`RustConfig::discover`]): Rust is a user install here, like QEMU.
fn search_rustc() -> (Option<(PathBuf, ToolchainSource)>, Vec<String>) {
    let mut log = Vec::new();

    for var in RUSTC_ENV_VARS {
        match std::env::var(var) {
            Ok(v) if !v.trim().is_empty() => {
                let path = PathBuf::from(v.trim());
                if path.is_file() {
                    log.push(format!("{var} = {} (found)", path.display()));
                    return (Some((path, ToolchainSource::EnvVar)), log);
                }
                log.push(format!(
                    "{var} = {} (set, but that file does not exist)",
                    path.display()
                ));
                return (None, log);
            }
            _ => log.push(format!("{var} (not set)")),
        }
    }

    for name in RUSTC_NAMES {
        match find_on_path(name) {
            Some(path) => {
                log.push(format!("PATH {name} -> {} (found)", path.display()));
                return (Some((path, ToolchainSource::Path)), log);
            }
            None => log.push(format!("PATH {name} (not found)")),
        }
    }

    (None, log)
}

/// Actionable message shown when no `rustc` was found.
fn rustc_not_found_message(log: &[String]) -> String {
    let mut msg = String::from("rustc not found.\nSearched:\n");
    for line in log {
        msg.push_str("  - ");
        msg.push_str(line);
        msg.push('\n');
    }
    msg.push_str(&format!(
        "Install Rust from {RUST_URL}\n\
         or set RISCDOM_RUSTC to the full path of the rustc executable and restart RiscDom."
    ));
    msg
}

/// Result of a compile attempt.
#[derive(Debug, Clone)]
pub struct CompileOutput {
    pub stdout: String,
    pub stderr: String,
    pub ok: bool,
}

/// Minimal startup code injected into every build.
///
/// The injection is unconditional: every ELF built through [`compile_freestanding`]
/// carries it, so a run's configuration fingerprint records the fact rather than
/// a copy of a value that could drift (v0.4 1e).
pub const CRT0_INJECTED: &str = "injected";

const CRT0: &str = r#".section .text.start
.global _start
.type _start, @function
_start:
    la sp, _stack_top
    call main
1:
    j 1b
.size _start, . - _start
"#;

/// Linker script template (`LINK_ADDR` / `STACK_ADDR` are substituted).
const LINK_LD: &str = r#"OUTPUT_ARCH(riscv)
ENTRY(_start)
SECTIONS
{
  . = LINK_ADDR;
  .text   : { *(.text.start) *(.text*) }
  .rodata : { *(.rodata*) }
  .data   : { *(.data*) }
  .bss    : { *(.bss*) *(COMMON) }
  _stack_top = STACK_ADDR;
}
"#;

/// `name` → `name.exe` on Windows.
fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// A candidate path plus the label used in the diagnostics.
struct Candidate {
    label: String,
    path: Option<PathBuf>,
}

/// Well-known install locations (Windows first, Unix covered too).
fn known_candidates() -> Vec<Candidate> {
    let mut out = Vec::new();

    if cfg!(windows) {
        for base in [r"C:\Program Files", r"C:\Program Files (x86)"] {
            let pattern = format!(r"{base}\xpack-riscv-none-elf-gcc-*\bin\riscv-none-elf-gcc.exe");
            let hit = list_dirs(base)
                .into_iter()
                .filter(|d| {
                    d.file_name()
                        .map(|n| {
                            n.to_string_lossy()
                                .to_lowercase()
                                .starts_with("xpack-riscv-none-elf-gcc-")
                        })
                        .unwrap_or(false)
                })
                .map(|d| d.join("bin").join("riscv-none-elf-gcc.exe"))
                .find(|p| p.is_file());
            out.push(Candidate {
                label: pattern,
                path: hit,
            });
        }
        for p in [
            r"C:\msys64\mingw64\bin\riscv64-unknown-elf-gcc.exe",
            r"C:\msys64\ucrt64\bin\riscv64-unknown-elf-gcc.exe",
        ] {
            out.push(Candidate {
                label: p.to_string(),
                path: Some(PathBuf::from(p)),
            });
        }
        // Developer machines keep toolchains under <drive>:\tools\<name>\bin.
        for base in [r"C:\tools", r"D:\tools"] {
            let label = format!(r"{base}\**\bin\{}", exe_name(GCC_NAMES[0]));
            let hit = scan_for_bin_gcc(Path::new(base), 3);
            out.push(Candidate { label, path: hit });
        }
    } else {
        if let Ok(home) = std::env::var("HOME") {
            out.push(Candidate {
                label: "~/.local/bin/riscv64-unknown-elf-gcc".into(),
                path: Some(
                    PathBuf::from(home)
                        .join(".local")
                        .join("bin")
                        .join("riscv64-unknown-elf-gcc"),
                ),
            });
        }
        for p in [
            "/opt/riscv/bin/riscv64-unknown-elf-gcc",
            "/usr/local/bin/riscv64-unknown-elf-gcc",
            "/usr/bin/riscv64-unknown-elf-gcc",
        ] {
            out.push(Candidate {
                label: p.to_string(),
                path: Some(PathBuf::from(p)),
            });
        }
    }

    out
}

/// Directory entries of `base` (empty when unreadable).
fn list_dirs(base: &str) -> Vec<PathBuf> {
    std::fs::read_dir(base)
        .map(|rd| {
            let mut v: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            v.sort();
            v
        })
        .unwrap_or_default()
}

/// Bounded recursive scan for `<dir>/bin/<gcc>` under `root`.
fn scan_for_bin_gcc(root: &Path, max_depth: usize) -> Option<PathBuf> {
    scan_for_bin_named(root, max_depth, &GCC_NAMES)
}

/// Bounded recursive scan for `<dir>/bin/<name>` under `root`, for any of `names`.
///
/// Shared by the GCC and the Zig search (v0.9 F3a): a developer toolchain directory is
/// walked the same way either time, and only the executable names differ.
fn scan_for_bin_named(root: &Path, max_depth: usize, names: &[&str]) -> Option<PathBuf> {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        for entry in list_dirs(&dir.to_string_lossy()) {
            for name in names {
                let candidate = entry.join("bin").join(exe_name(name));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            stack.push((entry, depth + 1));
        }
    }
    None
}

/// Look for `name` (and `name.exe` on Windows) on `PATH`.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in std::env::split_paths(&path) {
        let _ = sep;
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        if cfg!(windows) {
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }
    None
}

/// The full search, returning the hit (if any) plus a human-readable log.
fn search() -> (Option<(PathBuf, ToolchainSource)>, Vec<String>) {
    let mut log = Vec::new();

    for var in GCC_ENV_VARS {
        match std::env::var(var) {
            Ok(v) if !v.trim().is_empty() => {
                let path = PathBuf::from(v.trim());
                if path.is_file() {
                    log.push(format!("{var} = {} (found)", path.display()));
                    return (Some((path, ToolchainSource::EnvVar)), log);
                }
                log.push(format!(
                    "{var} = {} (set, but that file does not exist)",
                    path.display()
                ));
                return (None, log);
            }
            _ => log.push(format!("{var} (not set)")),
        }
    }

    for candidate in known_candidates() {
        match candidate.path {
            Some(path) if path.is_file() => {
                log.push(format!("{} (found)", candidate.label));
                return (Some((path, ToolchainSource::KnownPath)), log);
            }
            _ => log.push(format!("{} (not found)", candidate.label)),
        }
    }

    for name in GCC_NAMES {
        match find_on_path(name) {
            Some(path) => {
                log.push(format!("PATH {name} -> {} (found)", path.display()));
                return (Some((path, ToolchainSource::Path)), log);
            }
            None => log.push(format!("PATH {name} (not found)")),
        }
    }

    (None, log)
}

/// Actionable multi-line message shown when nothing was found.
fn not_found_message(log: &[String]) -> String {
    let mut msg = String::from("RISC-V GCC not found.\nSearched:\n");
    for line in log {
        msg.push_str("  - ");
        msg.push_str(line);
        msg.push('\n');
    }
    msg.push_str(&format!(
        "Install the xPack RISC-V GCC from {TOOLCHAIN_URL}\n\
         or set RISCDOM_RISCV_GCC to the full path of riscv64-unknown-elf-gcc.exe and restart RiscDom."
    ));
    msg
}

/// Is `src` a Zig source? The language follows the extension (v0.9 F3a).
fn is_zig_source(src: &Path) -> bool {
    has_extension(src, "zig")
}

/// Is `src` a Rust source? (v0.9 F3b-1)
fn is_rust_source(src: &Path) -> bool {
    has_extension(src, "rs")
}

fn has_extension(src: &Path, wanted: &str) -> bool {
    src.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(wanted))
        .unwrap_or(false)
}

/// Compile `src` into a freestanding ELF at `out`.
///
/// The language follows the source extension (v0.9 F3a): `.c` / `.h` / `.S` / `.s` go
/// through GCC exactly as before, `.zig` through [`run_zig`]. The C path is untouched.
pub fn compile_freestanding(
    cfg: &CompilerConfig,
    src: &Path,
    out: &Path,
) -> Result<CompileOutput, AgentError> {
    if is_zig_source(src) {
        return compile_zig(cfg, src, out);
    }
    if is_rust_source(src) {
        return compile_rust(cfg, src, out);
    }

    let (crt0, link_ld) = write_build_files(&cfg.link_addr)?;
    let build_dir = crt0.parent().map(Path::to_path_buf);

    let compiled = run_gcc(cfg, &crt0, &link_ld, src, out);

    // The injected files are scratch: gcc has read them by the time it returns, so
    // the per-build directory goes away with the build — on success and on failure
    // alike (v0.4 batch 3-followup-2: the unique path fixed the race but leaked a
    // directory per compile). The ELF itself lives at `out`, which the caller owns.
    if let Some(dir) = build_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
    compiled
}

/// Compile a `.zig` source into a freestanding ELF at `out` (v0.9 F3a).
///
/// Only the generated `link.ld` is injected — nothing else. The model's Zig source
/// provides `_start` itself, because the `-bios none` guest jumps to the load address
/// rather than to the ELF entry point, so the startup code has to be the first thing
/// there; `link.ld` places `.text.start` first for exactly that reason.
fn compile_zig(cfg: &CompilerConfig, src: &Path, out: &Path) -> Result<CompileOutput, AgentError> {
    let dir = build_scratch_dir()?;
    let link_ld = match write_link_ld(&dir, &cfg.link_addr) {
        Ok(path) => path,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let compiled = run_zig(&cfg.zig, &link_ld, &cfg.link_addr, src, out);
    let _ = std::fs::remove_dir_all(&dir);
    compiled
}

/// Invoke `zig build-exe` once (v0.9 F3a).
///
/// Zig brings its own cross linker, so the freestanding target needs no external
/// toolchain and no sysroot; `--script` is the generated `link.ld`, which keeps the load
/// address (`--image-base`) and the section order in one place shared with the C path.
fn run_zig(
    cfg: &ZigConfig,
    link_ld: &Path,
    link_addr: &str,
    src: &Path,
    out: &Path,
) -> Result<CompileOutput, AgentError> {
    let output = Command::new(&cfg.zig)
        .arg("build-exe")
        .arg(src)
        .arg(format!("-target={}", cfg.target))
        .arg("-O")
        .arg(&cfg.optimize)
        .arg("-fno-stack-check")
        .arg("-T")
        .arg(link_ld)
        .arg("--image-base")
        .arg(link_addr)
        .arg(format!("-femit-bin={}", out.display()))
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                // Turn "program not found" into an actionable report.
                let (_found, log) = search_zig();
                AgentError::Tool(zig_not_found_message(&log))
            } else {
                AgentError::Tool(format!("failed to run {}: {e}", cfg.zig.display()))
            }
        })?;

    Ok(CompileOutput {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Compile a `.rs` source into a freestanding ELF at `out` (v0.9 F3b-1).
///
/// Like the Zig arm, only the generated `link.ld` is injected: the source owns `_start`
/// (the `-bios none` guest jumps to the load address, so the startup code has to be the
/// first thing there) and a `#[panic_handler]` of its own.
fn compile_rust(cfg: &CompilerConfig, src: &Path, out: &Path) -> Result<CompileOutput, AgentError> {
    let dir = build_scratch_dir()?;
    let link_ld = match write_link_ld(&dir, &cfg.link_addr) {
        Ok(path) => path,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let compiled = run_rust(&cfg.rust, &cfg.gcc, &link_ld, src, out);
    let _ = std::fs::remove_dir_all(&dir);
    compiled
}

/// The `rustc` command line for a bare-metal ELF.
///
/// A **pure function on purpose**: it is the one part of the Rust path that a machine
/// without the target's `core` can still check, which is what the unit test does (v0.9
/// F3b-1).
///
/// The linker is the **configured** RISC-V GCC rather than the bare name
/// `riscv64-unknown-elf-gcc`: an xPack install is `riscv-none-elf-gcc`, so the bare name
/// would work only for one of the two toolchains this project accepts (a deliberate
/// deviation from the reconnaissance's draft command, which hard-coded the name).
fn rustc_args(
    cfg: &RustConfig,
    sysroot: &Path,
    linker: &Path,
    link_ld: &Path,
    src: &Path,
    out: &Path,
) -> Vec<String> {
    vec![
        src.display().to_string(),
        format!("--target={}", cfg.target),
        format!("--sysroot={}", sysroot.display()),
        format!("--edition={}", cfg.edition),
        format!("-Clinker={}", linker.display()),
        // The linker script carries the load address and the section order, exactly as it
        // does for C and Zig. `-T` and the path go as two arguments, the way GCC gets them.
        "-Clink-arg=-T".to_string(),
        format!("-Clink-arg={}", link_ld.display()),
        "-Clink-arg=-nostartfiles".to_string(),
        "-Clink-arg=-march=rv64gc".to_string(),
        "-Clink-arg=-mabi=lp64d".to_string(),
        "-Cpanic=abort".to_string(),
        "-Crelocation-model=static".to_string(),
        "-Ccode-model=medany".to_string(),
        "-o".to_string(),
        out.display().to_string(),
    ]
}

/// Invoke `rustc` once (v0.9 F3b-1).
///
/// `RUSTUP_TOOLCHAIN` is cleared **for this child only**: `--sysroot` decides where `core`
/// comes from, and a `rust-toolchain.toml` in the workspace must not switch the compiler
/// out from under the sysroot the host handed us. `Command::env_remove` is per-process-
/// child, so parallel builds in one test run are unaffected.
fn run_rust(
    cfg: &RustConfig,
    gcc: &Path,
    link_ld: &Path,
    src: &Path,
    out: &Path,
) -> Result<CompileOutput, AgentError> {
    let (rustc, sysroot) = cfg.require()?;
    let output = Command::new(rustc)
        .args(rustc_args(cfg, sysroot, gcc, link_ld, src, out))
        .env_remove("RUSTUP_TOOLCHAIN")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                let (_found, log) = search_rustc();
                AgentError::Tool(rustc_not_found_message(&log))
            } else {
                AgentError::Tool(format!("failed to run {}: {e}", rustc.display()))
            }
        })?;

    Ok(CompileOutput {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Build scratch prefix: `riscdom-build-` (see [`crate::tempdirs`] for the rules).
///
/// The sweep threshold lives in [`crate::tempdirs::TEMP_DIR_MAX_AGE`], which covers
/// this prefix along with every other `riscdom-*` directory.
///
/// Invoke the compiler once.
fn run_gcc(
    cfg: &CompilerConfig,
    crt0: &Path,
    link_ld: &Path,
    src: &Path,
    out: &Path,
) -> Result<CompileOutput, AgentError> {
    let output = Command::new(&cfg.gcc)
        .arg(format!("-march={}", cfg.march))
        .arg(format!("-mabi={}", cfg.mabi))
        .arg("-mcmodel=medany")
        .arg("-ffreestanding")
        .arg("-nostdlib")
        .arg("-nostartfiles")
        .arg("-T")
        .arg(link_ld)
        .arg("-o")
        .arg(out)
        .arg(crt0)
        .arg(src)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                // Turn "program not found" into an actionable report.
                let (_found, log) = search();
                AgentError::Tool(not_found_message(&log))
            } else {
                AgentError::Tool(format!("failed to run {}: {e}", cfg.gcc.display()))
            }
        })?;

    Ok(CompileOutput {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Write the injected `crt0.S` and linker script into a **per-build** directory.
///
/// Every call gets its own directory: builds are no longer single-threaded (a run
/// may compile while the environment preflight compiles, and tests run in
/// parallel), and sharing one path meant the loser of that race compiled against a
/// half-written file (v0.4 batch 3-followup). The directory is scratch —
/// `compile_freestanding` removes it when the build finishes.
fn write_build_files(link_addr: &str) -> Result<(PathBuf, PathBuf), AgentError> {
    let dir = build_scratch_dir()?;
    let crt0 = dir.join("crt0.S");
    let link_ld = dir.join("link.ld");

    let written = (|| -> Result<(), AgentError> {
        std::fs::write(&crt0, CRT0)?;
        std::fs::write(&link_ld, link_ld_contents(link_addr)?)?;
        Ok(())
    })();
    if let Err(e) = written {
        // Never leave a half-written build directory behind.
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok((crt0, link_ld))
}

/// A fresh per-build scratch directory.
///
/// Every compile gets its own directory: builds are no longer single-threaded (a run may
/// compile while the environment preflight compiles, and tests run in parallel), and
/// sharing one path left the loser of that race compiling against a half-written file
/// (v0.4 batch 3-followup). The directory is scratch — the caller removes it when the
/// build finishes.
fn build_scratch_dir() -> Result<PathBuf, AgentError> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);

    let dir = std::env::temp_dir().join(format!(
        "{}build-{}-{}",
        crate::tempdirs::TEMP_PREFIX,
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The linker script with the load / stack addresses substituted.
fn link_ld_contents(link_addr: &str) -> Result<String, AgentError> {
    let addr = parse_addr(link_addr)?;
    let stack = addr + 0x0800_0000; // 128 MiB above the load address
    Ok(LINK_LD
        .replace("LINK_ADDR", &format!("0x{addr:08x}"))
        .replace("STACK_ADDR", &format!("0x{stack:08x}")))
}

/// Write the generated linker script into `dir` and return its path.
///
/// Shared by both languages (v0.9 F3a): the script names neither compiler, so Zig reuses
/// it verbatim (`OUTPUT_ARCH`, `ENTRY(_start)`, `.text.start` first, `_stack_top`).
fn write_link_ld(dir: &Path, link_addr: &str) -> Result<PathBuf, AgentError> {
    let path = dir.join("link.ld");
    std::fs::write(&path, link_ld_contents(link_addr)?)?;
    Ok(path)
}

fn parse_addr(s: &str) -> Result<u64, AgentError> {
    u64::from_str_radix(s.trim().trim_start_matches("0x"), 16)
        .map_err(|e| AgentError::Tool(format!("invalid link address {s:?}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    /// `true` when a real RISC-V GCC can be found.
    ///
    /// The two tests below compile C for real, so they need the toolchain — not just this
    /// wrapper. A machine without one (see `ENVIRONMENT.md`) prints a skip instead of failing:
    /// the gate prints every skip, and the machine that has the toolchain runs them for real,
    /// so nothing is skipped silently. (Verified 2026-09-24: the Linux CI job has no
    /// `riscv64-unknown-elf-gcc`, which is what made these two fail there.)
    fn have_riscv_gcc() -> bool {
        CompilerConfig::discover().is_ok()
    }

    #[test]
    fn compiles_hello_fixture() {
        if !have_riscv_gcc() {
            eprintln!("skip: compiles_hello_fixture -- no RISC-V GCC found (ENVIRONMENT.md)");
            return;
        }
        let cfg = CompilerConfig::from_env();
        let out = std::env::temp_dir().join(format!(
            "riscdom-build-test-hello-{}.elf",
            std::process::id()
        ));
        let result = compile_freestanding(&cfg, &fixture("hello.c"), &out).expect("run gcc");
        assert!(
            result.ok,
            "compile failed:\nstdout={}\nstderr={}",
            result.stdout, result.stderr
        );
        assert!(out.exists());
    }

    #[test]
    fn reports_compile_failure_without_panicking() {
        if !have_riscv_gcc() {
            eprintln!(
                "skip: reports_compile_failure_without_panicking -- no RISC-V GCC found (ENVIRONMENT.md)"
            );
            return;
        }
        let cfg = CompilerConfig::from_env();
        let out =
            std::env::temp_dir().join(format!("riscdom-build-test-bad-{}.elf", std::process::id()));
        let result = compile_freestanding(&cfg, &fixture("broken.c"), &out).expect("run gcc");
        assert!(!result.ok, "broken source must not compile");
        assert!(
            !result.stderr.trim().is_empty(),
            "stderr should be captured"
        );
    }

    #[test]
    fn diagnostics_lists_the_search() {
        let text = CompilerConfig::diagnostics();
        assert!(text.contains("RISC-V GCC search:"), "{text}");
        assert!(text.contains("RISCDOM_RISCV_GCC"), "{text}");
        assert!(text.contains("RISCV_GCC"), "{text}");
        assert!(text.contains("=> "), "{text}");
    }

    /// The language is chosen by extension, not by a flag (v0.9 F3a, F3b-1).
    #[test]
    fn the_language_follows_the_extension() {
        assert!(is_zig_source(Path::new("hello.zig")));
        assert!(is_zig_source(Path::new("HELLO.ZIG")));
        assert!(is_rust_source(Path::new("hello.rs")));
        assert!(is_rust_source(Path::new("HELLO.RS")));
        assert!(!is_zig_source(Path::new("hello.c")));
        assert!(!is_rust_source(Path::new("hello.c")));
        assert!(!is_zig_source(Path::new("hello")));
        assert!(!is_rust_source(Path::new("hello")));
    }

    /// `true` when a real Zig can be found.
    ///
    /// The test below compiles Zig for real, so it needs the compiler — not just this
    /// wrapper. A machine without one (see `ENVIRONMENT.md`) prints a skip instead of
    /// failing: the gate prints every skip, and the machine that has Zig runs it for real,
    /// so nothing is skipped silently — the same shape as the two C tests above.
    fn have_zig() -> bool {
        ZigConfig::discover().is_ok()
    }

    #[test]
    fn diagnostics_lists_the_zig_search() {
        let text = ZigConfig::diagnostics();
        assert!(text.contains("Zig search:"), "{text}");
        assert!(text.contains("RISCDOM_ZIG"), "{text}");
        assert!(text.contains("=> "), "{text}");
    }

    #[test]
    fn compiles_hello_zig_fixture() {
        if !have_zig() {
            eprintln!("skip: compiles_hello_zig_fixture -- no Zig found (ENVIRONMENT.md)");
            return;
        }
        let cfg = CompilerConfig::from_env();
        let out = std::env::temp_dir().join(format!(
            "riscdom-build-test-hello-zig-{}.elf",
            std::process::id()
        ));
        let result = compile_freestanding(&cfg, &fixture("hello.zig"), &out).expect("run zig");
        assert!(
            result.ok,
            "compile failed:\nstdout={}\nstderr={}",
            result.stdout, result.stderr
        );
        assert!(out.exists());
    }

    /// The Rust command line is the one thing the Rust path can pin without the target's
    /// `core`: everything else about it needs a real sysroot (v0.9 F3b-1).
    #[test]
    fn rustc_args_are_the_documented_ones() {
        let cfg = RustConfig {
            rustc: Some(PathBuf::from("/rust/bin/rustc")),
            source: ToolchainSource::Path,
            sysroot: Some(PathBuf::from("/rust/rust-std-riscv64gc-unknown-none-elf")),
            target: RUST_TARGET.into(),
            edition: RUST_EDITION.into(),
        };
        let args = rustc_args(
            &cfg,
            Path::new("/rust/rust-std-riscv64gc-unknown-none-elf"),
            Path::new("/gcc/bin/riscv64-unknown-elf-gcc"),
            Path::new("/tmp/build/link.ld"),
            Path::new("hello.rs"),
            Path::new("/tmp/hello.elf"),
        );
        assert_eq!(
            args,
            [
                "hello.rs",
                "--target=riscv64gc-unknown-none-elf",
                "--sysroot=/rust/rust-std-riscv64gc-unknown-none-elf",
                "--edition=2021",
                "-Clinker=/gcc/bin/riscv64-unknown-elf-gcc",
                "-Clink-arg=-T",
                "-Clink-arg=/tmp/build/link.ld",
                "-Clink-arg=-nostartfiles",
                "-Clink-arg=-march=rv64gc",
                "-Clink-arg=-mabi=lp64d",
                "-Cpanic=abort",
                "-Crelocation-model=static",
                "-Ccode-model=medany",
                "-o",
                "/tmp/hello.elf",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<String>>()
        );
    }

    /// Neither half of a Rust build may fail silently (v0.9 F3b-1).
    #[test]
    fn rust_config_names_the_half_that_is_missing() {
        let without_rustc = RustConfig {
            rustc: None,
            source: ToolchainSource::Path,
            sysroot: Some(PathBuf::from("/rust")),
            target: RUST_TARGET.into(),
            edition: RUST_EDITION.into(),
        };
        let err = without_rustc
            .require()
            .expect_err("no rustc must be refused")
            .to_string();
        assert!(err.contains("rustc not found"), "{err}");

        let without_sysroot = RustConfig {
            rustc: Some(PathBuf::from("/rust/bin/rustc")),
            sysroot: None,
            ..without_rustc.clone()
        };
        let err = without_sysroot
            .require()
            .expect_err("no sysroot must be refused")
            .to_string();
        assert!(err.contains("rust_sysroot"), "{err}");
        assert!(err.contains(RUST_TARGET), "{err}");
    }

    /// `true` when a `.rs` source has both halves: a `rustc` and a sysroot carrying the
    /// target's `core`.
    ///
    /// The Rust test below compiles for real, so a machine without them (this one, at the
    /// time of writing: the target's `std` is not installed) prints a skip — the same shape
    /// as the C and Zig tests, so nothing is skipped silently.
    fn have_rust_std() -> bool {
        CompilerConfig::from_env().rust.require().is_ok()
    }

    #[test]
    fn rust_diagnostics_list_the_search() {
        let text = RustConfig::diagnostics();
        assert!(text.contains("rustc search:"), "{text}");
        assert!(text.contains("RISCDOM_RUSTC"), "{text}");
        assert!(text.contains("=> "), "{text}");
    }

    #[test]
    fn compiles_hello_rs_fixture() {
        if !have_rust_std() {
            eprintln!(
                "skip: compiles_hello_rs_fixture -- no rustc + target sysroot (ENVIRONMENT.md)"
            );
            return;
        }
        let cfg = CompilerConfig::from_env();
        let out = std::env::temp_dir().join(format!(
            "riscdom-build-test-hello-rs-{}.elf",
            std::process::id()
        ));
        let result = compile_freestanding(&cfg, &fixture("hello.rs"), &out).expect("run rustc");
        assert!(
            result.ok,
            "compile failed:\nstdout={}\nstderr={}",
            result.stdout, result.stderr
        );
        assert!(out.exists());
    }
}
