//! Shared helpers for the host integration tests.
//!
//! The download tests must never touch the network, so they run against a
//! loopback `TcpListener` serving a tiny archive built here.

#![allow(dead_code)]

use host_core::toolchain_download::ArchiveKind;
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
            // The name goes into the header by hand: `append_data` runs the tar crate's own path
            // check, which refuses `..` -- and one of these fixtures *is* an escaping entry. The
            // installer is what has to refuse it, not the fixture builder (`set_path` used to
            // panic here, so the test never reached the code it is about).
            let name_bytes = name.as_bytes();
            assert!(
                name_bytes.len() <= 100,
                "fixture name too long for a tar header: {name}"
            );
            header.as_old_mut().name[..name_bytes.len()].copy_from_slice(name_bytes);
            header.set_cksum();
            tar.append(&header, *body).expect("append");
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
) -> host_core::toolchain_download::DownloadSpec {
    host_core::toolchain_download::DownloadSpec {
        version: host_core::toolchain_download::XPACK_RISCV_GCC_VERSION.to_string(),
        url: server.url(name),
        sha256,
        archive_kind: platform_archive_kind(),
        toolchain: host_core::toolchain_download::Toolchain::C,
    }
}

/// The archive kind Zig ships on this platform (`.zip` on Windows, `.tar.xz` elsewhere).
pub fn zig_archive_kind() -> ArchiveKind {
    if cfg!(target_os = "windows") {
        ArchiveKind::Zip
    } else {
        ArchiveKind::TarXz
    }
}

/// The archive stem Zig uses here: `zig-<arch>-<os>-<version>`.
///
/// Built from the platform's own words, so the fixture cannot drift away from what
/// [`host_core::toolchain_download::zig_spec_for_current_platform`] names.
pub fn zig_stem() -> String {
    format!(
        "zig-{}-{}-{}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        host_core::toolchain_download::ZIG_VERSION
    )
}

/// Where the Zig executable sits inside an extracted archive.
///
/// Zig's release puts it at the **top level**, beside `lib/` -- not in a `bin/`.
pub fn zig_entry() -> String {
    let exe = if cfg!(target_os = "windows") {
        "zig.exe"
    } else {
        "zig"
    };
    format!("{}/{exe}", zig_stem())
}

/// Build the archive kind Zig ships here (`.zip` on Windows, `.tar.xz` elsewhere).
pub fn build_zig_archive(entries: &[(&str, &[u8])]) -> (Vec<u8>, String) {
    let bytes = if cfg!(target_os = "windows") {
        build_archive_bytes(entries)
    } else {
        build_tar_xz_bytes(entries)
    };
    let hash = sha256_hex(&bytes);
    (bytes, hash)
}

/// Archive bytes for a valid, installable Zig fixture (v0.9 F3a-download-apply).
pub fn zig_archive() -> (Vec<u8>, String) {
    let entry = zig_entry();
    let lib = format!("{}/lib/README.txt", zig_stem());
    build_zig_archive(&[
        (entry.as_str(), b"not a program\n".as_slice()),
        (lib.as_str(), b"zig fixture\n".as_slice()),
    ])
}

/// A download spec pointing at `server` for the Zig toolchain.
pub fn zig_spec_for(
    server: &MockServer,
    sha256: String,
    name: &str,
) -> host_core::toolchain_download::DownloadSpec {
    host_core::toolchain_download::DownloadSpec {
        version: host_core::toolchain_download::ZIG_VERSION.to_string(),
        url: server.url(name),
        sha256,
        archive_kind: zig_archive_kind(),
        toolchain: host_core::toolchain_download::Toolchain::Zig,
    }
}

/// The inner directory name of a `rust-std` component: `rust-std-<target>`.
///
/// Version-free on purpose: that is the name the locator looks for, while the version lives in
/// the *outer* directory (`rust-std-<version>-<target>/rust-std-<target>/`).
pub fn rust_std_dir_name() -> String {
    format!("rust-std-{}", agent::RUST_TARGET)
}

