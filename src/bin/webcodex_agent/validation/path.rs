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
        resolve_under_project(project_root, raw).ok()?
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
    if text.starts_with('/')
        || Path::new(&text)
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
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
    let root = std::fs::canonicalize(project_root)
        .map_err(|_| "project root is not accessible".to_string())?;
    if !root.is_dir() {
        return Err("project root is not a directory".to_string());
    }

    let mut ancestor = root.join(relative);
    let mut missing_components = Vec::new();
    loop {
        match std::fs::symlink_metadata(&ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = ancestor
                    .file_name()
                    .ok_or_else(|| "path has no accessible ancestor".to_string())?;
                missing_components.push(component.to_os_string());
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| "path has no accessible ancestor".to_string())?
                    .to_path_buf();
            }
            Err(_) => return Err("path is not accessible".to_string()),
        }
    }

    let canonical_ancestor = std::fs::canonicalize(&ancestor)
        .map_err(|_| "path ancestor is not accessible".to_string())?;
    if !canonical_ancestor.starts_with(&root) {
        return Err("path escapes project root".to_string());
    }

    let mut candidate = canonical_ancestor;
    for component in missing_components.iter().rev() {
        candidate.push(component);
    }
    Ok(candidate)
}

/// Resolve a required working directory under the project. Unlike targets,
/// cwd must already exist and must resolve to a directory.
pub(crate) fn resolve_cwd_under_project(
    project_root: &Path,
    relative: &str,
) -> Result<PathBuf, String> {
    let cwd = resolve_under_project(project_root, relative)?;
    let metadata = std::fs::metadata(&cwd).map_err(|_| "cwd is not accessible".to_string())?;
    if !metadata.is_dir() {
        return Err("cwd is not a directory".to_string());
    }
    Ok(cwd)
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

    #[test]
    fn resolves_existing_and_nonexistent_paths_under_project() {
        let project = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(project.path()).unwrap();
        fs::create_dir_all(root.join("src/真实目录")).unwrap();
        fs::write(root.join("src/app.py"), "x = 1\n").unwrap();

        assert_eq!(
            resolve_under_project(&root, "src/app.py").unwrap(),
            root.join("src/app.py")
        );
        assert_eq!(
            resolve_under_project(&root, "missing.py").unwrap(),
            root.join("missing.py")
        );
        assert_eq!(
            resolve_under_project(&root, "src/真实目录/不存在.py").unwrap(),
            root.join("src/真实目录/不存在.py")
        );
        assert_eq!(resolve_under_project(&root, ".").unwrap(), root);
        assert!(resolve_under_project(&root, "../escape.py").is_err());
        assert!(resolve_under_project(&root, "/etc/passwd").is_err());
    }

    #[test]
    fn cwd_must_exist_and_be_a_directory() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("file.py"), "x = 1\n").unwrap();

        assert!(resolve_cwd_under_project(root, "src").is_ok());
        assert!(resolve_cwd_under_project(root, "missing").is_err());
        assert!(resolve_cwd_under_project(root, "file.py").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_allows_internal_symlinks_for_existing_and_missing_targets() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(project.path()).unwrap();
        fs::create_dir_all(root.join("real/nested")).unwrap();
        fs::write(root.join("real/existing.py"), "x = 1\n").unwrap();
        symlink(root.join("real"), root.join("inside")).unwrap();
        symlink(root.join("inside"), root.join("inside_again")).unwrap();

        assert_eq!(
            resolve_under_project(&root, "inside/existing.py").unwrap(),
            root.join("real/existing.py")
        );
        assert_eq!(
            resolve_under_project(&root, "inside/not-yet.py").unwrap(),
            root.join("real/not-yet.py")
        );
        assert_eq!(
            resolve_under_project(&root, "inside_again/nested/新.py").unwrap(),
            root.join("real/nested/新.py")
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_external_symlink_existing_target_and_cwd() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("existing.py"), "secret\n").unwrap();
        fs::create_dir(outside.path().join("cwd")).unwrap();
        symlink(outside.path(), project.path().join("outside")).unwrap();

        assert!(resolve_under_project(project.path(), "outside/existing.py").is_err());
        assert!(resolve_cwd_under_project(project.path(), "outside/cwd").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_external_symlink_with_nonexistent_leaf() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), project.path().join("outside")).unwrap();

        let result = resolve_under_project(project.path(), "outside/not-yet-existing.py");
        assert!(result.is_err(), "escaped path was accepted: {result:?}");
    }
}
