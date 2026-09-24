//! v0.9 multi-language batch F3a-download — the `.tar.xz` archive kind, end to end.
//!
//! The unit tests in `toolchain_download.rs` cover the extractor itself. This file proves
//! the **dispatch arm**: a spec whose `archive_kind` is `TarXz` travels the whole download
//! path (fetch → checksum → extract → install) and comes back with the compiler. Like the
//! other download tests it runs offline, against a loopback server.

mod common;

use common::{spec_for_tar_xz, tar_xz_archive, unique_dir, MockServer};
use host_core::toolchain_download::{download_and_install, XPACK_RISCV_GCC_VERSION};
use std::sync::atomic::AtomicBool;

#[test]
fn a_tar_xz_archive_downloads_extracts_and_finds_the_compiler() {
    let dir = unique_dir("tarxz-download");
    let (bytes, hash) = tar_xz_archive();
    let server = MockServer::start(bytes);
    let spec = spec_for_tar_xz(&server, hash, "toolchain-archive");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let compiler =
        download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("download must succeed");

    println!("compiler: {}", compiler.display());
    assert!(compiler.is_file(), "{}", compiler.display());
    assert!(compiler.to_string_lossy().contains("riscv-none-elf-gcc"));
    assert_eq!(server.hits(), 1);
    assert!(dest.join(XPACK_RISCV_GCC_VERSION).is_dir());
}