/// Build a `.tar.xz` with the `tar` crate's **own** path handling, long names included.
///
/// [`build_tar_xz_bytes`] writes the header name by hand so that it can express an escaping
/// entry; this one is for fixtures whose names are merely *long*. A real `rust-std` path runs
/// well past the 100-byte header field, and the crate then emits the same GNU long-name entry
/// the published archives carry.
fn build_tar_xz_paths(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
    {
        let mut tar = tar::Builder::new(&mut encoder);
        for (name, body) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            tar.append_data(&mut header, name, *body)
                .expect("append a named entry");
        }
        tar.finish().expect("finish tar");
    }
    encoder.finish().expect("finish xz")
}

/// Archive bytes for a valid `rust-std` fixture (v0.9 F3b-2).
///
/// The shape is the real one: a top directory, the sysroot inside it, and `lib/rustlib/<target>/
/// lib` below that — which is both what `find_rust_std` returns and what `set_rust_sysroot`
/// insists on before it adopts anything. Those paths are longer than a tar header's name field,
/// so this fixture goes through the crate's long-name support.
pub fn rust_std_archive() -> (Vec<u8>, String) {
    let target = agent::RUST_TARGET;
    let inner = format!("rust-std-1.98.1-{target}/{}", rust_std_dir_name());
    let libs = format!("{inner}/lib/rustlib/{target}/lib");
    let core = format!("{libs}/libcore-fixture.rlib");
    let copyright = format!("{inner}/COPYRIGHT");
    let bytes = build_tar_xz_paths(&[
        (core.as_str(), b"core\n".as_slice()),
        (copyright.as_str(), b"fixture\n".as_slice()),
    ]);
    let hash = sha256_hex(&bytes);
    (bytes, hash)
}

/// A download spec pointing at `server` for Rust's sysroot, pinned to `version`.
///
/// `version` is a parameter, not `RUST_VERSION`, because the host refuses a download whose pin
/// does not match this machine's `rustc` (decision §52) — a test that wants a download to happen
/// has to ask the machine what it is.
pub fn rust_spec_for(
    server: &MockServer,
    sha256: String,
    name: &str,
    version: &str,
) -> host_core::toolchain_download::DownloadSpec {
    host_core::toolchain_download::DownloadSpec {
        version: version.to_string(),
        url: server.url(name),
        sha256,
        archive_kind: ArchiveKind::TarXz,
        toolchain: host_core::toolchain_download::Toolchain::Rust,
    }
}

/// Archive bytes whose `zig` really runs, so adoption (`zig version`) succeeds.
pub fn zig_archive_with_executable() -> (Vec<u8>, String) {
    let zig = fake_executable_bytes();
    let entry = zig_entry();
    let lib = format!("{}/lib/README.txt", zig_stem());
    build_zig_archive(&[
        (entry.as_str(), zig.as_slice()),
        (lib.as_str(), b"zig fixture\n".as_slice()),
    ])
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

/// Build a `.tar.xz` in memory (v0.9 multi-language batch F3a-download).
///
/// No platform branch: unlike the zip/gzip pair above, xz is read on every platform.
/// The name goes into the header by hand for the same reason as in
/// [`build_archive_bytes`].
fn build_tar_xz_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
    {
        let mut tar = tar::Builder::new(&mut encoder);
        for (name, body) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            let name_bytes = name.as_bytes();
            assert!(
                name_bytes.len() <= 100,
                "fixture name too long for a tar header: {name}"
            );
            header.as_old_mut().name[..name_bytes.len()].copy_from_slice(name_bytes);
            header.set_cksum();
            tar.append(&header, *body).expect("append");
        }
        tar.finish().expect("finish tar");
    }
    encoder.finish().expect("finish xz")
}

