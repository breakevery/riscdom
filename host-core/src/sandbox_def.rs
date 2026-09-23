//! Sandbox definitions: the named assembly of the resources a run needs (v0.9 F2a).
//!
//! A definition is **inputs, not a runtime**: it names which compiler, which QEMU,
//! which kernel and how much memory a run should use. Two shapes exist on purpose:
//!
//! - [`SandboxDef`] is what is **stored** (in `settings.json`) — the fields a person
//!   writes, every one optional but the name;
//! - [`SandboxView`] is what is **served** — the same fields plus the three things
//!   only the host can answer at the moment of the question: `source`, `runnable`
//!   and `shadowed`.
//!
//! `runnable` is deliberately **not stored**. It depends on what is installed right
//! now: a definition whose QEMU was uninstalled is still a valid definition, it just
//! cannot run. `shadowed` is a property of the merged registry, not of an entry.
//!
//! The registry is assembled from three sources on every read (F2a decision 4):
//! definitions written by hand, resources the scan found, and one built-in fallback
//! named [`DEFAULT_SANDBOX_NAME`]. Nothing the scan finds is ever written back to
//! `settings.json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The name of the built-in fallback definition ("use whatever the host finds").
pub const DEFAULT_SANDBOX_NAME: &str = "default";

/// The `version` a candidate carries when it is **not** a versioned install (v0.9
/// sandbox F2a-3).
///
/// A resource the data directory holds has a version directory beside its
/// siblings; the QEMU a machine already has does not, and the scan records this
/// sentinel for it rather than inventing a version.
pub const NO_VERSION: &str = "-";

/// Where an entry in the merged registry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxSource {
    /// Written by hand in `settings.json`.
    Manual,
    /// Found by the scan, or the built-in fallback.
    Discovered,
}

impl SandboxSource {
    /// The name the API and the CLI print.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Discovered => "discovered",
        }
    }
}

/// One sandbox definition, as stored in `settings.json`.
///
/// Every field but `name` is optional, and every one has `#[serde(default)]`, so a
/// hand-written file may carry any subset — and a file written before these fields
/// existed still parses (F2a decision 3: no migration).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxDef {
    /// Stable name; what a request names and what the merge matches on.
    pub name: String,
    /// Human-readable name for the UI.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Guest RAM in megabytes; `None` means the host's default (128 MiB).
    #[serde(default)]
    pub memory_mb: Option<u32>,
    /// The QEMU executable; `None` means discovery.
    #[serde(default)]
    pub qemu_exe: Option<PathBuf>,
    /// The RISC-V GCC to compile with; `None` means discovery.
    #[serde(default)]
    pub toolchain_path: Option<PathBuf>,
    /// The ELF to boot, when the definition pins one (F2a decision 2).
    ///
    /// `None` leaves today's behaviour alone: the model picks the ELF in `start_vm`,
    /// and a snapshot restore takes the newest `.elf` in the workspace root.
    #[serde(default)]
    pub kernel: Option<PathBuf>,
    /// Free-form note for a human (why this definition exists).
    #[serde(default)]
    pub notes: Option<String>,
}

impl SandboxDef {
    /// A definition that names one scanned resource.
    ///
    /// The scan builds one of these per installed resource, so a partial definition
    /// ("this toolchain, everything else discovered") is expressible without a
    /// cartesian product (F2a decision 6).
    pub fn for_resource(kind: &str, version: &str, path: PathBuf) -> Self {
        let (qemu_exe, toolchain_path) = match kind {
            "qemu" => (Some(path), None),
            _ => (None, Some(path)),
        };
        Self {
            name: resource_name(kind, version),
            display_name: None,
            memory_mb: None,
            qemu_exe,
            toolchain_path,
            kernel: None,
            notes: None,
        }
    }

    /// The fallback: no overrides at all.
    pub fn fallback() -> Self {
        Self {
            name: DEFAULT_SANDBOX_NAME.to_string(),
            display_name: Some("whatever this host finds".to_string()),
            memory_mb: None,
            qemu_exe: None,
            toolchain_path: None,
            kernel: None,
            notes: None,
        }
    }
}

