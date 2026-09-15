//! Shared helpers for the host integration tests.
//!
//! The download tests must never touch the network, so they run against a
//! loopback `TcpListener` serving a tiny archive built here.

#![allow(dead_code)]

use host::toolchain_download::ArchiveKind;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// A unique temp directory for one test.
pub fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-hosttest-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// The archive kind produced by [`build_archive`] on this platform.
pub fn platform_archive_kind() -> ArchiveKind {
    if cfg!(target_os = "windows") {
        ArchiveKind::Zip
    } else {
        ArchiveKind::TarGz
    }
}

/// Build a small archive (zip on Windows, tar.gz elsewhere) in memory.
pub fn build_archive(entries: &[(&str, &[u8])]) -> (Vec<u8>, String) {
    let bytes = build_archive_bytes(entries);
    let hash = sha256_hex(&bytes);
    (bytes, hash)
}

#[cfg(target_os = "windows")]
fn build_archive_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            writer.start_file(*name, options).expect("start entry");
            writer.write_all(body).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }
    buffer.into_inner()
}

#[cfg(not(target_os = "windows"))]
fn build_archive_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    {
        let mut tar = tar::Builder::new(&mut gz);
        for (name, body) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, name, *body).expect("append");
        }
        tar.finish().expect("finish tar");
    }
    gz.finish().expect("finish gz")
}

/// A one-shot HTTP server: it answers every request with the same body.
pub struct MockServer {
    port: u16,
    hits: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
}

impl MockServer {
    pub fn start(body: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                counter.fetch_add(1, Ordering::Relaxed);
                let mut buffer = [0u8; 2048];
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

    pub fn url(&self, name: &str) -> String {
        format!("http://127.0.0.1:{}/{}", self.port, name)
    }

    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        // The accept thread is parked in `accept()`; detaching is deliberate.
        let _ = self.handle.take();
    }
}

/// A compiler entry inside the extracted archive (xPack layout).
pub const COMPILER_ENTRY: &str = "xpack-riscv-none-elf-gcc-15.2.0-1/bin/riscv-none-elf-gcc.exe";

/// A download spec pointing at `server` with the given checksum.
pub fn spec_for(
    server: &MockServer,
    sha256: String,
    name: &str,
) -> host::toolchain_download::DownloadSpec {
    host::toolchain_download::DownloadSpec {
        version: host::toolchain_download::XPACK_RISCV_GCC_VERSION.to_string(),
        url: server.url(name),
        sha256,
        archive_kind: platform_archive_kind(),
        install_subdir: format!(
            "xpack-riscv-none-elf-gcc-{}",
            host::toolchain_download::XPACK_RISCV_GCC_VERSION
        ),
    }
}

/// Archive bytes for a valid, installable toolchain fixture.
pub fn toolchain_archive() -> (Vec<u8>, String) {
    build_archive(&[
        (COMPILER_ENTRY, b"#!/bin/sh\n# fake compiler\n"),
        (
            "xpack-riscv-none-elf-gcc-15.2.0-1/README.md",
            b"xpack fixture\n",
        ),
    ])
}

/// Bytes of a small, definitely-runnable executable.
///
/// Used as a fake `riscv-none-elf-gcc` so that the `--version` validation in
/// `set_toolchain_path` passes (`cmd.exe --version` exits 0 on Windows).
pub fn fake_executable_bytes() -> Vec<u8> {
    #[cfg(target_os = "windows")]
    {
        std::fs::read(r"C:\Windows\System32\cmd.exe").expect("read cmd.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::read("/bin/echo").expect("read /bin/echo")
    }
}

/// Archive bytes whose `riscv-none-elf-gcc` really runs (so adoption succeeds).
pub fn toolchain_archive_with_executable() -> (Vec<u8>, String) {
    let compiler = fake_executable_bytes();
    build_archive(&[
        (COMPILER_ENTRY, compiler.as_slice()),
        (
            "xpack-riscv-none-elf-gcc-15.2.0-1/README.md",
            b"xpack fixture\n",
        ),
    ])
}

/// Does `dir` exist and contain at least one file?
pub fn dir_has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().next().is_some())
        .unwrap_or(false)
}