/// Archive bytes for a `.tar.xz` carrying the compiler entry (v0.9 F3a-download).
///
/// The `.tar.xz` sibling of [`toolchain_archive`]: the same entries, xz instead of
/// zip/gzip, so the archive kind is the only thing that differs.
pub fn tar_xz_archive() -> (Vec<u8>, String) {
    let bytes = build_tar_xz_bytes(&[
        (COMPILER_ENTRY, b"#!/bin/sh\n# fake compiler\n".as_slice()),
        (
            "xpack-riscv-none-elf-gcc-15.2.0-1/README.md",
            b"xpack fixture\n".as_slice(),
        ),
    ]);
    let hash = sha256_hex(&bytes);
    (bytes, hash)
}

/// A download spec that serves `server` as a `.tar.xz` (v0.9 F3a-download).
pub fn spec_for_tar_xz(
    server: &MockServer,
    sha256: String,
    name: &str,
) -> host_core::toolchain_download::DownloadSpec {
    host_core::toolchain_download::DownloadSpec {
        version: host_core::toolchain_download::XPACK_RISCV_GCC_VERSION.to_string(),
        url: server.url(name),
        sha256,
        archive_kind: ArchiveKind::TarXz,
        toolchain: host_core::toolchain_download::Toolchain::C,
    }
}

/// Where the QEMU emulator sits inside an extracted archive.
///
/// The name comes from the sandbox (it owns QEMU discovery), so a fixture archive
/// cannot drift away from what [`host_core::qemu_download`] looks for.
pub fn qemu_entry() -> String {
    format!("qemu/bin/{}", sandbox::qemu_discover::exe_name())
}

/// The archive kind the QEMU downloader expects on this platform.
///
/// Separate from [`platform_archive_kind`] because the QEMU downloader is a
/// self-contained parallel of the toolchain one, with its own types (v0.4 #4).
pub fn qemu_archive_kind() -> host_core::qemu_download::ArchiveKind {
    if cfg!(target_os = "windows") {
        host_core::qemu_download::ArchiveKind::Zip
    } else {
        host_core::qemu_download::ArchiveKind::TarGz
    }
}

/// A download spec pointing at `server` with the given checksum (QEMU, v0.4 #4).
pub fn qemu_spec_for(
    server: &MockServer,
    sha256: String,
    name: &str,
) -> host_core::qemu_download::QemuDownloadSpec {
    host_core::qemu_download::QemuDownloadSpec {
        version: host_core::qemu_download::QEMU_VERSION.to_string(),
        url: server.url(name),
        sha256,
        archive_kind: qemu_archive_kind(),
    }
}

/// Archive bytes for a valid, installable QEMU fixture whose emulator does **not** run.
///
/// The adoption step is what refuses it (`set_qemu_path` runs `<path> --version`). The body is
/// deliberately not a `#!` script: with mode 0755 that *would* run on Unix, so the fixture would
/// stop testing what it is for (it passed on Windows only because a text `.exe` never runs).
pub fn qemu_archive() -> (Vec<u8>, String) {
    let entry = qemu_entry();
    build_archive(&[
        (entry.as_str(), b"not a program\n".as_slice()),
        ("qemu/README.txt", b"qemu fixture\n".as_slice()),
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

/// Archive bytes whose QEMU emulator really runs (so adoption succeeds).
///
/// The QEMU counterpart of [`toolchain_archive_with_executable`]: the same fake
/// executable, under the name the QEMU downloader looks for.
pub fn qemu_archive_with_executable() -> (Vec<u8>, String) {
    let emulator = fake_executable_bytes();
    let entry = qemu_entry();
    build_archive(&[
        (entry.as_str(), emulator.as_slice()),
        ("qemu/README.txt", b"qemu fixture\n".as_slice()),
    ])
}

/// Does `dir` exist and contain at least one file?
pub fn dir_has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().next().is_some())
        .unwrap_or(false)
}
