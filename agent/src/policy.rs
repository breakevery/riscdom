//! Workspace capability policy.
//!
//! Every file operation the AI requests must pass through here. The policy is
//! **deny by default**: a path is only accepted if it resolves inside the
//! workspace root, contains no `..` traversal, and (for writes) has an allowed
//! source extension.

use crate::error::AgentError;
use std::path::{Component, Path, PathBuf};

/// Rules for what the AI may read/write inside its workspace.
pub struct WorkspacePolicy {
    /// Workspace root (normalised, absolute).
    pub root: PathBuf,
    /// Allowed write extensions, e.g. `[".c", ".h", ".S", ".s"]`.
    pub allowed_extensions: Vec<String>,
}

impl WorkspacePolicy {
    /// Create a policy rooted at `root` with the default source-extension list.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: normalize(&root.into()),
            allowed_extensions: vec![".c".into(), ".h".into(), ".S".into(), ".s".into()],
        }
    }

    /// Validate a path for writing and return its resolved absolute path.
    pub fn check_write(&self, path: &Path) -> Result<PathBuf, AgentError> {
        let abs = self.resolve(path)?;
        let ext = abs
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{e}"))
            .unwrap_or_default();
        if !self
            .allowed_extensions
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&ext))
        {
            return Err(AgentError::PolicyDenied(format!(
                "extension not allowed: {}",
                if ext.is_empty() { "(none)" } else { &ext }
            )));
        }
        Ok(abs)
    }

    /// Validate a path for reading and return its resolved absolute path.
    pub fn check_read(&self, path: &Path) -> Result<PathBuf, AgentError> {
        self.resolve(path)
    }

    /// Shared resolution: reject traversal, resolve against the root, and
    /// require the result to stay inside the root.
    fn resolve(&self, path: &Path) -> Result<PathBuf, AgentError> {
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(AgentError::PolicyDenied(format!(
                "path traversal not allowed: {}",
                path.display()
            )));
        }

        let abs = if looks_absolute(path) {
            normalize(path)
        } else {
            normalize(&self.root.join(path))
        };

        if !abs.starts_with(&self.root) {
            return Err(AgentError::PolicyDenied(format!(
                "path outside workspace: {}",
                abs.display()
            )));
        }
        Ok(abs)
    }
}

/// Is `path` absolute (including drive-less `/foo` on Windows)?
fn looks_absolute(path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }
    matches!(
        path.components().next(),
        Some(Component::RootDir) | Some(Component::Prefix(_))
    )
}

/// Lexically normalise a path (no filesystem access, so it works for files
/// that do not exist yet).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn policy() -> (WorkspacePolicy, PathBuf) {
        // Unique per process: a fixed shared root made two test binaries fight
        // over the same directory (v0.4 batch 3-followup).
        let root = std::env::temp_dir().join(format!("riscdom-policy-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        (WorkspacePolicy::new(root.clone()), root)
    }

    #[test]
    fn allows_source_inside_root() {
        let (p, root) = policy();
        let ok = p.check_write(Path::new("hello.c")).expect("allow hello.c");
        assert_eq!(ok, normalize(&root.join("hello.c")));
        assert!(p.check_write(Path::new("sub/dir/a.S")).is_ok());
        assert!(p.check_write(Path::new("b.s")).is_ok());
        assert!(p.check_write(Path::new("h.h")).is_ok());
    }

    #[test]
    fn rejects_traversal() {
        let (p, _) = policy();
        let err = p.check_write(Path::new("../etc/passwd")).unwrap_err();
        assert!(matches!(err, AgentError::PolicyDenied(_)), "{err:?}");
    }

    #[test]
    fn rejects_disallowed_extension() {
        let (p, _) = policy();
        assert!(matches!(
            p.check_write(Path::new("script.py")),
            Err(AgentError::PolicyDenied(_))
        ));
        assert!(matches!(
            p.check_write(Path::new("noext")),
            Err(AgentError::PolicyDenied(_))
        ));
    }

    #[test]
    fn rejects_outside_root() {
        let (p, _) = policy();
        // Drive-less absolute path on Windows, or genuinely absolute elsewhere.
        let outside = PathBuf::from("/etc/passwd");
        assert!(matches!(
            p.check_read(&outside),
            Err(AgentError::PolicyDenied(_))
        ));
    }
}
