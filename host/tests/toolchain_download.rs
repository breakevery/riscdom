//! Stage v0.3-3a — the *download spec*: which asset this platform needs and the
//! checksum we pin it to.
//!
//! The server-backed download / verify / extract tests live in
//! `download_tests.rs`; these checks need no I/O at all.

use host::toolchain_download::{
    spec_for_current_platform, ArchiveKind, DownloadSpec, XPACK_RISCV_GCC_VERSION,
};

#[test]
fn the_pinned_spec_matches_this_platform() {
    let spec = spec_for_current_platform().expect("this platform is supported");
    println!("url: {}", spec.url);
    println!("sha256: {}", spec.sha256);

    assert!(spec.url.starts_with("https://github.com/xpack-dev-tools/"));
    assert!(
        spec.url.contains(&format!("/v{XPACK_RISCV_GCC_VERSION}/")),
        "{}",
        spec.url
    );
    assert_eq!(
        spec.sha256.len(),
        64,
        "a real SHA-256 must be pinned (no skip-verification mode)"
    );
    assert!(spec.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(
        spec.install_subdir,
        format!("xpack-riscv-none-elf-gcc-{XPACK_RISCV_GCC_VERSION}")
    );

    let name = spec.file_name();
    assert!(
        name.starts_with(&format!(
            "xpack-riscv-none-elf-gcc-{XPACK_RISCV_GCC_VERSION}-"
        )),
        "{name}"
    );

    // The archive kind must follow the platform: zip on Windows, tar.gz elsewhere.
    if cfg!(target_os = "windows") {
        assert!(matches!(spec.archive_kind, ArchiveKind::Zip), "{name}");
        assert!(name.ends_with("win32-x64.zip"), "{name}");
    } else {
        assert!(matches!(spec.archive_kind, ArchiveKind::TarGz), "{name}");
        assert!(name.ends_with(".tar.gz"), "{name}");
    }
}

#[test]
fn the_asset_name_is_the_last_url_segment() {
    let spec = DownloadSpec {
        version: "1.2.3".into(),
        url: "https://example.invalid/releases/download/v1.2.3/toolchain-x.tar.gz".into(),
        sha256: "00".into(),
        archive_kind: ArchiveKind::TarGz,
        install_subdir: "toolchain-x".into(),
    };
    assert_eq!(spec.file_name(), "toolchain-x.tar.gz");
}
