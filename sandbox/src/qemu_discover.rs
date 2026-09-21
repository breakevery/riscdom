//! Locating the QEMU system binary (v0.3 #5a).
//!
//! Mirrors the RISC-V GCC discovery in `agent::compiler` — without any
//! cross-crate dependency (the `sandbox` crate never depends on `agent`).
//!
//! Order: `RISCDOM_QEMU` → `QEMU_SYSTEM_RISCV64` → well-known install locations
//! → `PATH`. An environment variable that is set but does not point at an
//! existing file is an explicit error (never silently skipped).
//!
//! QEMU is **not** bundled and **not** downloaded: the user installs it and we
//! find it (bundling/downloading is a v0.4 decision).

use std::path::{Path, PathBuf};

/// Where the QEMU executable came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QemuSource {
    /// `RISCDOM_QEMU` / `QEMU_SYSTEM_RISCV64`.
    EnvVar,
    /// A well-known install location.
    KnownPath,
    /// Found on `PATH`.
    Path,
    /// Configured explicitly by the user (host `set_qemu_path`).
    Manual,
}

impl QemuSource {
    /// Stable identifier (also used by the host / UI).
    pub fn as_str(self) -> &'static str {
        match self {
            QemuSource::EnvVar => "EnvVar",
            QemuSource::KnownPath => "KnownPath",
            QemuSource::Path => "Path",
            QemuSource::Manual => "Manual",
        }
    }
}

impl std::fmt::Display for QemuSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A resolved QEMU executable plus where it came from.
#[derive(Debug, Clone)]
pub struct QemuLocation {
    pub exe: PathBuf,
    pub source: QemuSource,
}

/// Failures from [`discover`].
#[derive(Debug, Clone)]
pub enum QemuDiscoverError {
    /// Nothing matched; `diagnostics` lists every location that was tried.
    NotFound(String),
}

impl std::fmt::Display for QemuDiscoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QemuDiscoverError::NotFound(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for QemuDiscoverError {}

impl QemuDiscoverError {
    /// Stable code for the UI / audit detail.
    pub fn code(&self) -> &'static str {
        match self {
            QemuDiscoverError::NotFound(_) => "qemu_not_found",
        }
    }
}

/// Environment variables consulted, in priority order.
const QEMU_ENV_VARS: [&str; 2] = ["RISCDOM_QEMU", "QEMU_SYSTEM_RISCV64"];

/// The official QEMU download page, with no platform anchor (macOS / Linux / other).
pub const QEMU_DOWNLOAD_URL: &str = "https://www.qemu.org/download/";

/// The Windows-anchored download page 鈥?the route for a Windows machine without `winget`.
pub const QEMU_DOWNLOAD_URL_WINDOWS: &str = "https://www.qemu.org/download/#windows";

/// Suggested Windows install command (the route when `winget` is present).
pub const QEMU_WINGET_HINT: &str = "winget install SoftwareFreedomConservancy.QEMU";

/// Suggested macOS install command.
pub const QEMU_BREW_HINT: &str = "brew install qemu";

/// Suggested Linux packages (the RISC-V system emulator lives in these).
pub const QEMU_LINUX_PACKAGES: &str = "qemu-system-misc (Debian/Ubuntu) or qemu (Arch/Fedora)";

/// How to install QEMU on `os` (`"windows"` / `"macos"` / `"linux"`), as one
/// actionable line. `winget` picks the Windows branch 鈥?the command when it is
/// present, the download page otherwise (which still names the command, because a
/// caller that cannot detect `winget` must not hide it).
///
/// Pure and total, so every branch is testable on any one machine.
pub fn install_hint_for(os: &str, winget: bool) -> String {
    match os {
        "windows" if winget => format!("run `{QEMU_WINGET_HINT}`"),
        "windows" => format!(
            "download an installer from {QEMU_DOWNLOAD_URL_WINDOWS}, or run `{QEMU_WINGET_HINT}`"
        ),
        "macos" => format!("run `{QEMU_BREW_HINT}`"),
        "linux" => format!("install your distribution's package: {QEMU_LINUX_PACKAGES}"),
        other => format!("install QEMU yourself ({other}); see {QEMU_DOWNLOAD_URL}"),
    }
}

