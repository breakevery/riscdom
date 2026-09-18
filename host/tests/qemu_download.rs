//! v0.4 #4 — the QEMU downloader: fetch, verify, extract, install.
//!
//! Everything here runs offline: a loopback `TcpListener` serves a small archive
//! built in the test (see `common`), so no network access is involved. The pattern
//! is the RISC-V toolchain's (`download_tests.rs`), applied to QEMU.

mod common;

use common::{build_archive, qemu_archive, qemu_entry, qemu_spec_for, unique_dir, MockServer};
use host::qemu_download::{
    download_and_install, QemuDownloadError, QemuDownloadEvent, QEMU_VERSION,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn downloads_verifies_extracts_and_finds_the_emulator() {
    let dir = unique_dir("qemu-download-happy");
    let (bytes, hash) = qemu_archive();
    let server = MockServer::start(bytes);
    let spec = qemu_spec_for(&server, hash, "qemu-archive");

    let cancel = AtomicBool::new(false);
    let mut events: Vec<String> = Vec::new();
    let dest = dir.join("install");
    let qemu = download_and_install(&spec, &dest, &cancel, &mut |event| {
        events.push(match event {
            QemuDownloadEvent::Started { .. } => "started".into(),
            QemuDownloadEvent::Progress { .. } => "progress".into(),
            QemuDownloadEvent::Verifying => "verifying".into(),
            QemuDownloadEvent::Extracting => "extracting".into(),
            QemuDownloadEvent::Done { .. } => "done".into(),
            QemuDownloadEvent::Failed { .. } => "failed".into(),
            QemuDownloadEvent::Cancelled => "cancelled".into(),
        });
    })
    .expect("download must succeed");

    println!("qemu: {}", qemu.display());
    println!("events: {events:?}");
    assert!(qemu.is_file(), "{}", qemu.display());
    assert_eq!(
        qemu.file_name().and_then(|name| name.to_str()),
        Some(sandbox::qemu_discover::exe_name().as_str())
    );
    assert_eq!(events.first().map(String::as_str), Some("started"));
    assert!(events.iter().any(|e| e == "progress"), "{events:?}");
    assert!(events.contains(&"verifying".to_string()), "{events:?}");
    assert!(events.contains(&"extracting".to_string()), "{events:?}");
    assert_eq!(events.last().map(String::as_str), Some("done"));
    assert_eq!(server.hits(), 1);
    // The temporary archive is gone after a successful install.
    assert!(!dest.join(".download-tmp").join(spec.file_name()).exists());
}

#[test]
fn a_wrong_checksum_is_rejected_and_the_temp_file_removed() {
    let dir = unique_dir("qemu-download-checksum");
    let (bytes, _real_hash) = qemu_archive();
    let server = MockServer::start(bytes);
    let spec = qemu_spec_for(&server, "0".repeat(64), "qemu-archive");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let err = download_and_install(&spec, &dest, &cancel, &mut |_| {})
        .expect_err("checksum mismatch must fail");
    println!("{err}");
    assert!(matches!(err, QemuDownloadError::ChecksumMismatch { .. }));
    assert_eq!(err.code(), "checksum_mismatch");
    assert!(!dest.join(".download-tmp").join(spec.file_name()).exists());
    assert!(!dest.join(QEMU_VERSION).exists());
}

#[test]
fn cancelling_stops_the_download_and_cleans_up() {
    let dir = unique_dir("qemu-download-cancel");
    let (bytes, hash) = qemu_archive();
    let server = MockServer::start(bytes);
    let spec = qemu_spec_for(&server, hash, "qemu-archive");

    // Cancelled before the first chunk: the download must not complete.
    let cancel = AtomicBool::new(true);
    let dest = dir.join("install");
    let err =
        download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect_err("cancel must abort");
    assert!(matches!(err, QemuDownloadError::Cancelled));
    assert!(cancel.load(Ordering::Relaxed));
    assert!(!dest.join(".download-tmp").join(spec.file_name()).exists());
    assert!(!dest.join(QEMU_VERSION).exists());
}

#[test]
fn archive_entries_escaping_the_destination_are_refused() {
    let dir = unique_dir("qemu-download-slip");
    let entry = qemu_entry();
    let (bytes, hash) = build_archive(&[
        (entry.as_str(), b"ok\n".as_slice()),
        ("../escaped.txt", b"nope\n".as_slice()),
    ]);
    let server = MockServer::start(bytes);
    let spec = qemu_spec_for(&server, hash, "qemu-archive");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let err = download_and_install(&spec, &dest, &cancel, &mut |_| {})
        .expect_err("an escaping entry must be refused");
    println!("{err}");
    assert!(matches!(err, QemuDownloadError::UnsafeEntry(_)), "{err}");
    assert_eq!(err.code(), "unsafe_entry");
    assert!(!dir.join("escaped.txt").exists(), "nothing may escape");
    assert!(!dest.join(QEMU_VERSION).exists());
}

#[test]
fn a_second_run_is_idempotent_and_does_not_download_again() {
    let dir = unique_dir("qemu-download-idempotent");
    let (bytes, hash) = qemu_archive();
    let server = MockServer::start(bytes);
    let spec = qemu_spec_for(&server, hash, "qemu-archive");
    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");

    let first = download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("first");
    assert_eq!(server.hits(), 1);
    let second = download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("second");
    assert_eq!(first, second, "the same emulator path must be returned");
    assert_eq!(server.hits(), 1, "the second call must not hit the network");
}

#[test]
fn an_archive_without_qemu_is_reported_as_such() {
    let dir = unique_dir("qemu-download-missing");
    let (bytes, hash) = build_archive(&[("qemu/README.txt", b"no emulator here\n".as_slice())]);
    let server = MockServer::start(bytes);
    let spec = qemu_spec_for(&server, hash, "qemu-archive");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let err = download_and_install(&spec, &dest, &cancel, &mut |_| {})
        .expect_err("an archive without the emulator must fail");
    assert!(matches!(err, QemuDownloadError::NoQemuInArchive), "{err}");
    assert_eq!(err.code(), "no_qemu_in_archive");
    assert!(!dest.join(QEMU_VERSION).exists());
}
