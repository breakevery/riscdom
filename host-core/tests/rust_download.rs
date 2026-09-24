//! v0.9 multi-language batch F3b-2 — the Rust sysroot, end to end.
//!
//! The unit tests in `toolchain_download.rs` cover the spec and the locator. This file proves the
//! **dispatch**: a spec whose `toolchain` is `Rust` travels the whole download path (fetch →
//! checksum → extract → locate → install) and comes back with the **sysroot directory**, which is
//! what `--sysroot` needs. Like the other download tests it runs offline, against a loopback
//! server.

mod common;

use common::{rust_spec_for, rust_std_archive, unique_dir, MockServer};
use host_core::toolchain_download::{download_and_install, RUST_VERSION};
use std::sync::atomic::AtomicBool;

#[test]
fn a_rust_std_archive_downloads_extracts_and_finds_the_sysroot() {
    let dir = unique_dir("rust-download");
    let (bytes, hash) = rust_std_archive();
    let server = MockServer::start(bytes);
    let spec = rust_spec_for(&server, hash, "rust-std.tar.xz", RUST_VERSION);

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let sysroot =
        download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("download must succeed");

    println!("sysroot: {}", sysroot.display());
    assert!(sysroot.is_dir(), "{}", sysroot.display());
    // The product is the `rust-std-<target>/` directory — the one `--sysroot` points at — and it
    // carries the target's libraries, which is what `set_rust_sysroot` checks too.
    assert_eq!(
        sysroot.file_name().and_then(|n| n.to_str()),
        Some(common::rust_std_dir_name().as_str())
    );
    assert!(sysroot
        .join("lib")
        .join("rustlib")
        .join(agent::RUST_TARGET)
        .join("lib")
        .is_dir());
    assert_eq!(server.hits(), 1);
    assert!(dest.join(RUST_VERSION).is_dir());
}
