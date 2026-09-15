//! One-click RISC-V toolchain download (v0.3 #3a).
//!
//! Downloads an official xPack RISC-V GCC archive, verifies its **SHA-256**
//! against the value published in the release's `.sha` file, extracts it under
//! the app-data directory and returns the compiler executable.
//!
//! Rules baked into this module:
//!
//! - the download only ever happens when a caller asks for it (no background or
//!   startup downloads);
//! - the checksum is mandatory — there is no "skip verification" switch;
//! - progress is reported per chunk and cancellation is polled per chunk (and
//!   per extracted entry);
//! - archive entries may not escape the install directory (Zip Slip guard);
//! - everything lands under the given `dest_root` (never in the repository).
//!
//! `DownloadEvent::Failed` is emitted by the *caller* (it owns the error text);
//! this module emits Started / Progress / Verifying / Extracting / Done /
//! Cancelled.

use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

/// Version of the xPack RISC-V GCC we offer for download.
pub const XPACK_RISCV_GCC_VERSION: &str = "15.2.0-1";

/// Base URL of the xPack release assets.
pub const XPACK_RELEASE_BASE: &str =
    "https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases/download";

/// How the downloaded archive is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarGz,
}

/// Everything needed to fetch and install one toolchain build.
#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub version: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the archive, as published by the vendor.
    pub sha256: String,
    pub archive_kind: ArchiveKind,
    /// Directory (under the install root) the archive is extracted into.
    pub install_subdir: String,
}

impl DownloadSpec {
    /// File name of the archive (the last URL segment).
    pub fn file_name(&self) -> String {
        self.url
            .rsplit('/')
            .next()
            .unwrap_or("toolchain-archive")
            .to_string()
    }
}

/// Progress / lifecycle events for the UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DownloadEvent {
    Started { total_bytes: Option<u64> },
    Progress { downloaded: u64, total: Option<u64> },
    Verifying,
    Extracting,
    Done { install_path: PathBuf },
    Failed { reason: String },
    Cancelled,
}

/// Failures from [`download_and_install`].
#[derive(Debug, Error)]
pub enum ToolchainDownloadError {
    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),
    #[error("no download is known for asset {0}")]
    UnknownAsset(String),
    #[error("download failed: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("cancelled")]
    Cancelled,
    #[error("archive entry escapes the install directory: {0}")]
    UnsafeEntry(String),
    #[error("archive error: {0}")]
    Archive(String),
    #[error("no RISC-V compiler was found in the archive")]
    NoCompilerInArchive,
}

impl ToolchainDownloadError {
    /// Stable code for the UI / audit detail.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedPlatform(_) => "unsupported_platform",
            Self::UnknownAsset(_) => "unknown_asset",
            Self::Http(_) => "http",
            Self::Io(_) => "io",
            Self::ChecksumMismatch { .. } => "checksum_mismatch",
            Self::Cancelled => "cancelled",
            Self::UnsafeEntry(_) => "unsafe_entry",
            Self::Archive(_) => "archive",
            Self::NoCompilerInArchive => "no_compiler_in_archive",
        }
    }
}

/// Executable names we accept inside an extracted archive.
const GCC_NAMES: [&str; 2] = ["riscv-none-elf-gcc", "riscv64-unknown-elf-gcc"];

/// Official SHA-256 values, taken from the `<asset>.sha` files published next to
/// the archives in the v15.2.0-1 release (and matching GitHub's own asset
/// digests).
const SHA256_WIN32_X64: &str = "85ef714dacd273b1dadf4af4892774520ac01915bfa6da816a56e7e41591e09e";
const SHA256_DARWIN_X64: &str = "98e83f097b10163869dabffd58389ac8e4eb41bae0f67124569158655be593ea";
const SHA256_DARWIN_ARM64: &str =
    "6588e8351455fad8aca37551f0e5a5543f3346bfa9a837cf03cbd3bdd4989f8f";
const SHA256_LINUX_X64: &str = "aaaa8060c914851a3e5ee1ba82cc3d6f80972f90638a05c6e823a37557a33758";
const SHA256_LINUX_ARM64: &str = "4e60e2a54c16385e4e2476d08240f857495d5a61609d97e1ee49f72875a6ec1e";

