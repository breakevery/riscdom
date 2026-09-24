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

/// Which toolchain a download installs (v0.9 multi-language batch F3a-download-apply).
///
/// `serde` because it travels: the body that starts a download carries it, and the status a UI
/// polls reports it back. The labels are the ones every edge accepts (`"c"` / `"zig"` /
/// `"rust"`), and [`Toolchain::parse`] is the single place that decides what is acceptable — the
/// Tauri command, the HTTP body and the CLI all route through it, so a new label reaches every
/// edge at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Toolchain {
    /// The xPack RISC-V GCC: what the C path compiles with. The default.
    C,
    /// The Zig compiler (v0.9 F3a): the sandbox's second language.
    Zig,
    /// Rust (v0.9 F3b-2): the **sysroot** (`rust-std`), not the compiler — `rustc` comes from the
    /// machine, which is F3b's first ruling. This is the one arm whose product is a directory.
    Rust,
}

impl Toolchain {
    /// Every label this type accepts, in the order the error messages list them.
    pub const LABELS: [&str; 3] = ["c", "zig", "rust"];

    /// The label this toolchain travels as.
    pub fn label(self) -> &'static str {
        match self {
            Toolchain::C => "c",
            Toolchain::Zig => "zig",
            Toolchain::Rust => "rust",
        }
    }

    /// Parse a label. Absent or empty is [`Toolchain::C`], so a caller that sends no language
    /// keeps downloading exactly what it always did.
    pub fn parse(label: Option<&str>) -> Result<Self, String> {
        match label.map(str::trim).filter(|l| !l.is_empty()) {
            None | Some("c") => Ok(Toolchain::C),
            Some("zig") => Ok(Toolchain::Zig),
            Some("rust") => Ok(Toolchain::Rust),
            Some(other) => Err(format!(
                "unknown toolchain {other:?}: expected one of {}",
                Toolchain::LABELS.join(", ")
            )),
        }
    }
}

/// How the downloaded archive is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    TarGz,
    /// A `.tar.xz` (v0.9 multi-language batch F3a-download).
    ///
    /// Zig's macOS/Linux releases and Rust's `rust-std-*.tar.xz` ship as xz-compressed
    /// tarballs. It is a variant of this enum and not an online shape: [`ArchiveKind`]
    /// carries no serde, so adding it changes no wire type.
    TarXz,
}

/// Everything needed to fetch and install one toolchain build.
#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub version: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the archive, as published by the vendor.
    pub sha256: String,
    pub archive_kind: ArchiveKind,
    /// Which toolchain this is (v0.9 F3a-download-apply). It decides **two** things the rest of
    /// the module cannot guess: which locator finds the product inside the archive, and which
    /// "adopt" call the host makes once it is installed.
    pub toolchain: Toolchain,
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
#[serde(tag = "state", rename_all = "kebab-case")]
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
        toolchain: Toolchain::C,
        install_subdir: format!("xpack-riscv-none-elf-gcc-{XPACK_RISCV_GCC_VERSION}"),
    })
}

/// Version of the Zig release we offer for download (v0.9 F3a-download-apply).
///
/// Pinned exactly like the xPack one: the checksums below belong to this release and to no
/// other, so the two move together.
pub const ZIG_VERSION: &str = "0.16.0";

/// Base URL of the Zig release assets (the manifest lives at `<base>/index.json`).
pub const ZIG_RELEASE_BASE: &str = "https://ziglang.org/download";

/// Official SHA-256 values for Zig 0.16.0, taken from the `shasum` field of
/// `https://ziglang.org/download/index.json`.
///
/// Zig publishes no per-asset `.sha` file the way xPack does; the manifest **is** the source.
/// They are hardcoded for the same reason the xPack ones are: a download must not need the
/// network (or a build script) just to learn what it is about to verify.
const SHA256_ZIG_WIN32_X64: &str =
    "68659eb5f1e4eb1437a722f1dd889c5a322c9954607f5edcf337bc3684a75a7e";
const SHA256_ZIG_DARWIN_X64: &str =
    "0387557ed1877bc6a2e1802c8391953baddba76081876301c522f52977b52ba7";
const SHA256_ZIG_DARWIN_ARM64: &str =
    "b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489";
const SHA256_ZIG_LINUX_X64: &str =
    "70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00";
const SHA256_ZIG_LINUX_ARM64: &str =
    "ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17";

