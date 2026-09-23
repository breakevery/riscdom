//! Workspace archives: a project leaves and enters as one file (v0.9 project in/out).
//!
//! The AI's workspace is a directory, and "take the project with you" means
//! packing that directory into an archive and unpacking one back. Both directions
//! are **containment-only** on purpose: `WorkspacePolicy::check_write` allows four
//! source extensions (`.c` / `.h` / `.S` / `.s`), and a project is not made of
//! source files alone — a `README.md`, a linker script, a fixture. So the rule here
//! is the one the exports already use: a path may not escape the destination, and
//! nothing else is assumed about what a project contains.
//!
//! An archive coming **in** is untrusted input, which is the whole difference from
//! the toolchain and QEMU downloads: those are vendor artifacts with a pinned
//! SHA-256, so their extractors can afford to trust the shape. Here every entry is
//! checked — no traversal, no absolute paths, no symlinks or hard links (a link is
//! a way to write *outside* the workspace while looking like an entry inside it),
//! no `.riscdom/` (that directory is the host's own state: the audit DB, snapshots,
//! the preflight cache — a project archive must not be able to overwrite it).
//!
//! # Format
//!
//! **tar.gz** for export. It keeps the directory structure, is what a source tree
//! naturally is, and unpacks with the tools everybody already has; a `.zip` would
//! be friendlier to a double-click and no better at keeping a tree. Import accepts
//! **both**, chosen by what the caller says it sent (`Content-Type`) or, failing
//! that, by the bytes themselves.

use crate::HostError;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

/// The directory the host keeps its own state in, inside a workspace.
///
/// Never packed, never unpacked: it holds the audit DB, snapshots and the
/// preflight cache, none of which is project content (v0.9 project in/out).
pub const HOST_STATE_DIR: &str = ".riscdom";

/// How many entries one import may write. A cheap ceiling on a decompression
/// bomb that still leaves room for a real project.
pub const MAX_ARCHIVE_ENTRIES: usize = 100_000;

/// How many bytes one import may write in total, after decompression.
pub const MAX_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;

/// What an archive is, as far as the host can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    /// An uncompressed tar. A project archive is usually gzipped, but `.tar` is
    /// what `tar cf` produces, and refusing it would be pedantry.
    Tar,
    TarGz,
}

impl ArchiveFormat {
    /// The format a `Content-Type` names, when it names one we handle.
    ///
    /// Both the plain and the `+gzip` spellings of gzip are accepted, because
    /// clients disagree about which one a `.tar.gz` is.
    pub fn from_content_type(content_type: &str) -> Option<Self> {
        let essence = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match essence.as_str() {
            "application/zip" | "application/x-zip-compressed" => Some(ArchiveFormat::Zip),
            "application/gzip"
            | "application/x-gzip"
            | "application/x-tar+gzip"
            | "application/x-compressed-tar" => Some(ArchiveFormat::TarGz),
            "application/x-tar" | "application/tar" => Some(ArchiveFormat::Tar),
            _ => None,
        }
    }

    /// The format the bytes name, by their first few bytes.
    ///
    /// A zip starts with `PK` (one of the four local-header/end signatures); a
    /// gzip stream starts with the two bytes `1f 8b`; a tar has no leading magic
    /// at all, so it is recognised by the `ustar` marker every tar writes 257 bytes
    /// in. Anything else is not an archive we can read, and saying so is better
    /// than guessing.
    pub fn from_magic(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0x50, 0x4b, ..] => Some(ArchiveFormat::Zip),
            [0x1f, 0x8b, ..] => Some(ArchiveFormat::TarGz),
            _ if bytes.len() > 262 && &bytes[257..262] == b"ustar" => Some(ArchiveFormat::Tar),
            _ => None,
        }
    }

    /// The `Content-Type` this format is served with.
    pub fn content_type(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "application/zip",
            ArchiveFormat::Tar | ArchiveFormat::TarGz => "application/gzip",
        }
    }

    /// The default file name this format is offered under.
    pub fn default_file_name(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "workspace.zip",
            ArchiveFormat::Tar => "workspace.tar",
            ArchiveFormat::TarGz => "workspace.tar.gz",
        }
    }
}