/// `(asset file name, archive kind)` for the current OS/arch.
fn platform_asset_owned() -> Result<(String, ArchiveKind), ToolchainDownloadError> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let v = XPACK_RISCV_GCC_VERSION;
    let asset = |suffix: &str| format!("xpack-riscv-none-elf-gcc-{v}-{suffix}");
    match (os, arch) {
        ("windows", "x86_64") => Ok((asset("win32-x64.zip"), ArchiveKind::Zip)),
        ("macos", "x86_64") => Ok((asset("darwin-x64.tar.gz"), ArchiveKind::TarGz)),
        ("macos", "aarch64") => Ok((asset("darwin-arm64.tar.gz"), ArchiveKind::TarGz)),
        ("linux", "x86_64") => Ok((asset("linux-x64.tar.gz"), ArchiveKind::TarGz)),
        ("linux", "aarch64") => Ok((asset("linux-arm64.tar.gz"), ArchiveKind::TarGz)),
        _ => Err(ToolchainDownloadError::UnsupportedPlatform(format!(
            "{os}-{arch}"
        ))),
    }
}

/// Official checksum for an asset file name.
fn sha256_for_asset(asset: &str) -> Result<&'static str, ToolchainDownloadError> {
    if asset.ends_with("win32-x64.zip") {
        Ok(SHA256_WIN32_X64)
    } else if asset.ends_with("darwin-x64.tar.gz") {
        Ok(SHA256_DARWIN_X64)
    } else if asset.ends_with("darwin-arm64.tar.gz") {
        Ok(SHA256_DARWIN_ARM64)
    } else if asset.ends_with("linux-x64.tar.gz") {
        Ok(SHA256_LINUX_X64)
    } else if asset.ends_with("linux-arm64.tar.gz") {
        Ok(SHA256_LINUX_ARM64)
    } else {
        Err(ToolchainDownloadError::UnknownAsset(asset.to_string()))
    }
}

/// The download that matches this machine.
pub fn spec_for_current_platform() -> Result<DownloadSpec, ToolchainDownloadError> {
    let (asset, archive_kind) = platform_asset_owned()?;
    let sha256 = sha256_for_asset(&asset)?;
    let version = XPACK_RISCV_GCC_VERSION.to_string();
    Ok(DownloadSpec {
        url: format!("{XPACK_RELEASE_BASE}/v{version}/{asset}"),
        version,
        sha256: sha256.to_string(),
        archive_kind,
        install_subdir: format!("xpack-riscv-none-elf-gcc-{XPACK_RISCV_GCC_VERSION}"),
    })
}

/// Download, verify, extract and return the compiler executable.
///
/// Idempotent: if the install directory already contains a compiler, nothing is
/// downloaded and the existing path is returned.
pub fn download_and_install(
    spec: &DownloadSpec,
    dest_root: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(DownloadEvent),
) -> Result<PathBuf, ToolchainDownloadError> {
    let install_dir = dest_root.join(&spec.version);
    if let Some(existing) = find_compiler(&install_dir) {
        on_event(DownloadEvent::Done {
            install_path: existing.clone(),
        });
        return Ok(existing);
    }

    let tmp_dir = dest_root.join(".download-tmp");
    fs::create_dir_all(&tmp_dir)?;
    let archive = tmp_dir.join(spec.file_name());

    let result = download(spec, &archive, cancel, on_event)
        .and_then(|()| verify(spec, &archive, cancel, on_event))
        .and_then(|()| extract(spec, &archive, dest_root, &install_dir, cancel, on_event));

    match result {
        Ok(path) => {
            let _ = fs::remove_file(&archive);
            on_event(DownloadEvent::Done {
                install_path: path.clone(),
            });
            Ok(path)
        }
        Err(e) => {
            let _ = fs::remove_file(&archive);
            if matches!(e, ToolchainDownloadError::Cancelled) {
                on_event(DownloadEvent::Cancelled);
            }
            Err(e)
        }
    }
}

fn download(
    spec: &DownloadSpec,
    archive: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(DownloadEvent),
) -> Result<(), ToolchainDownloadError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| ToolchainDownloadError::Http(e.to_string()))?;
    let mut response = client
        .get(&spec.url)
        .send()
        .map_err(|e| ToolchainDownloadError::Http(e.to_string()))?;
    if !response.status().is_success() {
        return Err(ToolchainDownloadError::Http(format!(
            "HTTP {} for {}",
            response.status(),
            spec.url
        )));
    }

    let total = response.content_length();
    on_event(DownloadEvent::Started { total_bytes: total });

    let mut file = File::create(archive)?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ToolchainDownloadError::Cancelled);
        }
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        downloaded += read as u64;
        on_event(DownloadEvent::Progress { downloaded, total });
    }
    file.flush()?;
    Ok(())
}

