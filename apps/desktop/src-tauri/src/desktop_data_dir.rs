use crate::error::{DesktopError, DesktopResult};
use std::path::{Path, PathBuf};

pub const DESKTOP_DATA_DIR_ENV: &str = "WEBCODEX_DESKTOP_DATA_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopDataDirSource {
    Tauri,
    Environment,
}

impl DesktopDataDirSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tauri => "tauri",
            Self::Environment => "environment",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesktopDataDir {
    pub effective: PathBuf,
    pub source: DesktopDataDirSource,
    pub physical_resolution_changed: bool,
}

pub fn resolve(default: PathBuf) -> DesktopResult<DesktopDataDir> {
    resolve_with_override(default, std::env::var_os(DESKTOP_DATA_DIR_ENV))
}

fn resolve_with_override(
    default: PathBuf,
    override_value: Option<std::ffi::OsString>,
) -> DesktopResult<DesktopDataDir> {
    let (logical, source) = match override_value {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(DesktopError::new(
                    "desktop_data_dir_invalid",
                    format!("{DESKTOP_DATA_DIR_ENV} must be an absolute path"),
                    "Set the override to an absolute filesystem path or remove it.",
                ));
            }
            (path, DesktopDataDirSource::Environment)
        }
        None => (default, DesktopDataDirSource::Tauri),
    };

    let effective = resolve_physical_path(&logical)?;
    let physical_resolution_changed =
        !webcodex_runner_config::paths::paths_equal(&logical, &effective);
    Ok(DesktopDataDir {
        effective,
        source,
        physical_resolution_changed,
    })
}
#[cfg(windows)]
fn resolve_physical_path(path: &Path) -> DesktopResult<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid_data_dir(
            path,
            "Desktop data directory is not absolute",
        ));
    }

    // Resolve only ancestors, not the final Desktop-owned directory itself.
    // This is the important security boundary: a redirected Windows profile or
    // LocalAppData ancestor may legitimately be a Junction, but an existing
    // WebCodex data root that was replaced with a Junction/symlink must remain
    // visible to the credential-path safety checks instead of being
    // canonicalized away here.
    let leaf = path.file_name().ok_or_else(|| {
        invalid_data_dir(
            path,
            "Desktop data directory must have an application-owned final component",
        )
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid_data_dir(path, "Desktop data directory has no parent directory"))?;

    let mut resolved_parent = resolve_existing_ancestor(parent, path)?;
    resolved_parent.push(leaf);
    validate_effective_root(&resolved_parent, path)?;
    Ok(resolved_parent)
}

#[cfg(windows)]
fn resolve_existing_ancestor(path: &Path, original: &Path) -> DesktopResult<PathBuf> {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        match std::fs::symlink_metadata(&existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    invalid_data_dir(original, "Desktop data directory has no existing ancestor")
                })?;
                tail.push(name.to_os_string());
                if !existing.pop() {
                    return Err(invalid_data_dir(
                        original,
                        "Desktop data directory has no existing ancestor",
                    ));
                }
            }
            Err(_) => {
                return Err(invalid_data_dir(
                    original,
                    "Desktop cannot inspect its app-data directory",
                ))
            }
        }
    }

    let metadata = std::fs::metadata(&existing)
        .map_err(|_| invalid_data_dir(original, "Desktop cannot inspect its app-data directory"))?;
    if !metadata.is_dir() {
        return Err(invalid_data_dir(
            original,
            "Desktop data directory resolves through a non-directory ancestor",
        ));
    }

    let mut resolved = std::fs::canonicalize(&existing)
        .map_err(|_| invalid_data_dir(original, "Desktop cannot resolve its app-data directory"))?;
    resolved = normalize_windows_canonical_path(resolved);
    for component in tail.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

#[cfg(windows)]
fn validate_effective_root(path: &Path, original: &Path) -> DesktopResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_symlink() => Err(invalid_data_dir(
            original,
            "Desktop data directory itself is a reparse point",
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(invalid_data_dir(
            original,
            "Desktop data directory is not a directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(invalid_data_dir(
            original,
            "Desktop cannot inspect its app-data directory",
        )),
    }
}
#[cfg(not(windows))]
fn resolve_physical_path(path: &Path) -> DesktopResult<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid_data_dir(
            path,
            "Desktop data directory is not absolute",
        ));
    }
    Ok(path.to_path_buf())
}

#[cfg(windows)]
fn normalize_windows_canonical_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = value.strip_prefix(r"\?\") {
        let bytes = rest.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/')
        {
            return PathBuf::from(rest);
        }
    }
    path
}