/// What an import wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct UnpackReport {
    /// Files written (directories are not counted: they hold no content).
    pub files: usize,
    /// Bytes written, as the archive claimed them.
    pub bytes: u64,
}

/// A path inside the archive, resolved against the destination.
///
/// The same guard the toolchain download uses, for the same reason: a component
/// that can walk upward, or an absolute path, would let an entry write anywhere.
/// It answers the destination-relative path on success, so the caller still has to
/// join it — this only says the name is *shaped* like a path inside.
fn safe_relative(raw: &str, dest: &Path) -> Result<PathBuf, HostError> {
    // A Windows-produced archive may name entries with backslashes; a `\` is a
    // separator there and a legal file-name character here, and treating it as a
    // separator is the reading that does not surprise the user who unpacked it.
    let normalised = raw.replace('\\', "/");
    let rel = Path::new(&normalised);
    if rel.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(HostError::Archive(format!(
            "entry escapes the workspace: {raw}"
        )));
    }
    if rel.as_os_str().is_empty() {
        return Err(HostError::Archive("entry has no name".to_string()));
    }
    // The host's own state, and anything else hidden at the top level, is not
    // project content.
    let first = normalised.split('/').next().unwrap_or_default();
    if first == HOST_STATE_DIR {
        return Err(HostError::Archive(format!(
            "entry is the host's own state, not project content: {raw}"
        )));
    }
    Ok(dest.join(rel))
}

/// Unpack `archive` into `dest`.
///
/// `force` decides what happens when an entry names a file that is already there:
/// by default the import **refuses** (the caller asked to bring a project in, and
/// silently replacing what is in the workspace is not that); with `force` the file
/// is replaced, still one checked path at a time.
pub fn unpack_archive(
    archive: &[u8],
    dest: &Path,
    format: ArchiveFormat,
    force: bool,
) -> Result<UnpackReport, HostError> {
    std::fs::create_dir_all(dest)?;
    match format {
        ArchiveFormat::Zip => unpack_zip(archive, dest, force),
        ArchiveFormat::Tar => unpack_tar(archive, dest, force, None),
        ArchiveFormat::TarGz => unpack_tar(archive, dest, force, Some(())),
    }
}

fn unpack_zip(archive: &[u8], dest: &Path, force: bool) -> Result<UnpackReport, HostError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|e| HostError::Archive(format!("not a readable zip: {e}")))?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err(HostError::Archive(format!(
            "the archive has {} entries, more than the {MAX_ARCHIVE_ENTRIES} this host accepts",
            zip.len()
        )));
    }
    let mut report = UnpackReport { files: 0, bytes: 0 };
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| HostError::Archive(format!("cannot read entry {index}: {e}")))?;
        let name = entry.name().to_string();
        // A zip entry says what it is; a link is not content and could point
        // outside the workspace, so it is refused rather than followed.
        if entry.is_dir() {
            let out = safe_relative(&name, dest)?;
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if is_link_mode(entry.unix_mode()) {
            return Err(HostError::Archive(format!(
                "entry is a link, which an import does not follow: {name}"
            )));
        }
        let out = safe_relative(&name, dest)?;
        write_entry(&out, &mut entry, force)?;
        report.files += 1;
        report.bytes += entry.size();
        check_unpacked(&report)?;
    }
    Ok(report)
}

/// Unpack a tar, gzipped or not. `gzipped` is the only difference between the two
/// formats: everything after the reader — the entry checks, the ordering, the
/// containment — is one implementation, so a plain `.tar` cannot be the one that
/// forgets a guard.
fn unpack_tar(
    archive: &[u8],
    dest: &Path,
    force: bool,
    gzipped: Option<()>,
) -> Result<UnpackReport, HostError> {
    let reader: Box<dyn Read> = match gzipped {
        Some(()) => Box::new(flate2::read::GzDecoder::new(Cursor::new(archive))),
        None => Box::new(Cursor::new(archive)),
    };
    let mut tar = tar::Archive::new(reader);
    let entries = tar
        .entries()
        .map_err(|e| HostError::Archive(format!("not a readable tar.gz: {e}")))?;
    let mut report = UnpackReport { files: 0, bytes: 0 };
    for entry in entries {
        let mut entry =
            entry.map_err(|e| HostError::Archive(format!("cannot read tar entry: {e}")))?;
        let kind = entry.header().entry_type();
        // Only files and directories. A tar can also carry symlinks, hard links,
        // device nodes and fifos, and none of them is project content: a link is a
        // way to write outside the destination while looking like an entry inside
        // it, and the rest are not files at all.
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(HostError::Archive(
                "entry is a link, which an import does not follow".to_string(),
            ));
        }
        if !kind.is_file() && !kind.is_dir() {
            return Err(HostError::Archive(format!(
                "entry is not a file or a directory ({kind:?})"
            )));
        }
        let name = entry
            .path()
            .map_err(|e| HostError::Archive(format!("cannot read entry path: {e}")))?
            .to_string_lossy()
            .to_string();
        let out = safe_relative(&name, dest)?;
        if kind.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        let size = entry.header().size().unwrap_or(0);
        write_entry(&out, &mut entry, force)?;
        report.files += 1;
        report.bytes += size;
        check_unpacked(&report)?;
    }
    Ok(report)
}

