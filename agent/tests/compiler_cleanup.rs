//! v0.4 batch 3-followup-2 — a build's scratch directory does not outlive it.
//!
//! The unique per-build path fixed the race (3-followup) but leaked a directory per
//! compile. This file is deliberately the only test in its binary: the assertion is
//! about *this* process's scratch directories, and a concurrent compile in the same
//! process would make it ambiguous.

use agent::compiler::{compile_freestanding, CompilerConfig};
use std::path::PathBuf;

const HELLO_C: &str = include_str!("fixtures/hello.c");

/// Build scratch directories belonging to this process.
fn build_dirs_for_this_process() -> Vec<PathBuf> {
    let prefix = format!("riscdom-build-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_dir()
                        && path
                            .file_name()
                            .map(|name| name.to_string_lossy().starts_with(&prefix))
                            .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
#[ignore = "requires a discoverable RISC-V GCC; run with --include-ignored"]
fn a_build_leaves_no_scratch_directory_behind() {
    let dir = std::env::temp_dir().join(format!("riscdom-cleanup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("workspace dir");
    let src = dir.join("hello.c");
    let elf = dir.join("hello.elf");
    std::fs::write(&src, HELLO_C).expect("write source");

    let cfg = CompilerConfig::from_env();

    // 1. a successful build: the ELF survives, the scratch does not
    let ok = compile_freestanding(&cfg, &src, &elf).expect("run gcc");
    assert!(ok.ok, "stderr: {}", ok.stderr);
    assert!(elf.is_file(), "the ELF must still be produced");
    let leftovers = build_dirs_for_this_process();
    assert!(
        leftovers.is_empty(),
        "the build scratch must be removed: {leftovers:?}"
    );

    // 2. a failing build cleans up after itself too
    let broken = dir.join("broken.c");
    std::fs::write(&broken, "int main(void) { this is not C }").expect("write source");
    let failed = compile_freestanding(&cfg, &broken, &dir.join("broken.elf")).expect("run gcc");
    assert!(!failed.ok, "broken.c must not compile");
    let leftovers = build_dirs_for_this_process();
    assert!(
        leftovers.is_empty(),
        "a failed build must clean up as well: {leftovers:?}"
    );
}
