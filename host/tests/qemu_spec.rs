//! v0.4 #4 — the QEMU *download spec*: which build this platform would need, and
//! the checksum it must be pinned to.
//!
//! The server-backed download / verify / extract tests live in `qemu_download.rs`;
//! these checks need no I/O at all.

use host::qemu_download::{spec_for_current_platform, QemuDownloadError, QEMU_VERSION};

/// **The repository currently pins no QEMU build, and this test says so out loud.**
///
/// A spec carries a URL and a SHA-256. Neither may be guessed: a wrong digest that
/// nobody notices is worse than no download at all, and this module has no
/// "skip verification" switch. Where the release comes from, and why no build can
/// be pinned yet, is in `docs/qemu-distribution.md` §5 and the module header.
///
/// When a build *can* honestly be pinned, this test is the one to replace — it
/// exists so that silence is not mistaken for coverage.
#[test]
fn no_qemu_spec_is_invented_for_this_platform() {
    let error =
        spec_for_current_platform().expect_err("no QEMU build is pinned in this repository");
    assert!(
        matches!(error, QemuDownloadError::UnpinnedPlatform { .. }),
        "{error}"
    );
    assert_eq!(error.code(), "unpinned_platform");

    let text = error.to_string();
    println!("{text}");
    // The error has to be actionable: it names the release and where to get QEMU.
    assert!(text.contains(QEMU_VERSION), "{text}");
    assert!(text.contains("Install QEMU yourself"), "{text}");
}

/// The fields a spec is made of behave the way `download_and_install` assumes.
#[test]
fn a_spec_names_the_archive_after_the_last_url_segment() {
    let spec = host::qemu_download::QemuDownloadSpec {
        version: "1.2.3".into(),
        url: "https://example.invalid/releases/v1.2.3/qemu-1.2.3-win64.zip".into(),
        sha256: "00".into(),
        archive_kind: host::qemu_download::ArchiveKind::Zip,
        install_subdir: "qemu-1.2.3".into(),
    };
    assert_eq!(spec.file_name(), "qemu-1.2.3-win64.zip");
}
