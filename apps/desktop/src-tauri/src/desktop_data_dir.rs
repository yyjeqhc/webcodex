use crate::error::{DesktopError, DesktopResult};
#[cfg(all(test, windows))]
use std::path::Path;
use std::path::PathBuf;

pub use webcodex_environment::unified_update::DESKTOP_DATA_DIR_ENV;

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
    let resolved =
        webcodex_environment::unified_update::desktop_data_dir::resolve_with_override_detailed(
            default,
            override_value,
        )
        .map_err(project_data_dir_error)?;
    Ok(DesktopDataDir {
        effective: resolved.effective,
        source: match resolved.source {
            webcodex_environment::unified_update::desktop_data_dir::DesktopDataDirSource::Tauri => DesktopDataDirSource::Tauri,
            webcodex_environment::unified_update::desktop_data_dir::DesktopDataDirSource::Environment => DesktopDataDirSource::Environment,
        },
        physical_resolution_changed: resolved.physical_resolution_changed,
    })
}

fn project_data_dir_error(
    error: webcodex_environment::unified_update::desktop_data_dir::DesktopDataDirError,
) -> DesktopError {
    use webcodex_environment::unified_update::desktop_data_dir::DesktopDataDirError;
    match error {
        DesktopDataDirError::RelativeOverride => DesktopError::new(
            "desktop_data_dir_invalid",
            format!("{DESKTOP_DATA_DIR_ENV} must be an absolute path"),
            "Set the override to an absolute filesystem path or remove it.",
        ),
        DesktopDataDirError::Unavailable { reason, path_kind } => DesktopError::new(
            "desktop_data_dir_unavailable",
            reason.message(),
            "Check the Desktop app-data location and local filesystem permissions, then retry.",
        )
        .with_details(serde_json::json!({ "path_kind": path_kind.label() })),
    }
}

#[cfg(all(test, windows))]
fn resolve_physical_path(path: &Path) -> DesktopResult<PathBuf> {
    resolve_with_override(path.to_path_buf(), None).map(|d| d.effective)
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
        let expected = temp.path().canonicalize().unwrap().join("override");
        assert!(webcodex_runner_config::paths::paths_equal(
            &resolved.effective,
            &expected
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
        let expected = temp
            .path()
            .canonicalize()
            .unwrap()
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
        let expected = physical
            .canonicalize()
            .unwrap()
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
        let expected = second_target
            .canonicalize()
            .unwrap()
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
        let expected_runtime_root = effective.join("runtime").join("local");
        assert!(webcodex_runner_config::paths::paths_equal(
            &env_file,
            &expected_runtime_root.join("webcodex.env")
        ));
        assert!(webcodex_runner_config::paths::paths_equal(
            &server_data,
            &expected_runtime_root.join("data")
        ));
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use webcodex_environment::unified_update::desktop_data_dir::{
        DesktopDataDirError, DesktopDataDirFailureReason as Reason, DesktopDataDirPathKind,
    };

    #[test]
    fn relative_override_and_default_keep_distinct_desktop_errors() {
        let relative = PathBuf::from("private-relative-root");
        let error =
            resolve_with_override(relative.clone(), Some(relative.clone().into_os_string()))
                .unwrap_err();
        assert_eq!(error.code, "desktop_data_dir_invalid");
        assert_eq!(
            error.message,
            format!("{DESKTOP_DATA_DIR_ENV} must be an absolute path")
        );
        assert_eq!(
            error.next_action,
            "Set the override to an absolute filesystem path or remove it."
        );
        assert_eq!(error.details, None);

        let error = resolve_with_override(relative, None).unwrap_err();
        assert_eq!(error.code, "desktop_data_dir_unavailable");
        assert_eq!(error.message, "Desktop data directory is not absolute");
        assert_eq!(
            error.details,
            Some(serde_json::json!({ "path_kind": "relative" }))
        );
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("private-relative-root"));
    }

    #[test]
    fn unavailable_projection_preserves_each_fixed_reason_and_bounded_details() {
        for (reason, expected) in [
            (
                Reason::NotAbsolute,
                "Desktop data directory is not absolute",
            ),
            (
                Reason::MissingFinalComponent,
                "Desktop data directory must have an application-owned final component",
            ),
            (
                Reason::MissingParent,
                "Desktop data directory has no parent directory",
            ),
            (
                Reason::NoExistingAncestor,
                "Desktop data directory has no existing ancestor",
            ),
            (
                Reason::CannotInspect,
                "Desktop cannot inspect its app-data directory",
            ),
            (
                Reason::NonDirectoryAncestor,
                "Desktop data directory resolves through a non-directory ancestor",
            ),
            (
                Reason::CannotResolve,
                "Desktop cannot resolve its app-data directory",
            ),
            (
                Reason::RootReparsePoint,
                "Desktop data directory itself is a reparse point",
            ),
            (
                Reason::RootNotDirectory,
                "Desktop data directory is not a directory",
            ),
        ] {
            for path_kind in [
                DesktopDataDirPathKind::Absolute,
                DesktopDataDirPathKind::Relative,
            ] {
                let error =
                    project_data_dir_error(DesktopDataDirError::Unavailable { reason, path_kind });
                assert_eq!(error.code, "desktop_data_dir_unavailable");
                assert_eq!(error.message, expected);
                assert_eq!(error.next_action, "Check the Desktop app-data location and local filesystem permissions, then retry.");
                assert_eq!(
                    error.details,
                    Some(serde_json::json!({ "path_kind": path_kind.label() }))
                );
            }
        }
    }
}
