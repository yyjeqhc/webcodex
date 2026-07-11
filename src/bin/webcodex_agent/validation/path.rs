//! Project-relative path validation and absolute-path relativization.

use crate::validation_bridge::MAX_PATH_CHARS;
use std::path::{Component, Path, PathBuf};

/// Relativize an absolute (or absolute-looking) diagnostic path to the project
/// root. Returns `None` when the path is outside the project or cannot be
/// normalized.
pub(crate) fn relativize_path(project_root: &Path, raw: &str) -> Option<String> {
    if raw.is_empty() || raw.contains('\0') {
        return None;
    }
    let path = Path::new(raw);
    let canonical = if path.is_absolute() {
        std::fs::canonicalize(path).ok().or_else(|| {
            // File may not exist; normalize parent if possible.
            let parent = path.parent()?;
            let file = path.file_name()?;
            let parent_canon = std::fs::canonicalize(parent).ok()?;
            Some(parent_canon.join(file))
        })?
    } else {
        // Treat relative paths as project-relative already, but still reject `..`.
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return None;
        }
        project_root.join(path)
    };

    let root = std::fs::canonicalize(project_root).ok()?;
    if !canonical.starts_with(&root) {
        return None;
    }
    let rel = canonical.strip_prefix(&root).ok()?;
    let text = rel.to_string_lossy().replace('\\', "/");
    if text.is_empty() || text == "." {
        return None;
    }
    if text.chars().count() > MAX_PATH_CHARS {
        return None;
    }
    // Reject residual absolute or escape forms.
    if text.starts_with('/') || text.contains("..") {
        return None;
    }
    Some(text)
}

/// Resolve a validated project-relative path under the project root.
pub(crate) fn resolve_under_project(
    project_root: &Path,
    relative: &str,
) -> Result<PathBuf, String> {
    crate::validation_bridge::validate_project_relative_path(relative)?;
    let joined = project_root.join(relative);
    let root = std::fs::canonicalize(project_root)
        .map_err(|_| "project root is not accessible".to_string())?;
    // Canonicalize when the path exists; otherwise normalize lexically.
    let candidate = if joined.exists() {
        std::fs::canonicalize(&joined).map_err(|_| "path is not accessible".to_string())?
    } else {
        normalize_lexically(&joined)
    };
    if !candidate.starts_with(&root) {
        return Err("path escapes project root".to_string());
    }
    Ok(candidate)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn relativize_inside_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        let file = root.join("src/app.py");
        fs::write(&file, "x = 1\n").unwrap();
        let abs = fs::canonicalize(&file).unwrap();
        let rel = relativize_path(root, abs.to_str().unwrap()).unwrap();
        assert_eq!(rel, "src/app.py");
    }

    #[test]
    fn relativize_outside_project_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("secret.py");
        fs::write(&file, "x\n").unwrap();
        let abs = fs::canonicalize(&file).unwrap();
        assert!(relativize_path(root, abs.to_str().unwrap()).is_none());
    }
}