/// Is this Unix mode one of the link kinds? (Zip entries carry a mode, and on a
/// Windows-produced archive it is usually absent.)
fn is_link_mode(mode: Option<u32>) -> bool {
    const S_IFMT: u32 = 0o170000;
    const S_IFLNK: u32 = 0o120000;
    matches!(mode, Some(mode) if mode & S_IFMT == S_IFLNK)
}

/// Write one entry, honouring `force` and never leaving a half-written file.
fn write_entry<R: Read>(out: &Path, entry: &mut R, force: bool) -> Result<(), HostError> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if out.exists() && !force {
        return Err(HostError::WorkspaceEntryExists(
            out.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| out.display().to_string()),
        ));
    }
    // A temp file next to the target, then a rename: an interrupted import leaves
    // the workspace as it was, not with a half-written file in it.
    let tmp = out.with_extension(format!(
        "{}.part",
        out.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    let mut file = std::fs::File::create(&tmp)?;
    if let Err(e) = std::io::copy(entry, &mut file) {
        let _ = std::fs::remove_file(&tmp);
        return Err(HostError::Io(e));
    }
    drop(file);
    std::fs::rename(&tmp, out).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

/// Stop an archive that unpacks to far more than it should.
fn check_unpacked(report: &UnpackReport) -> Result<(), HostError> {
    if report.bytes > MAX_UNPACKED_BYTES {
        return Err(HostError::Archive(format!(
            "the archive unpacks to more than {MAX_UNPACKED_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Pack the workspace into a `tar.gz`.
///
/// The host's own state (`.riscdom/`) is left out: it is not the project, and a
/// project archive that carried an audit DB and a snapshot cache would be both
/// surprising and large. An empty workspace packs to a valid, empty archive — the
/// answer to "export this project" is never an error because there is nothing in it.
pub fn pack_workspace(root: &Path) -> Result<Vec<u8>, HostError> {
    let mut out = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        builder.follow_symlinks(false);
        if root.exists() {
            add_dir(&mut builder, root, root)?;
        }
        let encoder = builder
            .into_inner()
            .map_err(|e| HostError::Archive(format!("cannot finish the archive: {e}")))?;
        encoder
            .finish()
            .map_err(|e| HostError::Archive(format!("cannot finish the archive: {e}")))?;
    }
    Ok(out)
}

/// Append `dir`, recursively, skipping the host's state directory.
fn add_dir<W: Write>(
    builder: &mut tar::Builder<W>,
    root: &Path,
    dir: &Path,
) -> Result<(), HostError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if dir == root && name == HOST_STATE_DIR {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| HostError::Archive(e.to_string()))?
            .to_path_buf();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            builder
                .append_dir(&rel, &path)
                .map_err(|e| HostError::Archive(format!("cannot add {}: {e}", rel.display())))?;
            add_dir(builder, root, &path)?;
        } else if meta.is_file() {
            builder
                .append_file(&rel, &mut std::fs::File::open(&path)?)
                .map_err(|e| HostError::Archive(format!("cannot add {}: {e}", rel.display())))?;
        }
        // A link is neither: it is skipped rather than followed or stored, so an
        // exported archive never carries a path that points somewhere else.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "riscdom-workspace-io-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A zip with the given `(name, contents)` entries, built by hand.
    fn a_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, content) in entries {
                writer.start_file(*name, options).expect("start");
                writer.write_all(content).expect("write");
            }
            writer.finish().expect("finish");
        }
        out.into_inner()
    }

    /// A tar.gz with the given `(name, contents)` entries, built by hand.
    fn a_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::fast());
            let mut builder = tar::Builder::new(encoder);
            for (name, content) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, *name, *content)
                    .expect("append");
            }
            let encoder = builder.into_inner().expect("inner");
            encoder.finish().expect("finish gz");
        }
        out
    }

    /// The 512-byte header a tar entry is, written **verbatim**.
    ///
    /// The `tar` crate refuses to *write* a traversing name (it validates the path
    /// it is handed), and a hostile archive is exactly the one that never went
    /// through a well-behaved builder. So this writes the header itself: name,
    /// mode, size, checksum, content padded to a block, then the two zero blocks
    /// that end a tar. It is what a hand-crafted upload looks like.
    fn a_tar_with(name: &str, content: &[u8], entry_type: tar::EntryType) -> Vec<u8> {
        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        assert!(
            name_bytes.len() <= 100,
            "a tar name field is 100 bytes: {name}"
        );
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        let octal = |value: u64, width: usize| format!("{:0>width$o}\0", value, width = width - 1);
        header[100..108].copy_from_slice(octal(0o644, 8).as_bytes());
        header[108..116].copy_from_slice(octal(0, 8).as_bytes());
        header[116..124].copy_from_slice(octal(0, 8).as_bytes());
        header[124..136].copy_from_slice(octal(content.len() as u64, 12).as_bytes());
        header[136..148].copy_from_slice(octal(0, 12).as_bytes());
        header[148..156].copy_from_slice(b"        ");
        header[156] = match entry_type {
            tar::EntryType::Directory => b'5',
            tar::EntryType::Symlink => b'2',
            tar::EntryType::Link => b'1',
            _ => b'0',
        };
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|b| *b as u32).sum();
        header[148..156].copy_from_slice(format!("{:06o}\0 ", checksum).as_bytes());

        let mut tar_bytes = Vec::new();
        tar_bytes.extend_from_slice(&header);
        tar_bytes.extend_from_slice(content);
        tar_bytes.resize(tar_bytes.len().div_ceil(512) * 512, 0);
        tar_bytes.extend_from_slice(&[0u8; 1024]);
        tar_bytes
    }

    /// The same, gzipped — what a hostile `.tar.gz` upload looks like.
    fn a_hostile_tar_gz(name: &str, content: &[u8], entry_type: tar::EntryType) -> Vec<u8> {
        let tar_bytes = a_tar_with(name, content, entry_type);
        let mut out = Vec::new();
        {
            let mut encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::fast());
            encoder.write_all(&tar_bytes).expect("gzip");
            encoder.finish().expect("finish gz");
        }
        out
    }

    #[test]
    fn a_zip_lands_in_the_destination() {
        let dest = unique_dir("zip");
        let bytes = a_zip(&[
            ("hello.c", b"int main(void){return 0;}"),
            ("src/nested.c", b"nested"),
        ]);
        let report = unpack_archive(&bytes, &dest, ArchiveFormat::Zip, false).expect("unpack");
        assert_eq!(report.files, 2, "{report:?}");
        assert!(report.bytes > 0);
        assert_eq!(
            std::fs::read_to_string(dest.join("hello.c")).expect("read"),
            "int main(void){return 0;}"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("src").join("nested.c")).expect("read"),
            "nested"
        );
    }

    #[test]
    fn a_tar_gz_lands_in_the_destination() {
        let dest = unique_dir("tar");
        let bytes = a_tar_gz(&[("a.c", b"a"), ("deep/pkg/b.c", b"b")]);
        assert_eq!(
            ArchiveFormat::from_magic(&bytes),
            Some(ArchiveFormat::TarGz)
        );
        let report = unpack_archive(&bytes, &dest, ArchiveFormat::TarGz, false).expect("unpack");
        assert_eq!(report.files, 2, "{report:?}");
        assert_eq!(
            std::fs::read_to_string(dest.join("a.c")).expect("read"),
            "a"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("deep").join("pkg").join("b.c")).expect("read"),
            "b"
        );
    }

    #[test]
    fn the_hosts_own_state_is_neither_packed_nor_unpacked() {
        let root = unique_dir("state");
        std::fs::create_dir_all(root.join(HOST_STATE_DIR).join("snapshots")).unwrap();
        std::fs::write(root.join(HOST_STATE_DIR).join("audit.db"), b"host state").unwrap();
        std::fs::write(root.join("project.c"), b"content").unwrap();

        // Packed: the project is there, the state is not.
        let packed = pack_workspace(&root).expect("pack");
        let dest = unique_dir("state-out");
        unpack_archive(&packed, &dest, ArchiveFormat::TarGz, false).expect("unpack");
        assert!(dest.join("project.c").exists());
        assert!(
            !dest.join(HOST_STATE_DIR).exists(),
            "the export carried the host's state"
        );

        // Unpacked: an archive that carries it is refused, not merged.
        let hostile = a_hostile_tar_gz(".riscdom/audit.db", b"pwned", tar::EntryType::Regular);
        let err = unpack_archive(
            &hostile,
            &unique_dir("hostile"),
            ArchiveFormat::TarGz,
            false,
        )
        .expect_err("refused");
        assert!(matches!(err, HostError::Archive(_)), "{err}");
        assert!(err.to_string().contains(HOST_STATE_DIR), "{err}");
    }

    #[test]
    fn a_traversing_entry_is_refused() {
        for name in ["../escape.c", "/etc/passwd", "a/../../escape.c"] {
            let dest = unique_dir("escape");
            let bytes = a_hostile_tar_gz(name, b"x", tar::EntryType::Regular);
            let err =
                unpack_archive(&bytes, &dest, ArchiveFormat::TarGz, false).expect_err("refused");
            assert!(matches!(err, HostError::Archive(_)), "{name}: {err}");
            assert!(
                !dest.join("escape.c").exists(),
                "{name} escaped the destination"
            );
            let _ = std::fs::remove_dir_all(&dest);
        }
        // A backslash is a separator in a Windows-produced archive.
        let dest = unique_dir("escape-win");
        let bytes = a_hostile_tar_gz("..\\escape.c", b"x", tar::EntryType::Regular);
        let err = unpack_archive(&bytes, &dest, ArchiveFormat::TarGz, false).expect_err("refused");
        assert!(matches!(err, HostError::Archive(_)), "{err}");
    }

    #[test]
    fn a_link_is_refused() {
        // A symlink pointing outside the destination: the classic way an archive
        // writes somewhere it should not.
        let dest = unique_dir("link");
        let bytes = a_hostile_tar_gz("link", b"", tar::EntryType::Symlink);
        let err = unpack_archive(&bytes, &dest, ArchiveFormat::TarGz, false).expect_err("refused");
        assert!(matches!(err, HostError::Archive(_)), "{err}");

        // A hard link is the same idea with no dangling-target excuse.
        let dest = unique_dir("hardlink");
        let bytes = a_hostile_tar_gz("hard", b"", tar::EntryType::Link);
        let err = unpack_archive(&bytes, &dest, ArchiveFormat::TarGz, false).expect_err("refused");
        assert!(matches!(err, HostError::Archive(_)), "{err}");
    }

    #[test]
    fn an_existing_file_is_kept_unless_force_says_otherwise() {
        let dest = unique_dir("exists");
        std::fs::write(dest.join("keep.c"), b"mine").unwrap();
        let bytes = a_tar_gz(&[("keep.c", b"theirs")]);

        let err = unpack_archive(&bytes, &dest, ArchiveFormat::TarGz, false).expect_err("refused");
        assert!(matches!(err, HostError::WorkspaceEntryExists(_)), "{err}");
        assert_eq!(
            std::fs::read_to_string(dest.join("keep.c")).expect("read"),
            "mine",
            "a refused import changed the file anyway"
        );

        let report = unpack_archive(&bytes, &dest, ArchiveFormat::TarGz, true).expect("forced");
        assert_eq!(report.files, 1);
        assert_eq!(
            std::fs::read_to_string(dest.join("keep.c")).expect("read"),
            "theirs"
        );
    }

    #[test]
    fn an_empty_workspace_packs_to_a_valid_empty_archive() {
        let root = unique_dir("empty");
        let packed = pack_workspace(&root).expect("pack");
        assert_eq!(
            ArchiveFormat::from_magic(&packed),
            Some(ArchiveFormat::TarGz)
        );
        let dest = unique_dir("empty-out");
        let report = unpack_archive(&packed, &dest, ArchiveFormat::TarGz, false).expect("unpack");
        assert_eq!(report.files, 0, "{report:?}");
    }

    #[test]
    fn a_round_trip_keeps_every_file_and_its_bytes() {
        let root = unique_dir("round");
        std::fs::create_dir_all(root.join("src").join("deep")).unwrap();
        std::fs::write(root.join("README.md"), b"# project\n").unwrap();
        std::fs::write(root.join("src").join("main.c"), b"int main(void){}").unwrap();
        std::fs::write(
            root.join("src").join("deep").join("link.ld"),
            b"SECTIONS {}",
        )
        .unwrap();

        let packed = pack_workspace(&root).expect("pack");
        let dest = unique_dir("round-out");
        let report = unpack_archive(&packed, &dest, ArchiveFormat::TarGz, false).expect("unpack");
        assert_eq!(report.files, 3, "{report:?}");
        for rel in ["README.md", "src/main.c", "src/deep/link.ld"] {
            assert_eq!(
                std::fs::read(dest.join(rel)).expect("read"),
                std::fs::read(root.join(rel)).expect("read"),
                "{rel}"
            );
        }
    }

    #[test]
    fn a_plain_tar_is_recognised_and_unpacked() {
        // `tar cf project.tar .` is what a user's shell produces; refusing it would
        // be pedantry, and the guards must be the ones the gzipped path uses.
        let mut tar_bytes = {
            let mut builder = tar::Builder::new(Vec::new());
            let mut header = tar::Header::new_gnu();
            header.set_size(2);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "plain.c", &b"hi"[..])
                .expect("append");
            builder.into_inner().expect("inner")
        };
        assert_eq!(
            ArchiveFormat::from_magic(&tar_bytes),
            Some(ArchiveFormat::Tar)
        );
        let dest = unique_dir("plain-tar");
        let report = unpack_archive(&tar_bytes, &dest, ArchiveFormat::Tar, false).expect("unpack");
        assert_eq!(report.files, 1, "{report:?}");
        assert_eq!(
            std::fs::read_to_string(dest.join("plain.c")).expect("read"),
            "hi"
        );

        // Same guards: a hostile entry in an uncompressed tar is refused too.
        tar_bytes = a_tar_with("../escape.c", b"x", tar::EntryType::Regular);
        let err = unpack_archive(
            &tar_bytes,
            &unique_dir("plain-tar-escape"),
            ArchiveFormat::Tar,
            false,
        )
        .expect_err("refused");
        assert!(matches!(err, HostError::Archive(_)), "{err}");
        assert_eq!(
            ArchiveFormat::from_content_type("application/x-tar"),
            Some(ArchiveFormat::Tar)
        );
    }

    #[test]
    fn the_format_is_recognised_from_the_bytes_or_the_content_type() {
        assert_eq!(
            ArchiveFormat::from_magic(&a_zip(&[("a", b"x")])),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_magic(&a_tar_gz(&[("a", b"x")])),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(ArchiveFormat::from_magic(b"not an archive"), None);
        assert_eq!(
            ArchiveFormat::from_content_type("application/zip; charset=binary"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_content_type("application/gzip"),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_content_type("application/x-tar+gzip"),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(ArchiveFormat::from_content_type("text/plain"), None);
        assert_eq!(ArchiveFormat::TarGz.content_type(), "application/gzip");
        assert_eq!(ArchiveFormat::Zip.default_file_name(), "workspace.zip");
        assert_eq!(ArchiveFormat::TarGz.default_file_name(), "workspace.tar.gz");
    }

    #[test]
    fn rubbish_is_not_an_archive() {
        let dest = unique_dir("rubbish");
        let err = unpack_archive(b"not an archive at all", &dest, ArchiveFormat::Zip, false)
            .expect_err("refused");
        assert!(matches!(err, HostError::Archive(_)), "{err}");
        let err = unpack_archive(b"not an archive at all", &dest, ArchiveFormat::TarGz, false)
            .expect_err("refused");
        assert!(matches!(err, HostError::Archive(_)), "{err}");
    }
}