/// `(asset file name, archive kind)` for the current OS/arch, Zig's names.
///
/// Separate from [`platform_asset_owned`] because the names, the archive kinds and the release
/// host all differ; the **shape** (five `(os, arch)` arms and an honest
/// [`ToolchainDownloadError::UnsupportedPlatform`]) is deliberately the same one. Taking
/// `os`/`arch` as arguments is what lets the unit test cover all five arms on one machine.
fn zig_asset_for(os: &str, arch: &str) -> Result<(String, ArchiveKind), ToolchainDownloadError> {
    let v = ZIG_VERSION;
    let asset = |arch: &str, os: &str, ext: &str| format!("zig-{arch}-{os}-{v}.{ext}");
    match (os, arch) {
        ("windows", "x86_64") => Ok((asset("x86_64", "windows", "zip"), ArchiveKind::Zip)),
        ("macos", "x86_64") => Ok((asset("x86_64", "macos", "tar.xz"), ArchiveKind::TarXz)),
        ("macos", "aarch64") => Ok((asset("aarch64", "macos", "tar.xz"), ArchiveKind::TarXz)),
        ("linux", "x86_64") => Ok((asset("x86_64", "linux", "tar.xz"), ArchiveKind::TarXz)),
        ("linux", "aarch64") => Ok((asset("aarch64", "linux", "tar.xz"), ArchiveKind::TarXz)),
        _ => Err(ToolchainDownloadError::UnsupportedPlatform(format!(
            "{os}-{arch}"
        ))),
    }
}

/// The pinned download for this machine, for one toolchain (v0.9 F3a-download-apply).
///
/// The single place the three edges (the Tauri command, the HTTP route, the CLI) agree on how a
/// language becomes a spec, so none of them repeats the match.
pub fn spec_for_toolchain(toolchain: Toolchain) -> Result<DownloadSpec, ToolchainDownloadError> {
    match toolchain {
        Toolchain::C => spec_for_current_platform(),
        Toolchain::Zig => zig_spec_for_current_platform(),
        Toolchain::Rust => rust_spec_for_current_platform(),
    }
}

/// Official checksum for a Zig asset file name.
fn sha256_for_zig_asset(asset: &str) -> Result<&'static str, ToolchainDownloadError> {
    let v = ZIG_VERSION;
    if asset == format!("zig-x86_64-windows-{v}.zip") {
        Ok(SHA256_ZIG_WIN32_X64)
    } else if asset == format!("zig-x86_64-macos-{v}.tar.xz") {
        Ok(SHA256_ZIG_DARWIN_X64)
    } else if asset == format!("zig-aarch64-macos-{v}.tar.xz") {
        Ok(SHA256_ZIG_DARWIN_ARM64)
    } else if asset == format!("zig-x86_64-linux-{v}.tar.xz") {
        Ok(SHA256_ZIG_LINUX_X64)
    } else if asset == format!("zig-aarch64-linux-{v}.tar.xz") {
        Ok(SHA256_ZIG_LINUX_ARM64)
    } else {
        Err(ToolchainDownloadError::UnknownAsset(asset.to_string()))
    }
}

/// Version of the `rust-std` component we offer for download (v0.9 F3b-2).
///
/// **The hard constraint**: a sysroot carries metadata `rustc` compares against its own, so it is
/// only usable by the release that produced it. The host therefore refuses a Rust download when
/// the machine's `rustc` reports a different release (decision §52).
pub const RUST_VERSION: &str = "1.98.1";

/// Base URL of the Rust release components.
///
/// The dated directory form (`<base>/<date>/…`) does not exist for this asset; the unversioned
/// directory is the one that serves it.
pub const RUST_RELEASE_BASE: &str = "https://static.rust-lang.org/dist";

/// Official SHA-256 of `rust-std-1.98.1-riscv64gc-unknown-none-elf.tar.xz`, from the `.sha256`
/// file published beside it (12,508,136 bytes).
///
/// Unlike Zig, Rust publishes that per-asset file — the value is hardcoded here for the same
/// reason as the others: a download must not need the network to learn what it is about to verify.
const SHA256_RUST_STD_RISCV64GC: &str =
    "32ff80918e1adff90f1ac4ccc7f53ff65b709616b0c08630ca29e0c1187cb870";

/// The one `rust-std` asset: `rust-std-<version>-<target>.tar.xz`.
fn rust_asset() -> String {
    format!("rust-std-{RUST_VERSION}-{}.tar.xz", agent::RUST_TARGET)
}

