//! Stage v0.3-3a — toolchain download, verification and extraction.
//!
//! Everything here runs offline: a `TcpListener` on localhost serves a small zip
//! built in the test, so no network access is involved.

use host::toolchain_download::{
    download_and_install, ArchiveKind, DownloadEvent, DownloadSpec, ToolchainDownloadError,
    XPACK_RISCV_GCC_VERSION,
};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("riscdom-tcdl-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build a small zip archive that looks like the xPack layout.
fn build_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (name, body) in entries {
        writer.start_file(*name, options).expect("start entry");
        writer.write_all(body).expect("write entry");
    }
    writer.finish().expect("finish zip");
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A one-shot HTTP server that serves `body` for the first request it sees.
struct MockServer {
    port: u16,
    hits: Arc<AtomicUsize>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MockServer {
    fn start(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                counter.fetch_add(1, Ordering::Relaxed);
                // Read the request head (we do not care about its content).
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        Self {
            port,
            hits,
            handle: Some(handle),
        }
    }

    fn url(&self, name: &str) -> String {
        format!("http://127.0.0.1:{}/{}", self.port, name)
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        // The accept thread is parked in `accept()`; detaching it is deliberate.
        // It holds no test state and the listener dies with the test process.
        let _ = self.handle.take();
    }
}

fn spec_for(server: &MockServer, sha256: String, name: &str) -> DownloadSpec {
    DownloadSpec {
        version: XPACK_RISCV_GCC_VERSION.to_string(),
        url: server.url(name),
        sha256,
        archive_kind: ArchiveKind::Zip,
        install_subdir: format!("xpack-riscv-none-elf-gcc-{XPACK_RISCV_GCC_VERSION}"),
    }
}

const COMPILER_ENTRY: &str = "xpack-riscv-none-elf-gcc-15.2.0-1/bin/riscv-none-elf-gcc.exe";

fn fixture_zip(dir: &Path) -> (Vec<u8>, String) {
    let path = dir.join("fixture.zip");
    build_zip(
        &path,
        &[
            (COMPILER_ENTRY, b"#!/bin/sh\n# fake compiler\n"),
            (
                "xpack-riscv-none-elf-gcc-15.2.0-1/README.md",
                b"xpack fixture\n",
            ),
        ],
    );
    let bytes = std::fs::read(&path).expect("read fixture");
    let hash = sha256_hex(&bytes);
    (bytes, hash)
}

#[test]
fn downloads_verifies_extracts_and_finds_the_compiler() {
    let dir = unique_dir("happy");
    let (bytes, hash) = fixture_zip(&dir);
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "xpack-win32-x64.zip");

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
    assert!(compiler
        .to_string_lossy()
        .ends_with("riscv-none-elf-gcc.exe"));
    assert!(
        events.first().map(|e| e == "started").unwrap_or(false),
        "{events:?}"
    );
    assert!(events.iter().any(|e| e == "progress"), "{events:?}");
    assert!(events.contains(&"verifying".to_string()), "{events:?}");
    assert!(events.contains(&"extracting".to_string()), "{events:?}");
    assert_eq!(events.last().map(String::as_str), Some("done"));
    assert_eq!(server.hits(), 1);
    // The archive is removed after a successful install.
    assert!(!dest
        .join(".download-tmp")
        .join("xpack-win32-x64.zip")
        .exists());
}

#[test]
fn a_wrong_checksum_is_rejected_and_the_temp_file_removed() {
    let dir = unique_dir("checksum");
    let (bytes, _real_hash) = fixture_zip(&dir);
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, "0".repeat(64), "xpack-win32-x64.zip");

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
    assert!(!dest
        .join(".download-tmp")
        .join("xpack-win32-x64.zip")
        .exists());
    assert!(!dest.join(XPACK_RISCV_GCC_VERSION).exists());
}

#[test]
fn cancelling_stops_the_download_and_cleans_up() {
    let dir = unique_dir("cancel");
    let (bytes, hash) = fixture_zip(&dir);
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "xpack-win32-x64.zip");

    // Already cancelled before we start: the download must not even complete.
    let cancel = AtomicBool::new(true);
    let dest = dir.join("install");
    let err =
        download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect_err("cancel must abort");
    assert!(matches!(err, ToolchainDownloadError::Cancelled));
    assert!(!dest
        .join(".download-tmp")
        .join("xpack-win32-x64.zip")
        .exists());
    assert!(!dest.join(XPACK_RISCV_GCC_VERSION).exists());
}

#[test]
fn zip_slip_entries_are_refused() {
    let dir = unique_dir("slip");
    let path = dir.join("evil.zip");
    build_zip(
        &path,
        &[(COMPILER_ENTRY, b"ok\n"), ("../escaped.txt", b"nope\n")],
    );
    let bytes = std::fs::read(&path).expect("read");
    let hash = sha256_hex(&bytes);
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "xpack-win32-x64.zip");

    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");
    let err = download_and_install(&spec, &dest, &cancel, &mut |_| {})
        .expect_err("zip slip must be refused");
    println!("{err}");
    assert!(
        matches!(err, ToolchainDownloadError::UnsafeEntry(_)),
        "{err}"
    );
    assert_eq!(err.code(), "unsafe_entry");
    // Nothing escaped: the parent of the install root has no stray file.
    assert!(!dir.join("escaped.txt").exists());
    assert!(!dest.join(XPACK_RISCV_GCC_VERSION).exists());
}

#[test]
fn a_second_run_is_idempotent_and_does_not_download_again() {
    let dir = unique_dir("idempotent");
    let (bytes, hash) = fixture_zip(&dir);
    let server = MockServer::start(bytes);
    let spec = spec_for(&server, hash, "xpack-win32-x64.zip");
    let cancel = AtomicBool::new(false);
    let dest = dir.join("install");

    let first = download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("first");
    assert_eq!(server.hits(), 1);
    let second = download_and_install(&spec, &dest, &cancel, &mut |_| {}).expect("second");
    assert_eq!(first, second, "the same compiler path must be returned");
    assert_eq!(server.hits(), 1, "the second call must not hit the network");
}

#[test]
fn the_spec_matches_this_platform() {
    let spec = host::toolchain_download::spec_for_current_platform().expect("supported platform");
    println!("url: {}", spec.url);
    assert!(spec.url.starts_with("https://github.com/xpack-dev-tools/"));
    assert_eq!(spec.version, XPACK_RISCV_GCC_VERSION);
    assert_eq!(spec.sha256.len(), 64, "a real SHA-256 must be pinned");
    assert!(spec.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(spec.file_name().ends_with(".zip") || spec.file_name().ends_with(".tar.gz"));
}