fn invalid_data_dir(path: &Path, message: &'static str) -> DesktopError {
    DesktopError::new(
        "desktop_data_dir_unavailable",
        message,
        "Check the Desktop app-data location and local filesystem permissions, then retry.",
    )
    .with_details(serde_json::json!({
        "path_kind": if path.is_absolute() { "absolute" } else { "relative" }
    }))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::Command;

    fn create_junction(link: &Path, target: &Path) {
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .expect("launch mklink");
        assert!(status.success(), "mklink /J failed with {status}");
    }

    #[test]
    fn explicit_override_is_distinct_and_relative_override_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("default");
        let override_path = temp.path().join("override");

        let resolved =
            resolve_with_override(default, Some(override_path.clone().into_os_string())).unwrap();
        assert_eq!(resolved.source, DesktopDataDirSource::Environment);
        assert!(webcodex_runner_config::paths::paths_equal(
            &resolved.effective,
            &override_path
        ));

        let error = resolve_with_override(
            temp.path().join("default"),
            Some(std::ffi::OsString::from("relative-data-root")),
        )
        .unwrap_err();
        assert_eq!(error.code, "desktop_data_dir_invalid");
    }

    #[test]
    fn final_desktop_data_root_junction_is_rejected_instead_of_canonicalized_away() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        let outside = temp.path().join("outside");
        let data_root = parent.join("WebCodex");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        create_junction(&data_root, &outside);

        let error = resolve_physical_path(&data_root).unwrap_err();
        assert_eq!(error.code, "desktop_data_dir_unavailable");
        assert!(error.message.contains("reparse point"));
    }

    #[test]
    fn ordinary_existing_ancestor_keeps_equivalent_path_and_appends_missing_tail() {
        let temp = tempfile::tempdir().unwrap();
        let logical = temp.path().join("missing").join("nested");
        let resolved = resolve_physical_path(&logical).unwrap();
        let expected = normalize_windows_canonical_path(temp.path().canonicalize().unwrap())
            .join("missing")
            .join("nested");
        assert!(webcodex_runner_config::paths::paths_equal(
            &resolved, &expected
        ));
    }

    #[test]
    fn ancestor_junction_resolves_to_physical_target_with_missing_tail() {
        let temp = tempfile::tempdir().unwrap();
        let physical = temp.path().join("physical");
        let logical_root = temp.path().join("logical");
        std::fs::create_dir(&physical).unwrap();
        create_junction(&logical_root, &physical);

        let logical = logical_root.join("AppData").join("Local").join("WebCodex");
        let resolved = resolve_physical_path(&logical).unwrap();
        let expected = normalize_windows_canonical_path(physical.canonicalize().unwrap())
            .join("AppData")
            .join("Local")
            .join("WebCodex");
        assert!(webcodex_runner_config::paths::paths_equal(
            &resolved, &expected
        ));
        assert!(!webcodex_runner_config::paths::paths_equal(
            &resolved, &logical
        ));
    }

    #[test]
    fn dangling_junction_ancestor_fails_closed_instead_of_becoming_missing_tail() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let junction = temp.path().join("junction");
        std::fs::create_dir_all(&target).unwrap();
        create_junction(&junction, &target);
        std::fs::remove_dir(&target).unwrap();

        let logical = junction.join("AppData").join("Local").join("WebCodex");
        let error = resolve_physical_path(&logical).unwrap_err();
        assert_eq!(error.code, "desktop_data_dir_unavailable");
    }

    #[test]
    fn nested_junctions_resolve_before_reappending_missing_tail() {
        let temp = tempfile::tempdir().unwrap();
        let physical = temp.path().join("physical");
        let first = temp.path().join("first");
        let second_target = physical.join("profile");
        let second = physical.join("redirected-profile");
        std::fs::create_dir_all(&second_target).unwrap();
        create_junction(&first, &physical);
        create_junction(&second, &second_target);

        let logical = first
            .join("redirected-profile")
            .join("AppData")
            .join("Local")
            .join("WebCodex");
        let resolved = resolve_physical_path(&logical).unwrap();
        let expected = normalize_windows_canonical_path(second_target.canonicalize().unwrap())
            .join("AppData")
            .join("Local")
            .join("WebCodex");
        assert!(webcodex_runner_config::paths::paths_equal(
            &resolved, &expected
        ));
    }

    #[test]
    fn local_runtime_paths_are_derived_from_the_effective_physical_root() {
        let temp = tempfile::tempdir().unwrap();
        let physical = temp.path().join("physical");
        let logical_root = temp.path().join("logical");
        std::fs::create_dir(&physical).unwrap();
        create_junction(&logical_root, &physical);

        let effective = resolve_physical_path(&logical_root.join("WebCodex")).unwrap();
        let (env_file, server_data) = crate::state::local_runtime_paths(&effective);
        assert!(webcodex_runner_config::paths::paths_equal(
            &env_file,
            &physical.join("WebCodex/runtime/local/webcodex.env")
        ));
        assert!(webcodex_runner_config::paths::paths_equal(
            &server_data,
            &physical.join("WebCodex/runtime/local/data")
        ));
    }
}
