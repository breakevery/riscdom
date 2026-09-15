//! Stage v0.3-5a — locating the QEMU system binary.
//!
//! Everything runs offline: the checks only look at the environment and the
//! filesystem. The environment cases share one test because the variables are
//! process-global.

use sandbox::qemu_discover::{
    diagnostics, discover, exe_name, QemuDiscoverError, QemuSource, QEMU_DOWNLOAD_URL,
};

#[test]
fn discovery_order_and_actionable_errors() {
    // 1. This machine has QEMU (the other sandbox tests boot it).
    let location = discover().expect("QEMU must be discoverable here");
    assert!(location.exe.is_file(), "{}", location.exe.display());
    println!(
        "discovered: {} ({})",
        location.exe.display(),
        location.source
    );
    assert!(location
        .exe
        .to_string_lossy()
        .contains("qemu-system-riscv64"));

    // 2. diagnostics() reports the search.
    let text = diagnostics();
    println!("{text}");
    assert!(text.contains("QEMU search:"), "{text}");
    assert!(text.contains("RISCDOM_QEMU"), "{text}");
    assert!(text.contains("QEMU_SYSTEM_RISCV64"), "{text}");
    assert!(text.contains("=> "), "{text}");

    // 3. An environment variable pointing at a missing file is an explicit,
    //    actionable error (never silently skipped).
    std::env::set_var(
        "RISCDOM_QEMU",
        r"C:\definitely\not\here\qemu-system-riscv64.exe",
    );
    let err = discover().expect_err("must fail");
    let msg = err.to_string();
    println!("--- error message ---\n{msg}\n---------------------");
    assert!(
        msg.starts_with("QEMU (qemu-system-riscv64) not found."),
        "{msg}"
    );
    assert!(
        msg.contains("set, but that file does not exist"),
        "the stale env var must be reported: {msg}"
    );
    assert!(msg.contains("Searched:"), "{msg}");
    assert!(
        msg.contains(QEMU_DOWNLOAD_URL),
        "install link missing: {msg}"
    );
    assert!(msg.contains("QEMU"), "{msg}");
    assert!(
        msg.contains("set RISCDOM_QEMU"),
        "remediation missing: {msg}"
    );
    assert_eq!(err.code(), "qemu_not_found");
    assert!(matches!(err, QemuDiscoverError::NotFound(_)));

    // 4. Pointing at a real file gives `source == EnvVar`.
    std::env::set_var("RISCDOM_QEMU", &location.exe);
    let hit = discover().expect("env hit");
    assert_eq!(hit.source, QemuSource::EnvVar);
    assert_eq!(hit.exe, location.exe);

    // 5. `QEMU_SYSTEM_RISCV64` is honoured as the generic fallback name.
    std::env::remove_var("RISCDOM_QEMU");
    std::env::set_var("QEMU_SYSTEM_RISCV64", &location.exe);
    let hit = discover().expect("generic env hit");
    assert_eq!(hit.source, QemuSource::EnvVar);

    std::env::remove_var("QEMU_SYSTEM_RISCV64");
    assert!(exe_name().starts_with("qemu-system-riscv64"));
}

#[test]
fn the_search_log_always_starts_with_the_environment_variables() {
    // The search stops at the first hit, so only the first lines are guaranteed:
    // both environment variables are always reported.
    std::env::remove_var("RISCDOM_QEMU");
    std::env::remove_var("QEMU_SYSTEM_RISCV64");
    let text = diagnostics();
    println!("{text}");
    assert!(text.contains("QEMU search:"), "{text}");
    assert!(text.contains("- RISCDOM_QEMU (not set)"), "{text}");
    assert!(text.contains("- QEMU_SYSTEM_RISCV64 (not set)"), "{text}");
    assert!(text.contains("=> "), "{text}");
}