fn verify(
    spec: &DownloadSpec,
    archive: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(DownloadEvent),
) -> Result<(), ToolchainDownloadError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(ToolchainDownloadError::Cancelled);
    }
    on_event(DownloadEvent::Verifying);

    let mut file = File::open(archive)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(ToolchainDownloadError::Cancelled);
        }
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    let expected = spec.sha256.to_lowercase();
    if actual != expected {
        return Err(ToolchainDownloadError::ChecksumMismatch { expected, actual });
    }
    Ok(())
}

fn extract(
    spec: &DownloadSpec,
    archive: &Path,
    dest_root: &Path,
    install_dir: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(DownloadEvent),
) -> Result<PathBuf, ToolchainDownloadError> {
    on_event(DownloadEvent::Extracting);
    let staging = dest_root.join(".extract-tmp");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let extracted = match spec.archive_kind {
        ArchiveKind::Zip => extract_zip(archive, &staging, cancel),
        #[cfg(not(target_os = "windows"))]
        ArchiveKind::TarGz => extract_tar_gz(archive, &staging, cancel),
        #[cfg(target_os = "windows")]
        ArchiveKind::TarGz => Err(ToolchainDownloadError::Archive(
            "tar.gz archives are not supported on this platform".to_string(),
        )),
    };
    if let Err(e) = extracted {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let Some(found) = find_compiler(&staging) else {
        let _ = fs::remove_dir_all(&staging);
        return Err(ToolchainDownloadError::NoCompilerInArchive);
    };
    let relative = found
        .strip_prefix(&staging)
        .map_err(|_| ToolchainDownloadError::NoCompilerInArchive)?
        .to_path_buf();

    if install_dir.exists() {
        fs::remove_dir_all(install_dir)?;
    }
    fs::rename(&staging, install_dir)?;
    Ok(install_dir.join(relative))
}

/// Reject any entry whose *relative* path could escape the destination.
fn safe_relative(raw: &str, dest: &Path) -> Result<PathBuf, ToolchainDownloadError> {
    let rel = Path::new(raw);
    if rel.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ToolchainDownloadError::UnsafeEntry(raw.to_string()));
    }
    Ok(dest.join(rel))
}

#[cfg(target_os = "windows")]
fn extract_zip(
    archive: &Path,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<(), ToolchainDownloadError> {
    let file = File::open(archive)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| ToolchainDownloadError::Archive(e.to_string()))?;
    for index in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ToolchainDownloadError::Cancelled);
        }
        let mut entry = zip
            .by_index(index)
            .map_err(|e| ToolchainDownloadError::Archive(e.to_string()))?;
        let name = entry.name().to_string();
        let out = safe_relative(&name, dest)?;
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut target = File::create(&out)?;
        std::io::copy(&mut entry, &mut target)?;
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn extract_tar_gz(
    archive: &Path,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<(), ToolchainDownloadError> {
    let file = File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.set_overwrite(true);
    let entries = tar
        .entries()
        .map_err(|e| ToolchainDownloadError::Archive(e.to_string()))?;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(ToolchainDownloadError::Cancelled);
        }
        let mut entry = entry.map_err(|e| ToolchainDownloadError::Archive(e.to_string()))?;
        let name = entry
            .path()
            .map_err(|e| ToolchainDownloadError::Archive(e.to_string()))?
            .to_string_lossy()
            .to_string();
        let out = safe_relative(&name, dest)?;
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        entry
            .unpack(&out)
            .map_err(|e| ToolchainDownloadError::Archive(e.to_string()))?;
    }
    Ok(())
}

/// Find a RISC-V compiler inside `dir` (bounded recursive scan).
fn find_compiler(dir: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 4 {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = walk(&path, depth + 1) {
                    return Some(found);
                }
            } else if is_compiler_name(&path) {
                return Some(path);
            }
        }
        None
    }
    walk(dir, 0)
}

fn is_compiler_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let stem = name.strip_suffix(".exe").unwrap_or(name);
    GCC_NAMES.contains(&stem)
}
