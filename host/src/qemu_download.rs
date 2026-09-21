//! One-click QEMU download (v0.4 #4).
//!
//! A parallel of `toolchain_download.rs`: a **pinned** version, a per-platform URL
//! and **SHA-256**, Zip-Slip-guarded extraction, cancellation, progress reported
//! per chunk, idempotent re-runs, and installation under the app-data directory.
//! The RISC-V toolchain module is untouched; the two are deliberately separate, so
//! following one cannot change the other.
//!
//! **The spec table is empty on purpose, and that is a decision, not an omission.**
//! The project decided to *guide* the user instead of downloading QEMU (v0.4 #4,
//! `docs/qemu-distribution.md` §5): a spec may only carry a URL and a digest somebody has actually
//! fetched and hashed, and the upstream release publishes no Windows binary to pin. Pinning a
//! third-party packager's installer would put that packager into our supply chain without saying so,
//! and a guessed digest is a silent integrity hole — this module has no "skip verification" switch.
//! So [`spec_for_current_platform`] refuses and hands back [`install_guidance`], and the manual path
//! in `docs/qemu-setup.md` is the way in.
//!
//! Everything below the spec table is written and exercised by tests against a loopback server
//! (`host/tests/qemu_download.rs`), so pinning a real release later would be a data change, not a
//! code change. Nothing calls it today.
//!
//! `QemuDownloadEvent::Failed` is emitted by the *caller* (it owns the error text);
//! this module emits Started / Progress / Verifying / Extracting / Done /
//! Cancelled.

use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

/// The QEMU release this project was verified against (`ENVIRONMENT.md`).
pub const QEMU_VERSION: &str = "11.1.0";

/// How the downloaded archive is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarGz,
}

/// Everything needed to fetch and install one QEMU build.
#[derive(Debug, Clone)]
pub struct QemuDownloadSpec {
    pub version: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the archive, as published by the vendor.
    pub sha256: String,
    pub archive_kind: ArchiveKind,
    /// Directory (under the install root) the archive is extracted into.
    pub install_subdir: String,
}

impl QemuDownloadSpec {
    /// File name of the archive (the last URL segment).
    pub fn file_name(&self) -> String {
        self.url
            .rsplit('/')
            .next()
            .unwrap_or("qemu-archive")
            .to_string()
    }
}

/// Progress / lifecycle events for the UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum QemuDownloadEvent {
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
pub enum QemuDownloadError {
    #[error(
        "no QEMU download is pinned for {platform}: RiscDom does not fetch QEMU {version} itself — \
         upstream publishes no Windows binary, and pinning a third-party packager's installer is a \
         supply-chain decision this project has not taken (docs/qemu-distribution.md §5). \
         Install it yourself: {hint}. RiscDom finds it afterwards."
    )]
    UnpinnedPlatform {
        platform: String,
        version: &'static str,
        hint: String,
    },
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
    #[error("no QEMU system emulator was found in the archive")]
    NoQemuInArchive,
}

impl QemuDownloadError {
    /// Stable code for the UI / audit detail.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnpinnedPlatform { .. } => "unpinned_platform",
            Self::Http(_) => "http",
            Self::Io(_) => "io",
            Self::ChecksumMismatch { .. } => "checksum_mismatch",
            Self::Cancelled => "cancelled",
            Self::UnsafeEntry(_) => "unsafe_entry",
            Self::Archive(_) => "archive",
            Self::NoQemuInArchive => "no_qemu_in_archive",
        }
    }
}

/// Is `winget` available to run? (Windows only — `false` everywhere else.)
///
/// `winget` is the install path this project guides users to when it exists; when it does not, the
/// official download page is offered instead.
///
/// The App Execution Alias is checked first because it is a cheap filesystem test: asking the
/// program itself spawns a process, and on a loaded machine that spawn can fail (observed while
/// writing this: two back-to-back `winget --version` calls disagreed). The spawn stays as the
/// fallback for installs that put `winget` somewhere else on `PATH`.
pub fn winget_available() -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        // Where the App Installer registers the alias for the current user.
        let alias = Path::new(&local)
            .join("Microsoft")
            .join("WindowsApps")
            .join("winget.exe");
        if alias.is_file() {
            return true;
        }
    }
    std::process::Command::new("winget")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// What to tell the user to do instead — the decision in `docs/qemu-distribution.md` §5 is to guide,
/// not to download.
///
/// The platform picks the route (winget/page on Windows, Homebrew on macOS, the
/// distribution's package on Linux); only the Windows branch needs the `winget`
/// probe, which cannot leave this crate. The wording itself lives in `sandbox`,
/// next to the constants it names.
pub fn install_guidance() -> String {
    sandbox::qemu_discover::install_hint_for(std::env::consts::OS, winget_available())
}

/// The download that matches this machine.
///
/// There is none today, for the reason in the module docs: this project guides users to a QEMU they
/// install themselves instead of downloading one (upstream publishes no Windows binary, and pinning
/// a third-party packager is a supply-chain decision — `docs/qemu-distribution.md` §5). The function
/// still exists so that the error is actionable and a future decision would be a table entry.
pub fn spec_for_current_platform() -> Result<QemuDownloadSpec, QemuDownloadError> {
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    Err(QemuDownloadError::UnpinnedPlatform {
        platform,
        version: QEMU_VERSION,
        hint: install_guidance(),
    })
}

