//! Stage 24c — RISC-V toolchain discovery.
//!
//! Everything lives in one test on purpose: the environment variables are
//! process-global, so the cases must not run concurrently.

use agent::compiler::{CompilerConfig, ToolchainSource};

#[test]
#[ignore = "requires a discoverable RISC-V GCC; run with --include-ignored"]
fn discovery_precedence_and_actionable_errors() {
    // 1. This machine must have a discoverable toolchain (the other agent tests
    //    already require a working compiler).
    let discovered = CompilerConfig::discover().expect("a toolchain must be discoverable here");
    assert!(discovered.is_file(), "{}", discovered.display());
    println!("discovered: {}", discovered.display());

    // 2. diagnostics() reports the search (env vars + what was tried + result).
    let text = CompilerConfig::diagnostics();
    println!("{text}");
    assert!(text.contains("RISC-V GCC search:"), "{text}");
    assert!(text.contains("RISCDOM_RISCV_GCC"), "{text}");
    assert!(text.contains("RISCV_GCC"), "{text}");
    assert!(text.contains("=> "), "{text}");

    // 3. An environment variable pointing at a missing file is an explicit,
    //    actionable error (never silently ignored).
    std::env::set_var(
        "RISCDOM_RISCV_GCC",
        r"C:\definitely\not\here\riscv64-unknown-elf-gcc.exe",
    );
    let err = CompilerConfig::discover().expect_err("must fail");
    let msg = err.to_string();
    println!("--- error message ---\n{msg}\n---------------------");
    assert!(msg.starts_with("RISC-V GCC not found."), "{msg}");
    assert!(
        msg.contains("set, but that file does not exist"),
        "the stale env var must be reported: {msg}"
    );
    assert!(msg.contains("Searched:"), "{msg}");
    assert!(
        msg.contains("xpack-dev-tools"),
        "install link missing: {msg}"
    );
    assert!(
        msg.contains("set RISCDOM_RISCV_GCC"),
        "remediation missing: {msg}"
    );
    let cfg = CompilerConfig::from_env();
    assert_eq!(
        cfg.gcc.components().count(),
        1,
        "a stale env var must not be used as a path: {:?}",
        cfg.gcc
    );

    // 4. Pointing at a real file gives `source == EnvVar`.
    std::env::set_var("RISCDOM_RISCV_GCC", &discovered);
    let cfg = CompilerConfig::from_env();
    assert_eq!(cfg.source, ToolchainSource::EnvVar);
    assert_eq!(cfg.gcc, discovered);
    assert_eq!(CompilerConfig::discover().expect("env hit"), discovered);

    // 5. `RISCV_GCC` is honoured as the generic fallback name.
    std::env::remove_var("RISCDOM_RISCV_GCC");
    std::env::set_var("RISCV_GCC", &discovered);
    let cfg = CompilerConfig::from_env();
    assert_eq!(cfg.source, ToolchainSource::EnvVar);

    std::env::remove_var("RISCV_GCC");
}
