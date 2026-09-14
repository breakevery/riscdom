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
}

/// Executable names we accept, in preference order (xPack ships the `riscv-none-elf-` prefix).
const GCC_NAMES: [&str; 2] = ["riscv64-unknown-elf-gcc", "riscv-none-elf-gcc"];

/// Environment variables consulted, in priority order.
const GCC_ENV_VARS: [&str; 2] = ["RISCDOM_RISCV_GCC", "RISCV_GCC"];

/// Where to tell users to get a toolchain.
pub const TOOLCHAIN_URL: &str =
    "https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases";

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

/// Result of a compile attempt.
#[derive(Debug, Clone)]
pub struct CompileOutput {
    pub stdout: String,
    pub stderr: String,
    pub ok: bool,
}

/// Minimal startup code injected into every build.
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
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        for entry in list_dirs(&dir.to_string_lossy()) {
            for name in GCC_NAMES {
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

/// Compile `src` into a freestanding ELF at `out`.
pub fn compile_freestanding(
    cfg: &CompilerConfig,
    src: &Path,
    out: &Path,
) -> Result<CompileOutput, AgentError> {
    let (crt0, link_ld) = write_build_files(&cfg.link_addr)?;

    let output = Command::new(&cfg.gcc)
        .arg(format!("-march={}", cfg.march))
        .arg(format!("-mabi={}", cfg.mabi))
        .arg("-mcmodel=medany")
        .arg("-ffreestanding")
        .arg("-nostdlib")
        .arg("-nostartfiles")
        .arg("-T")
        .arg(&link_ld)
        .arg("-o")
        .arg(out)
        .arg(&crt0)
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

/// Write the injected `crt0.S` and linker script to a shared build dir.
///
/// The directory is deterministic (overwritten per build) to avoid needing
/// cleanup; compiles are single-threaded in the MVP.
fn write_build_files(link_addr: &str) -> Result<(PathBuf, PathBuf), AgentError> {
    let dir = std::env::temp_dir().join("riscdom-build");
    std::fs::create_dir_all(&dir)?;

    let crt0 = dir.join("crt0.S");
    let link_ld = dir.join("link.ld");

    let addr = parse_addr(link_addr)?;
    let stack = addr + 0x0800_0000; // 128 MiB above the load address

    std::fs::write(&crt0, CRT0)?;
    std::fs::write(
        &link_ld,
        LINK_LD
            .replace("LINK_ADDR", &format!("0x{addr:08x}"))
            .replace("STACK_ADDR", &format!("0x{stack:08x}")),
    )?;
    Ok((crt0, link_ld))
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

    #[test]
    fn compiles_hello_fixture() {
        let cfg = CompilerConfig::from_env();
        let out = std::env::temp_dir().join("riscdom-build-test-hello.elf");
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
        let cfg = CompilerConfig::from_env();
        let out = std::env::temp_dir().join("riscdom-build-test-bad.elf");
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
}
