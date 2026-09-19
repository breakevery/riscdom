//! v0.4 #4 — the QEMU *download spec*: which build this platform would need, and
//! the checksum it must be pinned to.
//!
//! The server-backed download / verify / extract tests live in `qemu_download.rs`;
//! these checks need no I/O at all.

use host::qemu_download::{
    install_guidance, install_guidance_with, spec_for_current_platform, winget_available,
    QemuDownloadError, QEMU_VERSION,
};

/// **The repository pins no QEMU build, and this test says so out loud.**
///
/// A spec carries a URL and a SHA-256. Neither may be guessed: a wrong digest that
/// nobody notices is worse than no download at all, and this module has no
/// "skip verification" switch. The project therefore *guides* the user to a QEMU they
/// install themselves — where that release comes from is decided in
/// `docs/qemu-distribution.md` §5.
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
    // The error has to be actionable: it names the release and carries the guidance.
    assert!(text.contains(QEMU_VERSION), "{text}");
    assert!(text.contains("Install it yourself"), "{text}");
    assert!(
        text.contains(&install_guidance()),
        "the error must carry the guidance: {text}"
    );
}

/// Both guidance branches name their source, whether or not this machine has `winget`.
#[test]
fn both_guidance_branches_name_their_source() {
    let with = install_guidance_with(true);
    let without = install_guidance_with(false);
    println!("with winget: {with}");
    println!("without winget: {without}");
    assert!(
        with.contains(sandbox::qemu_discover::QEMU_WINGET_HINT),
        "{with}"
    );
    assert!(
        without.contains(sandbox::qemu_discover::QEMU_DOWNLOAD_URL),
        "{without}"
    );
}

/// The live guidance is one of exactly the two documented things.
///
/// It does **not** pin which one: detection is a property of the machine at that instant, and a test
/// that detects twice can disagree with itself (it did — `winget --version` returned success and
/// then failure back to back under load). The branches themselves are pinned deterministically by
/// [`both_guidance_branches_name_their_source`].
#[test]
fn the_guidance_names_winget_or_the_download_page() {
    let guidance = install_guidance();
    println!("winget available: {}", winget_available());
    println!("guidance: {guidance}");
    assert!(!guidance.is_empty());
    assert!(
        guidance.contains(sandbox::qemu_discover::QEMU_WINGET_HINT)
            || guidance.contains(sandbox::qemu_discover::QEMU_DOWNLOAD_URL),
        "the guidance must name winget or the download page: {guidance}"
    );
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
