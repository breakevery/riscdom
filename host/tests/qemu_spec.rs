//! v0.4 #4 — the QEMU *download spec*: which build this platform would need, and
//! the checksum it must be pinned to.
//!
//! The server-backed download / verify / extract tests live in `qemu_download.rs`;
//! these checks need no I/O at all.

use host::qemu_download::{
    install_guidance, spec_for_current_platform, winget_available, QemuDownloadError, QEMU_VERSION,
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

/// Every platform's install line names its own route — and every branch is reachable
/// on any one machine, because the platform and the `winget` flag are arguments.
#[test]
fn every_install_hint_names_its_own_route() {
    use sandbox::qemu_discover::{
        install_hint_for, QEMU_BREW_HINT, QEMU_DOWNLOAD_URL, QEMU_DOWNLOAD_URL_WINDOWS,
        QEMU_LINUX_PACKAGES, QEMU_WINGET_HINT,
    };

    let windows_with_winget = install_hint_for("windows", true);
    let windows_without = install_hint_for("windows", false);
    let macos = install_hint_for("macos", false);
    let linux = install_hint_for("linux", false);
    let other = install_hint_for("freebsd", false);
    println!("{windows_with_winget}\n{windows_without}\n{macos}\n{linux}\n{other}");

    assert!(
        windows_with_winget.contains(QEMU_WINGET_HINT),
        "{windows_with_winget}"
    );
    assert!(
        windows_without.contains(QEMU_DOWNLOAD_URL_WINDOWS)
            && windows_without.contains(QEMU_WINGET_HINT),
        "{windows_without}"
    );
    assert!(
        macos.contains(QEMU_BREW_HINT) && !macos.contains(QEMU_WINGET_HINT),
        "{macos}"
    );
    assert!(
        linux.contains(QEMU_LINUX_PACKAGES) && !linux.contains(QEMU_WINGET_HINT),
        "{linux}"
    );
    assert!(other.contains(QEMU_DOWNLOAD_URL), "{other}");
}

/// The live guidance names the route this platform actually has.
///
/// It does **not** pin *which* Windows branch is taken: detection is a property of the machine at
/// that instant, and a test that detects twice can disagree with itself (it did — `winget
/// --version` returned success and then failure back to back under load). The branches themselves
/// are pinned deterministically by [`every_install_hint_names_its_own_route`].
#[test]
fn the_guidance_names_this_platforms_route() {
    use sandbox::qemu_discover::{
        QEMU_BREW_HINT, QEMU_DOWNLOAD_URL, QEMU_DOWNLOAD_URL_WINDOWS, QEMU_LINUX_PACKAGES,
        QEMU_WINGET_HINT,
    };

    let guidance = install_guidance();
    println!("winget available: {}", winget_available());
    println!("guidance: {guidance}");
    assert!(!guidance.is_empty());
    assert!(
        [
            QEMU_WINGET_HINT,
            QEMU_DOWNLOAD_URL_WINDOWS,
            QEMU_BREW_HINT,
            QEMU_LINUX_PACKAGES,
            QEMU_DOWNLOAD_URL,
        ]
        .iter()
        .any(|marker| guidance.contains(marker)),
        "the guidance must name a route: {guidance}"
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
