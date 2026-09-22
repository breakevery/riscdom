//! Stage v0.3-3a — toolchain download, verification and extraction.
//!
//! Everything here runs offline: a loopback `TcpListener` serves a small archive
//! built in the test (see `common`), so no network access is involved.

mod common;

use common::{build_archive, spec_for, toolchain_archive, unique_dir, MockServer, COMPILER_ENTRY};
use host_core::toolchain_download::{
    download_and_install, DownloadEvent, ToolchainDownloadError, XPACK_RISCV_GCC_VERSION,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn downloads_verifies_extracts_and_finds_the_compiler() {
    let dir = unique_dir("download-happy");
    let (bytes, hash) = toolchain_archive();
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "toolchain-archive");

    let cancel = AtomicBool::new(false);
    let mut events: Vec<String> = Vec::new();
    let dest = dir.join("install");
    let compiler = download_and_install(&spec, &dest, &cancel, &mut |event| {
        events.push(match event {
            DownloadEvent::Started { .. } => "started".into(),
            DownloadEvent::Progress { .. } => "progress".into(),
            DownloadEvent::Verifying => "verifying".into(),
            DownloadEvent::Extracting => "extracting".into(),
            DownloadEvent::Done { .. } => "done".into(),
            DownloadEvent::Failed { .. } => "failed".into(),
            DownloadEvent::Cancelled => "cancelled".into(),
        });
    })
    .expect("download must succeed");

    println!("compiler: {}", compiler.display());
    println!("events: {events:?}");
    assert!(compiler.is_file(), "{}", compiler.display());
    assert!(compiler.to_string_lossy().contains("riscv-none-elf-gcc"));
    assert_eq!(events.first().map(String::as_str), Some("started"));
    assert!(events.iter().any(|e| e == "progress"), "{events:?}");
    assert!(events.contains(&"verifying".to_string()), "{events:?}");
    assert!(events.contains(&"extracting".to_string()), "{events:?}");
    assert_eq!(events.last().map(String::as_str), Some("done"));
    assert_eq!(server.hits(), 1);
    // The temporary archive is gone after a successful install.
    assert!(!dir
        .join("install")
        .join(".download-tmp")
        .join("toolchain-archive")
        .exists());
}

#[test]
fn a_wrong_checksum_is_rejected_and_the_temp_file_removed() {
    let dir = unique_dir("download-checksum");
    let (bytes, _real_hash) = toolchain_archive();
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, "0".repeat(64), "toolchain-archive");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let err = download_and_install(&spec, &dest, &cancel, &mut |_| {})
        .expect_err("checksum mismatch must fail");
    println!("{err}");
    assert!(matches!(
        err,
        ToolchainDownloadError::ChecksumMismatch { .. }
    ));
    assert_eq!(err.code(), "checksum_mismatch");
    assert!(!dir
        .join("install")
        .join(".download-tmp")
        .join("toolchain-archive")
        .exists());
    assert!(!dest.join(XPACK_RISCV_GCC_VERSION).exists());
}

#[test]
fn cancelling_stops_the_download_and_cleans_up() {
    let dir = unique_dir("download-cancel");
    let (bytes, hash) = toolchain_archive();
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "toolchain-archive");

    // Cancelled before the first chunk: the download must not complete.
    let cancel = AtomicBool::new(true);
    let dest = dir.join("install");
    let err =
        download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect_err("cancel must abort");
    assert!(matches!(err, ToolchainDownloadError::Cancelled));
    assert!(cancel.load(Ordering::Relaxed));
    assert!(!dir
        .join("install")
        .join(".download-tmp")
        .join("toolchain-archive")
        .exists());
    assert!(!dest.join(XPACK_RISCV_GCC_VERSION).exists());
}

#[test]
fn archive_entries_escaping_the_destination_are_refused() {
    let dir = unique_dir("download-slip");
    let (bytes, hash) = build_archive(&[(COMPILER_ENTRY, b"ok\n"), ("../escaped.txt", b"nope\n")]);
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "toolchain-archive");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let err = download_and_install(&spec, &dest, &cancel, &mut |_| {})
        .expect_err("an escaping entry must be refused");
    println!("{err}");
    assert!(
        matches!(err, ToolchainDownloadError::UnsafeEntry(_)),
        "{err}"
    );
    assert_eq!(err.code(), "unsafe_entry");
    assert!(!dir.join("escaped.txt").exists(), "nothing may escape");
    assert!(!dest.join(XPACK_RISCV_GCC_VERSION).exists());
}

#[test]
fn a_second_run_is_idempotent_and_does_not_download_again() {
    let dir = unique_dir("download-idempotent");
    let (bytes, hash) = toolchain_archive();
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "toolchain-archive");
    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");

    let first = download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("first");
    assert_eq!(server.hits(), 1);
    let second = download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("second");
    assert_eq!(first, second, "the same compiler path must be returned");
    assert_eq!(server.hits(), 1, "the second call must not hit the network");
}
