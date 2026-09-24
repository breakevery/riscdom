//! v0.9 multi-language batch F3a-download-apply — the Zig toolchain, end to end.
//!
//! The unit tests in `toolchain_download.rs` cover the spec table and the locator. This file
//! proves the **dispatch**: a spec whose `toolchain` is `Zig` travels the whole download path
//! (fetch → checksum → extract → locate → install) and comes back with the Zig binary, not
//! with a compiler it never contained. Like the other download tests it runs offline, against
//! a loopback server.

mod common;

use common::{unique_dir, zig_archive, zig_spec_for, MockServer};
use host_core::toolchain_download::{download_and_install, ZIG_VERSION};
use std::sync::atomic::AtomicBool;

#[test]
fn a_zig_archive_downloads_extracts_and_finds_the_binary() {
    let dir = unique_dir("zig-download");
    let (bytes, hash) = zig_archive();
    let server = MockServer::start(bytes);
    let spec = zig_spec_for(&server, hash, "zig-archive");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let zig =
        download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("download must succeed");

    println!("zig: {}", zig.display());
    assert!(zig.is_file(), "{}", zig.display());
    assert!(
        zig.file_name().and_then(|n| n.to_str()) == Some("zig")
            || zig.file_name().and_then(|n| n.to_str()) == Some("zig.exe"),
        "the located product must be the Zig binary: {}",
        zig.display()
    );
    // It sits beside `lib/`, which is what Zig's own README says a release is made of.
    assert!(zig.parent().expect("a parent").join("lib").is_dir());
    assert_eq!(server.hits(), 1);
    assert!(dest.join(ZIG_VERSION).is_dir());
}