/// `qemu-system-riscv64` (+ `.exe` on Windows).
pub fn exe_name() -> String {
    if cfg!(windows) {
        "qemu-system-riscv64.exe".to_string()
    } else {
        "qemu-system-riscv64".to_string()
    }
}

/// Locate the QEMU system binary for RISC-V 64.
pub fn discover() -> Result<QemuLocation, QemuDiscoverError> {
    let (found, log) = search();
    match found {
        Some((exe, source)) => Ok(QemuLocation { exe, source }),
        None => Err(QemuDiscoverError::NotFound(not_found_message(&log))),
    }
}

/// Human-readable record of the search: where we looked and what happened.
pub fn diagnostics() -> String {
    let (found, log) = search();
    let mut out = String::from("QEMU search:\n");
    for line in &log {
        out.push_str("  - ");
        out.push_str(line);
        out.push('\n');
    }
    match found {
        Some((exe, source)) => out.push_str(&format!(
            "  => found: {} (source: {source})\n",
            exe.display()
        )),
        None => out.push_str("  => not found\n"),
    }
    out
}

/// A candidate path plus the label used in the diagnostics.
struct Candidate {
    label: String,
    path: Option<PathBuf>,
}

/// Well-known install locations (Windows first, Unix covered too).
fn known_candidates() -> Vec<Candidate> {
    let exe = exe_name();
    let mut out = Vec::new();

    if cfg!(windows) {
        for base in [r"C:\Program Files\qemu", r"C:\Program Files (x86)\qemu"] {
            out.push(Candidate {
                label: format!(r"{base}\{exe}"),
                path: Some(PathBuf::from(base).join(&exe)),
            });
        }
        for base in [r"C:\msys64\mingw64\bin", r"C:\msys64\ucrt64\bin"] {
            out.push(Candidate {
                label: format!(r"{base}\{exe}"),
                path: Some(PathBuf::from(base).join(&exe)),
            });
        }
        // Developer machines keep tools under <drive>:\tools\…
        for base in [r"C:\tools", r"D:\tools"] {
            let label = format!(r"{base}\**\{exe}");
            let hit = scan_for_qemu(Path::new(base), 3);
            out.push(Candidate { label, path: hit });
        }
    } else {
        for p in [
            "/opt/homebrew/bin/qemu-system-riscv64",
            "/usr/local/bin/qemu-system-riscv64",
            "/usr/bin/qemu-system-riscv64",
        ] {
            out.push(Candidate {
                label: p.to_string(),
                path: Some(PathBuf::from(p)),
            });
        }
    }

    out
}

/// Directory entries of `base` (empty when unreadable).
fn list_dirs(base: &str) -> Vec<PathBuf> {
    std::fs::read_dir(base)
        .map(|rd| {
            let mut v: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            v.sort();
            v
        })
        .unwrap_or_default()
}

/// Bounded recursive scan for the QEMU binary under `root`.
fn scan_for_qemu(root: &Path, max_depth: usize) -> Option<PathBuf> {
    let exe = exe_name();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        let direct = dir.join(&exe);
        if direct.is_file() {
            return Some(direct);
        }
        for entry in list_dirs(&dir.to_string_lossy()) {
            let bin = entry.join(&exe);
            if bin.is_file() {
                return Some(bin);
            }
            stack.push((entry, depth + 1));
        }
    }
    None
}

/// Look for the QEMU binary on `PATH`.
fn find_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exe = exe_name();
    let bare = "qemu-system-riscv64";
    for dir in std::env::split_paths(&path) {
        let with_exe = dir.join(&exe);
        if with_exe.is_file() {
            return Some(with_exe);
        }
        if cfg!(windows) {
            if let Some(parent) = with_exe.parent() {
                let _ = parent;
            }
        }
        let direct = dir.join(bare);
        if direct.is_file() {
            return Some(direct);
        }
    }
    None
}