/// Official checksum for the `rust-std` asset (a single arm: there is one asset).
fn sha256_for_rust_asset(asset: &str) -> Result<&'static str, ToolchainDownloadError> {
    if asset == rust_asset() {
        Ok(SHA256_RUST_STD_RISCV64GC)
    } else {
        Err(ToolchainDownloadError::UnknownAsset(asset.to_string()))
    }
}

/// The pinned Rust sysroot download (v0.9 F3b-2).
///
/// **No `(os, arch)` branch**: a `rust-std` component is for a *target*, not a host, so one asset
/// serves every platform — `rustc` itself comes from the machine (F3b's first ruling).
pub fn rust_spec_for_current_platform() -> Result<DownloadSpec, ToolchainDownloadError> {
    let asset = rust_asset();
    let sha256 = sha256_for_rust_asset(&asset)?;
    let version = RUST_VERSION.to_string();
    let stem = asset.strip_suffix(".tar.xz").unwrap_or(&asset).to_string();
    Ok(DownloadSpec {
        url: format!("{RUST_RELEASE_BASE}/{asset}"),
        version,
        sha256: sha256.to_string(),
        archive_kind: ArchiveKind::TarXz,
        toolchain: Toolchain::Rust,
        install_subdir: stem,
    })
}

/// The Zig download that matches this machine (v0.9 F3a-download-apply).
///
/// A sibling of [`spec_for_current_platform`] rather than a branch inside it: the two
/// releases share a shape and nothing else, and keeping them apart is what leaves the C spec
/// (and its test) untouched.
pub fn zig_spec_for_current_platform() -> Result<DownloadSpec, ToolchainDownloadError> {
    let (asset, archive_kind) = zig_asset_for(std::env::consts::OS, std::env::consts::ARCH)?;
    let sha256 = sha256_for_zig_asset(&asset)?;
    let version = ZIG_VERSION.to_string();
    // The field is dead this batch (decision §49); naming the archive's own top directory is
    // the least surprising thing to leave in it.
    let stem = asset
        .strip_suffix(".tar.xz")
        .or_else(|| asset.strip_suffix(".zip"))
        .unwrap_or(&asset)
        .to_string();
    Ok(DownloadSpec {
        url: format!("{ZIG_RELEASE_BASE}/{version}/{asset}"),
        version,
        sha256: sha256.to_string(),
        archive_kind,
        toolchain: Toolchain::Zig,
        install_subdir: stem,
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
    if let Some(existing) = product_locator(spec.toolchain)(&install_dir) {
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
        // Unlike `.tar.gz` -- which is only ever a unix *asset* here -- a `.tar.xz` is the
        // shape of the host's own Zig and Rust downloads on every platform, so this arm
        // carries no platform gate and a Windows host can read one too.
        ArchiveKind::TarXz => extract_tar_xz(archive, &staging, cancel),
    };
    if let Err(e) = extracted {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let Some(found) = product_locator(spec.toolchain)(&staging) else {
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

/// Zip archives are a Windows asset kind here, and `zip` is a Windows-only
/// dependency: on every other platform the match arm still exists, so it reports
/// that instead of failing to compile.
#[cfg(not(target_os = "windows"))]
fn extract_zip(
    _archive: &Path,
    _dest: &Path,
    _cancel: &AtomicBool,
) -> Result<(), ToolchainDownloadError> {
    Err(ToolchainDownloadError::Archive(
        "zip archives are not supported on this platform".to_string(),
    ))
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

/// Unpack a `.tar.xz`.
///
/// The mirror of [`extract_tar_gz`] with an xz decoder: same Zip-Slip guard
/// ([`safe_relative`]), same `set_overwrite(true)`, same per-entry cancellation.
fn extract_tar_xz(
    archive: &Path,
    dest: &Path,
    cancel: &AtomicBool,
) -> Result<(), ToolchainDownloadError> {
    let file = File::open(archive)?;
    let decoder = xz2::read::XzDecoder::new(file);
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

/// The locator that finds a spec's product inside a directory (v0.9 F3a-download-apply).
///
/// One function per language, as §49 decides: the C toolchain's scan is the wide, recursive
/// one the sandbox registry also uses, while a Zig release is shallow by construction. The
/// C arm names exactly the function the code used before this batch, so the C path does not
/// move.
fn product_locator(toolchain: Toolchain) -> fn(&Path) -> Option<PathBuf> {
    match toolchain {
        Toolchain::C => find_compiler,
        Toolchain::Zig => find_zig,
        Toolchain::Rust => find_rust_std,
    }
}

/// Find the Rust sysroot inside `dir` (v0.9 F3b-2).
///
/// Unlike the C and Zig arms this returns a **directory**: what Rust needs from us is the target's
/// `core`, and a `rust-std-<target>/` tree is what carries it. The component nests it one level
/// down (`rust-std-<version>-<target>/rust-std-<target>/`) and the **inner** name carries no
/// version — that is the name this looks for, at depth 0 (a sysroot somebody unpacked by hand) and
/// at depth 1 (the archive as it comes out). Both are directories, so the locator's `PathBuf`
/// contract is unchanged; only what the path *means* differs, and the adopt call
/// (`set_rust_sysroot`) is the one that knows it.
pub(crate) fn find_rust_std(dir: &Path) -> Option<PathBuf> {
    let name = format!("rust-std-{}", agent::RUST_TARGET);

    // Depth 0: the sysroot sits in `dir` itself.
    let direct = dir.join(&name);
    if direct.is_dir() {
        return Some(direct);
    }

    // Depth 1: the component's own top directory, whose name carries the version.
    let mut subdirs: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    subdirs.sort();
    for subdir in subdirs {
        let candidate = subdir.join(&name);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Find the Zig executable inside `dir` (v0.9 F3a-download-apply).
///
/// Zig's release is shallow: the archive's top-level directory holds the binary **beside**
/// `lib/` (`zig-x86_64-linux-0.16.0/zig`), so depth 0 covers an extracted archive and depth 1
/// covers the same archive once it sits inside a parent directory. The names come from the
/// agent, which already owns them (`agent::ZIG_NAMES`); the `.exe` suffix is the platform's.
pub(crate) fn find_zig(dir: &Path) -> Option<PathBuf> {
    let names: Vec<String> = agent::ZIG_NAMES
        .iter()
        .flat_map(|name| [(*name).to_string(), format!("{name}.exe")])
        .collect();

    // Depth 0: the binary sits in `dir` itself (`dir/zig`, `dir/zig.exe`).
    for name in &names {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // Depth 1: the archive's own top directory. Its name carries the version, so it is not
    // written down anywhere -- the scan reads it.
    let mut subdirs: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    subdirs.sort();
    for subdir in subdirs {
        for name in &names {
            let candidate = subdir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Find a RISC-V compiler inside `dir` (bounded recursive scan).
///
/// `pub(crate)` since v0.9 F2a: the sandbox registry scans `<data-dir>/toolchain`
/// through this rather than growing a second copy of the scan (or of the names,
/// which come from `agent::GCC_NAMES`).
pub(crate) fn find_compiler(dir: &Path) -> Option<PathBuf> {
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

/// Does `path` name one of the RISC-V GCC executables?
///
/// The list comes from the agent (the crate that discovers and runs the compiler),
/// so the host does not keep a second copy of the names (v0.4 1e-followup).
pub fn is_compiler_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let stem = name.strip_suffix(".exe").unwrap_or(name);
    agent::GCC_NAMES.contains(&stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `.tar.xz` in memory (v0.9 multi-language F3a-download).
    ///
    /// The name is copied into the header by hand, exactly as `host-core/tests/common`
    /// does it: `tar`'s own `append_data` refuses `..`, and one fixture here *is* an
    /// escaping entry — the extractor is what has to refuse it, not the builder.
    fn tar_xz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
        {
            let mut tar = tar::Builder::new(&mut encoder);
            for (name, body) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(body.len() as u64);
                header.set_mode(0o755);
                let bytes = name.as_bytes();
                assert!(bytes.len() <= 100, "fixture name too long: {name}");
                header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
                header.set_cksum();
                tar.append(&header, *body).expect("append");
            }
            tar.finish().expect("finish tar");
        }
        encoder.finish().expect("finish xz")
    }

    /// Write `bytes` to a fresh scratch directory and return `(archive, dest)`.
    fn scratch(bytes: &[u8], tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("riscdom-tarxz-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch");
        let archive = dir.join("fixture.tar.xz");
        fs::write(&archive, bytes).expect("write archive");
        let dest = dir.join("out");
        fs::create_dir_all(&dest).expect("create dest");
        (archive, dest)
    }

    #[test]
    fn a_tar_xz_unpacks_its_entries() {
        let (archive, dest) = scratch(&tar_xz(&[("pkg/hello.txt", b"hello xz\n")]), "happy");
        extract_tar_xz(&archive, &dest, &AtomicBool::new(false)).expect("extract");
        let written = fs::read_to_string(dest.join("pkg").join("hello.txt")).expect("read");
        assert_eq!(written, "hello xz\n");
    }

    #[test]
    fn an_escaping_entry_in_a_tar_xz_is_refused() {
        let bytes = tar_xz(&[("pkg/ok.txt", b"ok\n"), ("../escaped.txt", b"nope\n")]);
        let (archive, dest) = scratch(&bytes, "slip");
        let err = extract_tar_xz(&archive, &dest, &AtomicBool::new(false))
            .expect_err("an escaping entry must be refused");
        assert!(
            matches!(err, ToolchainDownloadError::UnsafeEntry(_)),
            "{err}"
        );
        assert_eq!(err.code(), "unsafe_entry");
        // Nothing may land beside the destination directory.
        assert!(!dest.parent().expect("parent").join("escaped.txt").exists());
    }

    #[test]
    fn an_existing_file_is_overwritten_by_the_archive() {
        let (archive, dest) = scratch(&tar_xz(&[("hello.txt", b"new\n")]), "overwrite");
        let target = dest.join("hello.txt");
        fs::write(&target, b"old\n").expect("seed");
        extract_tar_xz(&archive, &dest, &AtomicBool::new(false)).expect("extract");
        assert_eq!(fs::read_to_string(&target).expect("read"), "new\n");
    }

    #[test]
    fn a_cancelled_extract_stops_and_reports() {
        let (archive, dest) = scratch(&tar_xz(&[("hello.txt", b"hi\n")]), "cancel");
        let err = extract_tar_xz(&archive, &dest, &AtomicBool::new(true))
            .expect_err("a cancelled extract must stop");
        assert!(matches!(err, ToolchainDownloadError::Cancelled), "{err}");
        assert!(!dest.join("hello.txt").exists());
    }

    /// The three edges (Tauri / HTTP / CLI) all parse their language here, so the default and
    /// the refusal are this type's business (v0.9 F3a-download-apply).
    #[test]
    fn the_toolchain_labels_round_trip_and_default_to_c() {
        assert_eq!(Toolchain::parse(None), Ok(Toolchain::C));
        assert_eq!(Toolchain::parse(Some("")), Ok(Toolchain::C));
        assert_eq!(Toolchain::parse(Some(" c ")), Ok(Toolchain::C));
        assert_eq!(Toolchain::parse(Some("zig")), Ok(Toolchain::Zig));
        assert_eq!(Toolchain::parse(Some("rust")), Ok(Toolchain::Rust));
        assert_eq!(Toolchain::C.label(), "c");
        assert_eq!(Toolchain::Zig.label(), "zig");
        assert_eq!(Toolchain::Rust.label(), "rust");
        let err = Toolchain::parse(Some("go")).expect_err("go is not a toolchain here");
        assert!(err.contains("rust"), "the message lists every label: {err}");
    }

    /// One asset, no platform branch, and the checksum the vendor published (v0.9 F3b-2).
    #[test]
    fn the_rust_spec_is_one_asset_for_every_platform() {
        let spec = rust_spec_for_current_platform().expect("the Rust spec needs no platform arm");
        assert_eq!(
            spec.url,
            "https://static.rust-lang.org/dist/rust-std-1.98.1-riscv64gc-unknown-none-elf.tar.xz"
        );
        assert_eq!(spec.version, RUST_VERSION);
        assert_eq!(spec.sha256, SHA256_RUST_STD_RISCV64GC);
        assert_eq!(spec.archive_kind, ArchiveKind::TarXz);
        assert_eq!(spec.toolchain, Toolchain::Rust);
        assert_eq!(
            spec.file_name(),
            "rust-std-1.98.1-riscv64gc-unknown-none-elf.tar.xz"
        );
        assert!(
            sha256_for_rust_asset("rust-std-1.98.1-riscv64gc-unknown-none-elf.tar.gz").is_err(),
            "only the published asset has a checksum here"
        );
        // Every label the edges accept maps to a spec, so `LABELS` cannot drift from `parse`.
        for label in Toolchain::LABELS {
            let kind = Toolchain::parse(Some(label)).expect("a label that parses");
            assert_eq!(kind.label(), label);
            assert!(spec_for_toolchain(kind).is_ok(), "{label}");
        }
    }

    /// The Rust locator returns a **directory** — the sysroot — at either depth (v0.9 F3b-2).
    #[test]
    fn find_rust_std_reads_both_depths() {
        let target = agent::RUST_TARGET;
        let name = format!("rust-std-{target}");

        // Depth 1: exactly how the component comes out of the archive.
        let nested =
            std::env::temp_dir().join(format!("riscdom-rust-nested-{}", std::process::id()));
        let _ = fs::remove_dir_all(&nested);
        let inner = nested.join(format!("rust-std-1.98.1-{target}")).join(&name);
        fs::create_dir_all(inner.join("lib").join("rustlib").join(target).join("lib"))
            .expect("create the sysroot shape");
        let found = find_rust_std(&nested).expect("the nested sysroot");
        assert_eq!(found, inner);
        assert!(found.is_dir(), "the product is a directory, not a file");

        // Depth 0: a sysroot somebody unpacked by hand.
        let flat = std::env::temp_dir().join(format!("riscdom-rust-flat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&flat);
        fs::create_dir_all(flat.join(&name).join("lib")).expect("create flat");
        assert_eq!(find_rust_std(&flat), Some(flat.join(&name)));

        let empty = std::env::temp_dir().join(format!("riscdom-rust-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&empty);
        fs::create_dir_all(&empty).expect("create empty");
        assert_eq!(find_rust_std(&empty), None);
    }

    /// Zig's five supported platforms, named and checksummed (0.16.0).
    ///
    /// `zig_asset_for` takes `os`/`arch` precisely so this test can cover every arm on one
    /// machine: `spec_for_current_platform`-style functions can only ever exercise the host's
    /// own arm.
    #[test]
    fn zig_covers_the_five_platforms_and_names_its_archives() {
        let cases = [
            (
                "windows",
                "x86_64",
                "zig-x86_64-windows-0.16.0.zip",
                ArchiveKind::Zip,
            ),
            (
                "macos",
                "x86_64",
                "zig-x86_64-macos-0.16.0.tar.xz",
                ArchiveKind::TarXz,
            ),
            (
                "macos",
                "aarch64",
                "zig-aarch64-macos-0.16.0.tar.xz",
                ArchiveKind::TarXz,
            ),
            (
                "linux",
                "x86_64",
                "zig-x86_64-linux-0.16.0.tar.xz",
                ArchiveKind::TarXz,
            ),
            (
                "linux",
                "aarch64",
                "zig-aarch64-linux-0.16.0.tar.xz",
                ArchiveKind::TarXz,
            ),
        ];
        let mut seen = std::collections::HashSet::new();
        for (os, arch, asset, kind) in cases {
            let (found, found_kind) = zig_asset_for(os, arch).expect("a supported platform");
            assert_eq!(found, asset, "{os}-{arch}");
            assert_eq!(found_kind, kind, "{os}-{arch}");
            let sha = sha256_for_zig_asset(&found).expect("a published checksum");
            assert_eq!(sha.len(), 64, "{found}");
            assert!(
                sha.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{sha}"
            );
            assert!(seen.insert(sha), "two assets share a checksum: {found}");
        }
        assert!(
            zig_asset_for("windows", "aarch64").is_err(),
            "Zig publishes this one; it is outside this batch's five"
        );
        assert!(zig_asset_for("freebsd", "x86_64").is_err());
        assert!(
            sha256_for_zig_asset("zig-0.16.0.tar.xz").is_err(),
            "the source tarball is not a product"
        );
    }

    /// The locator reads both layouts the release can land in: extracted, and extracted
    /// inside a parent (which is how `extract()` leaves a staging directory).
    #[test]
    fn find_zig_reads_both_depths() {
        let flat = std::env::temp_dir().join(format!("riscdom-zig-flat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&flat);
        fs::create_dir_all(&flat).expect("create flat");
        fs::write(flat.join("zig"), b"#!/bin/sh\n").expect("write zig");
        assert_eq!(find_zig(&flat), Some(flat.join("zig")));

        let nested =
            std::env::temp_dir().join(format!("riscdom-zig-nested-{}", std::process::id()));
        let _ = fs::remove_dir_all(&nested);
        let top = nested.join("zig-x86_64-linux-0.16.0");
        fs::create_dir_all(top.join("lib")).expect("create lib");
        fs::write(top.join("zig"), b"#!/bin/sh\n").expect("write zig");
        assert_eq!(find_zig(&nested), Some(top.join("zig")));

        let empty = std::env::temp_dir().join(format!("riscdom-zig-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&empty);
        fs::create_dir_all(&empty).expect("create empty");
        assert_eq!(find_zig(&empty), None);
    }
}