/// The name a scanned resource gets: `<kind>-<version>`.
///
/// A resource the scan knows **no** version for is named for the resource itself
/// instead — `format!("{kind}-{version}")` on [`NO_VERSION`] produced a definition
/// called `qemu--`, which is not a name anybody can type (v0.9 sandbox F2a-3). The
/// machine's own QEMU is therefore the emulator it is, `qemu-system-riscv64` (the
/// stem the sandbox's discovery searches for, so this file keeps no second copy of
/// the name — the `.exe` suffix a Windows build carries is a file name, not a
/// definition name).
fn resource_name(kind: &str, version: &str) -> String {
    if version != NO_VERSION {
        return format!("{kind}-{version}");
    }
    match kind {
        "qemu" => sandbox::qemu_discover::exe_name()
            .trim_end_matches(".exe")
            .to_string(),
        other => other.to_string(),
    }
}

/// One sandbox, as served: the definition plus what only the host knows now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SandboxView {
    pub name: String,
    pub display_name: Option<String>,
    pub memory_mb: Option<u32>,
    pub qemu_exe: Option<String>,
    pub toolchain_path: Option<String>,
    pub kernel: Option<String>,
    pub notes: Option<String>,
    /// `manual` or `discovered`.
    pub source: SandboxSource,
    /// Could this definition run **right now**? See the module docs.
    pub runnable: bool,
    /// Is a hand-written definition using this name? The entry stays in the list,
    /// marked, so the merge is visible instead of silent.
    pub shadowed: bool,
}

/// One installed resource the scan found. Never a definition, never written back.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateView {
    /// `toolchain` or `qemu`.
    pub kind: String,
    /// The version directory it was found under; [`NO_VERSION`] for a system install.
    pub version: String,
    /// The executable itself.
    pub path: String,
    /// `installed` (under the data directory) or `system` (found on this machine).
    pub origin: String,
    /// Does it run (`--version` exits 0)?
    pub runnable: bool,
}

impl CandidateView {
    /// One resource under the data directory.
    pub fn installed(kind: &str, version: &str, path: &std::path::Path, runnable: bool) -> Self {
        Self {
            kind: kind.to_string(),
            version: version.to_string(),
            path: path.display().to_string(),
            origin: "installed".to_string(),
            runnable,
        }
    }

    /// The QEMU this machine already has (found through the sandbox's discovery).
    pub fn system_qemu(path: &std::path::Path) -> Self {
        Self {
            kind: "qemu".to_string(),
            version: NO_VERSION.to_string(),
            path: path.display().to_string(),
            origin: "system".to_string(),
            // Discovery only returns a location it found; whether it *runs* is what
            // `probe_qemu` answers, and a system QEMU that does not run is not worth
            // a second probe here.
            runnable: true,
        }
    }
}

/// The scan's answer: two independent lists (F2a decision 6 — no cartesian product).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidatesView {
    pub toolchains: Vec<CandidateView>,
    pub qemus: Vec<CandidateView>,
}

/// Scan one data directory: `<base>/toolchain/*`, `<base>/qemu/*` and the
/// machine's own QEMU.
///
/// Bounded to `base` (the host owns it — never the repository, never the agent
/// workspace): each resource directory holds one directory per version, side by
/// side, and the executable is found inside it by the downloader's own scan, so
/// the two agree on what "installed" means. `.download-tmp` / `.extract-tmp` are
/// the installers' by-products, not versions, and are skipped.
///
/// Pure reading: nothing is written, and no candidate is executed. The result is
/// a candidate list, never a registry (F2a decision 3 — the scan is never written
/// back to `settings.json`).
pub fn discover_in(base: &Path) -> CandidatesView {
    let toolchains = scanned(
        &base.join("toolchain"),
        crate::toolchain_download::find_compiler,
    )
    .into_iter()
    .map(|(version, exe)| CandidateView::installed("toolchain", &version, &exe, true))
    .collect();

    let mut qemus: Vec<CandidateView> =
        scanned(&base.join("qemu"), crate::qemu_download::find_qemu)
            .into_iter()
            .map(|(version, exe)| CandidateView::installed("qemu", &version, &exe, true))
            .collect();
    if let Ok(location) = sandbox::qemu_discover::discover() {
        qemus.push(CandidateView::system_qemu(&location.exe));
    }

    CandidatesView { toolchains, qemus }
}

