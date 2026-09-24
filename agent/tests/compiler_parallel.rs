//! v0.4 batch 3-followup — concurrent builds must not share files.
//!
//! The compiler injects `crt0.S` and a linker script; those used to live at one
//! fixed temporary path, so two builds running at once compiled against a file
//! the other was rewriting. Every build now gets its own directory.

use agent::compiler::{compile_freestanding, CompilerConfig};
use std::path::PathBuf;

const HELLO_C: &str = include_str!("fixtures/hello.c");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-parallel-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[ignore = "requires a discoverable RISC-V GCC; run with --include-ignored"]
fn many_builds_at_once_do_not_collide() {
    let dir = unique_dir("builds");
    let cfg = CompilerConfig::from_env();
    let per_round = 8;
    let rounds = 5;

    for round in 0..rounds {
        let mut handles = Vec::new();
        for i in 0..per_round {
            let cfg = cfg.clone();
            let src = dir.join(format!("hello-{round}-{i}.c"));
            let elf = dir.join(format!("hello-{round}-{i}.elf"));
            std::fs::write(&src, HELLO_C).expect("write source");
            handles.push(std::thread::spawn(move || {
                let result = compile_freestanding(&cfg, &src, &elf).expect("run gcc");
                (result.ok, result.stderr, elf.exists())
            }));
        }
        for (index, handle) in handles.into_iter().enumerate() {
            let (ok, stderr, exists) = handle.join().expect("compile thread");
            assert!(
                ok && exists,
                "round {round}, build {index} failed against a shared file: {stderr}"
            );
        }
    }
}