/// Download, verify, extract and return the QEMU executable.
///
/// Idempotent: if the install directory already contains the emulator, nothing is
/// downloaded and the existing path is returned.
pub fn download_and_install(
    spec: &QemuDownloadSpec,
    dest_root: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(QemuDownloadEvent),
) -> Result<PathBuf, QemuDownloadError> {
    let install_dir = dest_root.join(&spec.version);
    if let Some(existing) = find_qemu(&install_dir) {
        on_event(QemuDownloadEvent::Done {
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
            on_event(QemuDownloadEvent::Done {
                install_path: path.clone(),
            });
            Ok(path)
        }
        Err(e) => {
            let _ = fs::remove_file(&archive);
            if matches!(e, QemuDownloadError::Cancelled) {
                on_event(QemuDownloadEvent::Cancelled);
            }
            Err(e)
        }
    }
}

fn download(
    spec: &QemuDownloadSpec,
    archive: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(QemuDownloadEvent),
) -> Result<(), QemuDownloadError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| QemuDownloadError::Http(e.to_string()))?;
    let mut response = client
        .get(&spec.url)
        .send()
        .map_err(|e| QemuDownloadError::Http(e.to_string()))?;
    if !response.status().is_success() {
        return Err(QemuDownloadError::Http(format!(
            "HTTP {} for {}",
            response.status(),
            spec.url
        )));
    }

    let total = response.content_length();
    on_event(QemuDownloadEvent::Started { total_bytes: total });

    let mut file = File::create(archive)?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(QemuDownloadError::Cancelled);
        }
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        downloaded += read as u64;
        on_event(QemuDownloadEvent::Progress { downloaded, total });
    }
    file.flush()?;
    Ok(())
}

fn verify(
    spec: &QemuDownloadSpec,
    archive: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(QemuDownloadEvent),
) -> Result<(), QemuDownloadError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(QemuDownloadError::Cancelled);
    }
    on_event(QemuDownloadEvent::Verifying);

    let mut file = File::open(archive)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(QemuDownloadError::Cancelled);
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
        return Err(QemuDownloadError::ChecksumMismatch { expected, actual });
    }
    Ok(())
}

fn extract(
    spec: &QemuDownloadSpec,
    archive: &Path,
    dest_root: &Path,
    install_dir: &Path,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(QemuDownloadEvent),
) -> Result<PathBuf, QemuDownloadError> {
    on_event(QemuDownloadEvent::Extracting);
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
        ArchiveKind::TarGz => Err(QemuDownloadError::Archive(
            "tar.gz archives are not supported on this platform".to_string(),
        )),
    };
    if let Err(e) = extracted {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let Some(found) = find_qemu(&staging) else {
        let _ = fs::remove_dir_all(&staging);
        return Err(QemuDownloadError::NoQemuInArchive);
    };
    let relative = found
        .strip_prefix(&staging)
        .map_err(|_| QemuDownloadError::NoQemuInArchive)?
        .to_path_buf();

    if install_dir.exists() {
        fs::remove_dir_all(install_dir)?;
    }
    fs::rename(&staging, install_dir)?;
    Ok(install_dir.join(relative))
}

/// Reject any entry whose *relative* path could escape the destination.
fn safe_relative(raw: &str, dest: &Path) -> Result<PathBuf, QemuDownloadError> {
    let rel = Path::new(raw);
    if rel.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(QemuDownloadError::UnsafeEntry(raw.to_string()));
    }
    Ok(dest.join(rel))
}

#[cfg(target_os = "windows")]
fn extract_zip(archive: &Path, dest: &Path, cancel: &AtomicBool) -> Result<(), QemuDownloadError> {
    let file = File::open(archive)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| QemuDownloadError::Archive(e.to_string()))?;
    for index in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(QemuDownloadError::Cancelled);
        }
        let mut entry = zip
            .by_index(index)
            .map_err(|e| QemuDownloadError::Archive(e.to_string()))?;
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
) -> Result<(), QemuDownloadError> {
    let file = File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.set_overwrite(true);
    let entries = tar
        .entries()
        .map_err(|e| QemuDownloadError::Archive(e.to_string()))?;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(QemuDownloadError::Cancelled);
        }
        let mut entry = entry.map_err(|e| QemuDownloadError::Archive(e.to_string()))?;
        let name = entry
            .path()
            .map_err(|e| QemuDownloadError::Archive(e.to_string()))?
            .to_string_lossy()
            .to_string();
        let out = safe_relative(&name, dest)?;
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        entry
            .unpack(&out)
            .map_err(|e| QemuDownloadError::Archive(e.to_string()))?;
    }
    Ok(())
}

/// Find the QEMU system emulator inside `dir` (bounded recursive scan).
///
/// The name comes from the sandbox, which owns discovery — the host does not keep
/// a second copy of it (v0.4 1e-followup).
fn find_qemu(dir: &Path) -> Option<PathBuf> {
    let wanted = sandbox::qemu_discover::exe_name();
    fn walk(dir: &Path, wanted: &str, depth: usize) -> Option<PathBuf> {
        if depth > 4 {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = walk(&path, wanted, depth + 1) {
                    return Some(found);
                }
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case(wanted))
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
        None
    }
    walk(dir, &wanted, 0)
}
