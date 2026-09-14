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

use crate::error::AgentError;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Compiler configuration.
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// RISC-V cross compiler (`RISCDOM_RISCV_GCC` or a default path).
    pub gcc: PathBuf,
    /// `-march` value.
    pub march: String,
    /// `-mabi` value.
    pub mabi: String,
    /// Load address the image is linked at.
    pub link_addr: String,
}

impl CompilerConfig {
    /// Build from the environment with sane defaults.
    pub fn from_env() -> Self {
        let gcc = std::env::var("RISCDOM_RISCV_GCC")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_gcc);
        Self {
            gcc,
            march: "rv64gc".into(),
            mabi: "lp64d".into(),
            link_addr: "0x80000000".into(),
        }
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

/// Default cross compiler location when `RISCDOM_RISCV_GCC` is unset.
fn default_gcc() -> PathBuf {
    if cfg!(windows) {
        let p = PathBuf::from(r"D:\tools\riscv64-unknown-elf\bin\riscv64-unknown-elf-gcc.exe");
        if p.exists() {
            return p;
        }
        PathBuf::from("riscv64-unknown-elf-gcc.exe")
    } else {
        PathBuf::from("riscv64-unknown-elf-gcc")
    }
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
        .map_err(|e| AgentError::Tool(format!("failed to run {}: {e}", cfg.gcc.display())))?;

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
        assert!(!result.stderr.trim().is_empty(), "stderr should be captured");
    }
}
