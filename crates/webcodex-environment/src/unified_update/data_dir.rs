use super::{UpdateError, UpdateResult};
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

/// Bounded diagnostics for hosts that retain data-directory failure details.
/// No variant contains filesystem paths or operating-system error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopDataDirError {
    RelativeOverride,
    Unavailable {
        reason: DesktopDataDirFailureReason,
        path_kind: DesktopDataDirPathKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopDataDirPathKind {
    Absolute,
    Relative,
}

impl DesktopDataDirPathKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Absolute => "absolute",
            Self::Relative => "relative",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopDataDirFailureReason {
    NotAbsolute,
    MissingFinalComponent,
    MissingParent,
    NoExistingAncestor,
    CannotInspect,
    NonDirectoryAncestor,
    CannotResolve,
    RootReparsePoint,
    RootNotDirectory,
}

impl DesktopDataDirFailureReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotAbsolute => "Desktop data directory is not absolute",
            Self::MissingFinalComponent => {
                "Desktop data directory must have an application-owned final component"
            }
            Self::MissingParent => "Desktop data directory has no parent directory",
            Self::NoExistingAncestor => "Desktop data directory has no existing ancestor",
            Self::CannotInspect => "Desktop cannot inspect its app-data directory",
            Self::NonDirectoryAncestor => {
                "Desktop data directory resolves through a non-directory ancestor"
            }
            Self::CannotResolve => "Desktop cannot resolve its app-data directory",
            Self::RootReparsePoint => "Desktop data directory itself is a reparse point",
            Self::RootNotDirectory => "Desktop data directory is not a directory",
        }
    }
}

pub fn resolve(default: PathBuf) -> UpdateResult<DesktopDataDir> {
    resolve_with_override(default, std::env::var_os(DESKTOP_DATA_DIR_ENV))
}

pub fn resolve_with_override(
    default: PathBuf,
    override_value: Option<std::ffi::OsString>,
) -> UpdateResult<DesktopDataDir> {
    resolve_with_override_detailed(default, override_value)
        .map_err(|_| UpdateError::CacheUnavailable)
}

/// Uses the same path authority as the updater API while retaining safe host diagnostics.
pub fn resolve_with_override_detailed(
    default: PathBuf,
    override_value: Option<std::ffi::OsString>,
) -> Result<DesktopDataDir, DesktopDataDirError> {
    let (logical, source) = match override_value {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(DesktopDataDirError::RelativeOverride);
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
fn resolve_physical_path(path: &Path) -> Result<PathBuf, DesktopDataDirError> {
    if !path.is_absolute() {
        return Err(invalid_data_dir(
            path,
            DesktopDataDirFailureReason::NotAbsolute,
        ));
    }

    // Resolve only ancestors, not the final Desktop-owned directory itself.
    // This is the important security boundary: a redirected Windows profile or
    // LocalAppData ancestor may legitimately be a Junction, but an existing
    // WebCodex data root that was replaced with a Junction/symlink must remain
    // visible to the credential-path safety checks instead of being
    // canonicalized away here.
    let leaf = path.file_name().ok_or_else(|| {
        invalid_data_dir(path, DesktopDataDirFailureReason::MissingFinalComponent)
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid_data_dir(path, DesktopDataDirFailureReason::MissingParent))?;

    let mut resolved_parent = resolve_existing_ancestor(parent, path)?;
    resolved_parent.push(leaf);
    validate_effective_root(&resolved_parent, path)?;
    Ok(resolved_parent)
}

#[cfg(windows)]
fn resolve_existing_ancestor(path: &Path, original: &Path) -> Result<PathBuf, DesktopDataDirError> {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        match std::fs::symlink_metadata(&existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    invalid_data_dir(original, DesktopDataDirFailureReason::NoExistingAncestor)
                })?;
                tail.push(name.to_os_string());
                if !existing.pop() {
                    return Err(invalid_data_dir(
                        original,
                        DesktopDataDirFailureReason::NoExistingAncestor,
                    ));
                }
            }
            Err(_) => {
                return Err(invalid_data_dir(
                    original,
                    DesktopDataDirFailureReason::CannotInspect,
                ))
            }
        }
    }

    let metadata = std::fs::metadata(&existing)
        .map_err(|_| invalid_data_dir(original, DesktopDataDirFailureReason::CannotInspect))?;
    if !metadata.is_dir() {
        return Err(invalid_data_dir(
            original,
            DesktopDataDirFailureReason::NonDirectoryAncestor,
        ));
    }

    let mut resolved = std::fs::canonicalize(&existing)
        .map_err(|_| invalid_data_dir(original, DesktopDataDirFailureReason::CannotResolve))?;
    resolved = normalize_windows_canonical_path(resolved);
    for component in tail.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

#[cfg(windows)]
fn validate_effective_root(path: &Path, original: &Path) -> Result<(), DesktopDataDirError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_symlink() => Err(invalid_data_dir(
            original,
            DesktopDataDirFailureReason::RootReparsePoint,
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(invalid_data_dir(
            original,
            DesktopDataDirFailureReason::RootNotDirectory,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(invalid_data_dir(
            original,
            DesktopDataDirFailureReason::CannotInspect,
        )),
    }
}
#[cfg(not(windows))]
fn resolve_physical_path(path: &Path) -> Result<PathBuf, DesktopDataDirError> {
    if !path.is_absolute() {
        return Err(invalid_data_dir(
            path,
            DesktopDataDirFailureReason::NotAbsolute,
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

fn invalid_data_dir(path: &Path, reason: DesktopDataDirFailureReason) -> DesktopDataDirError {
    DesktopDataDirError::Unavailable {
        reason,
        path_kind: if path.is_absolute() {
            DesktopDataDirPathKind::Absolute
        } else {
            DesktopDataDirPathKind::Relative
        },
    }
}
pub fn default_desktop_data_dir() -> UpdateResult<PathBuf> {
    let default = dirs::data_local_dir()
        .ok_or(UpdateError::CacheUnavailable)?
        .join("dev.webcodex.desktop");
    resolve(default).map(|dir| dir.effective)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_errors_preserve_path_kind_while_updater_errors_remain_static() {
        let default = PathBuf::from("private-relative-default");
        assert_eq!(
            resolve_with_override_detailed(default.clone(), None).unwrap_err(),
            DesktopDataDirError::Unavailable {
                reason: DesktopDataDirFailureReason::NotAbsolute,
                path_kind: DesktopDataDirPathKind::Relative,
            }
        );
        assert!(matches!(
            resolve_with_override(default.clone(), None),
            Err(UpdateError::CacheUnavailable)
        ));
        let override_value = Some(std::ffi::OsString::from("private-relative-override"));
        assert_eq!(
            resolve_with_override_detailed(default.clone(), override_value.clone()).unwrap_err(),
            DesktopDataDirError::RelativeOverride
        );
        assert!(matches!(
            resolve_with_override(default, override_value),
            Err(UpdateError::CacheUnavailable)
        ));
    }

    #[test]
    fn detailed_and_updater_apis_use_the_same_effective_root() {
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("default");
        let override_value = Some(temp.path().join("override").into_os_string());
        let detailed =
            resolve_with_override_detailed(default.clone(), override_value.clone()).unwrap();
        let updater = resolve_with_override(default, override_value).unwrap();
        assert_eq!(detailed.effective, updater.effective);
        assert_eq!(detailed.source, DesktopDataDirSource::Environment);
        assert_eq!(detailed.source, updater.source);
        assert_eq!(
            detailed.physical_resolution_changed,
            updater.physical_resolution_changed
        );
    }
}