/// Every version directory under `dir`, with the executable `find` locates in it.
///
/// Sorted by version so the listing is deterministic; a missing directory is an
/// empty list, not an error (nothing is installed yet is the normal state).
fn scanned(dir: &Path, find: impl Fn(&Path) -> Option<PathBuf>) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let version = entry.file_name().to_string_lossy().to_string();
        if version.starts_with('.') {
            continue;
        }
        if let Some(exe) = find(&path) {
            found.push((version, exe));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_definition_round_trips() {
        let def = SandboxDef {
            name: "blink".into(),
            display_name: Some("Blink".into()),
            memory_mb: Some(256),
            qemu_exe: Some(PathBuf::from("/opt/qemu/qemu-system-riscv64")),
            toolchain_path: Some(PathBuf::from("/opt/gcc/bin/riscv-none-elf-gcc")),
            kernel: Some(PathBuf::from("hello.elf")),
            notes: Some("for the blink example".into()),
        };
        let text = serde_json::to_string(&def).expect("serialises");
        let back: SandboxDef = serde_json::from_str(&text).expect("parses");
        assert_eq!(back, def);
    }

    #[test]
    fn a_hand_written_file_may_carry_only_a_name() {
        // Every other field defaults, which is what makes `sandboxes[]` additive.
        let bare: SandboxDef = serde_json::from_str(r#"{"name":"only-a-name"}"#).expect("parses");
        assert_eq!(bare.name, "only-a-name");
        assert_eq!(bare.memory_mb, None);
        assert_eq!(bare.qemu_exe, None);
        assert_eq!(bare.toolchain_path, None);
        assert_eq!(bare.kernel, None);
        assert_eq!(bare.notes, None);
        assert_eq!(bare.display_name, None);
    }

    #[test]
    fn a_scanned_definition_names_one_resource() {
        let toolchain = SandboxDef::for_resource("toolchain", "15.2.0-1", PathBuf::from("/gcc"));
        assert_eq!(toolchain.name, "toolchain-15.2.0-1");
        assert_eq!(toolchain.toolchain_path, Some(PathBuf::from("/gcc")));
        assert_eq!(toolchain.qemu_exe, None);

        let qemu = SandboxDef::for_resource("qemu", "11.1.0", PathBuf::from("/qemu"));
        assert_eq!(qemu.name, "qemu-11.1.0");
        assert_eq!(qemu.qemu_exe, Some(PathBuf::from("/qemu")));
        assert_eq!(qemu.toolchain_path, None);
    }

    #[test]
    fn a_resource_without_a_version_is_named_for_the_resource() {
        // The machine's own QEMU is not a versioned install, so the scan records
        // no version for it. `qemu--` is what concatenating that sentinel used to
        // produce (v0.9 sandbox F2a-3); the name is the emulator's instead, on
        // every platform (a Windows build's `.exe` is a file name, not a name).
        let system = SandboxDef::for_resource("qemu", NO_VERSION, PathBuf::from("/usr/bin/qemu"));
        assert_eq!(system.name, "qemu-system-riscv64");
        assert_eq!(system.qemu_exe, Some(PathBuf::from("/usr/bin/qemu")));
        assert_eq!(system.toolchain_path, None);

        // A versioned install keeps the concatenated name, unchanged.
        let installed = SandboxDef::for_resource("qemu", "11.1.0", PathBuf::from("/qemu"));
        assert_eq!(installed.name, "qemu-11.1.0");
    }

    #[test]
    fn the_fallback_overrides_nothing() {
        let def = SandboxDef::fallback();
        assert_eq!(def.name, DEFAULT_SANDBOX_NAME);
        assert!(def.qemu_exe.is_none() && def.toolchain_path.is_none() && def.kernel.is_none());
        assert_eq!(def.memory_mb, None);
    }

    #[test]
    fn the_source_names_are_the_lowercase_words() {
        assert_eq!(SandboxSource::Manual.as_str(), "manual");
        assert_eq!(SandboxSource::Discovered.as_str(), "discovered");
        assert_eq!(
            serde_json::to_string(&SandboxSource::Manual).expect("serialises"),
            r#""manual""#
        );
    }
}