/// The full search, returning the hit (if any) plus a human-readable log.
fn search() -> (Option<(PathBuf, QemuSource)>, Vec<String>) {
    let mut log = Vec::new();

    for var in QEMU_ENV_VARS {
        match std::env::var(var) {
            Ok(v) if !v.trim().is_empty() => {
                let path = PathBuf::from(v.trim());
                if path.is_file() {
                    log.push(format!("{var} = {} (found)", path.display()));
                    return (Some((path, QemuSource::EnvVar)), log);
                }
                log.push(format!(
                    "{var} = {} (set, but that file does not exist)",
                    path.display()
                ));
                return (None, log);
            }
            _ => log.push(format!("{var} (not set)")),
        }
    }

    for candidate in known_candidates() {
        match candidate.path {
            Some(path) if path.is_file() => {
                log.push(format!("{} (found)", candidate.label));
                return (Some((path, QemuSource::KnownPath)), log);
            }
            _ => log.push(format!("{} (not found)", candidate.label)),
        }
    }

    match find_on_path() {
        Some(path) => {
            log.push(format!(
                "PATH qemu-system-riscv64 -> {} (found)",
                path.display()
            ));
            (Some((path, QemuSource::Path)), log)
        }
        None => {
            log.push(format!("PATH {} (not found)", exe_name()));
            (None, log)
        }
    }
}

/// Actionable multi-line message shown when nothing was found.
fn not_found_message(log: &[String]) -> String {
    let exe = exe_name();
    let mut msg = String::from("QEMU (qemu-system-riscv64) not found.\nSearched:\n");
    for line in log {
        msg.push_str("  - ");
        msg.push_str(line);
        msg.push('\n');
    }
    // `false` because the sandbox cannot detect `winget` (that lives in the host);
    // the Windows branch it selects names both the page and the command.
    msg.push_str(&format!(
        "Install QEMU: {}\n\
         then set RISCDOM_QEMU to the full path of {exe} and restart RiscDom.",
        install_hint_for(std::env::consts::OS, false)
    ));
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The install line follows the platform, and the Windows branch follows `winget`.
    ///
    /// The branches are decided by an argument rather than by the machine, so all of
    /// them run wherever the gate runs.
    #[test]
    fn the_install_hint_follows_the_platform() {
        let win_cmd = install_hint_for("windows", true);
        let win_page = install_hint_for("windows", false);
        assert!(win_cmd.contains(QEMU_WINGET_HINT), "{win_cmd}");
        assert!(!win_cmd.contains(QEMU_DOWNLOAD_URL_WINDOWS), "{win_cmd}");
        assert!(win_page.contains(QEMU_DOWNLOAD_URL_WINDOWS), "{win_page}");
        assert!(win_page.contains(QEMU_WINGET_HINT), "{win_page}");

        let macos = install_hint_for("macos", false);
        assert!(macos.contains(QEMU_BREW_HINT), "{macos}");
        assert!(!macos.contains(QEMU_WINGET_HINT), "{macos}");

        let linux = install_hint_for("linux", false);
        assert!(linux.contains(QEMU_LINUX_PACKAGES), "{linux}");
        assert!(!linux.contains(QEMU_WINGET_HINT), "{linux}");

        let other = install_hint_for("freebsd", false);
        assert!(other.contains(QEMU_DOWNLOAD_URL), "{other}");
    }

    /// The not-found message carries the platform hint and the variable to set.
    #[test]
    fn the_not_found_message_is_actionable() {
        let message = not_found_message(&["RISCDOM_QEMU (not set)".to_string()]);
        assert!(message.contains("RISCDOM_QEMU (not set)"), "{message}");
        assert!(
            message.contains(&install_hint_for(std::env::consts::OS, false)),
            "{message}"
        );
        assert!(message.contains(exe_name().as_str()), "{message}");
    }
}
