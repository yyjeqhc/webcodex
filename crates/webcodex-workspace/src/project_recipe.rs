//! Neutral project recipe/root and source-file resolution.
//!
//! This module owns filesystem facts shared by project operations. It does not
//! select validation actions, synthesize argv, or attach validation evidence.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

const PROJECT_RECIPE_NAMES: [&str; 4] = ["rust", "node", "python", "go"];
const PROJECT_RECIPE_MARKERS: [&str; 4] =
    ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub enum ProjectRecipeId {
    Rust,
    Node,
    Python,
    Go,
}

impl ProjectRecipeId {
    pub fn as_str(self) -> &'static str {
        PROJECT_RECIPE_NAMES[self as usize]
    }

    pub fn marker(self) -> &'static str {
        PROJECT_RECIPE_MARKERS[self as usize]
    }

    fn all() -> [Self; 4] {
        [Self::Rust, Self::Node, Self::Python, Self::Go]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProjectRecipeRoot {
    pub recipe: ProjectRecipeId,
    /// Canonical registered project root used as the containment boundary.
    pub execution_root: PathBuf,
    pub absolute_root: PathBuf,
    pub relative_root: String,
}

impl ResolvedProjectRecipeRoot {
    pub fn marker_path(&self) -> PathBuf {
        self.absolute_root.join(self.recipe.marker())
    }
}

/// Canonical manifest/dependency inputs for portable Rust/Go project-operation
/// provenance. Paths stay inside the registered execution root. Rust members
/// include the effective Cargo workspace manifest and root lockfile while their
/// native execution cwd remains the nearest package/recipe root.
pub fn project_recipe_provenance_files(
    resolved: &ResolvedProjectRecipeRoot,
) -> Result<Option<Vec<PathBuf>>, ProjectRecipeResolutionError> {
    match resolved.recipe {
        ProjectRecipeId::Rust => {
            let workspace_root = rust_workspace_root(resolved)?;
            let marker_path = resolved.marker_path();
            let workspace_manifest = workspace_root.join("Cargo.toml");
            let mut files = vec![marker_path.clone()];
            if workspace_manifest != marker_path {
                files.push(workspace_manifest);
            }
            files.push(workspace_root.join("Cargo.lock"));
            Ok(Some(files))
        }
        ProjectRecipeId::Go => Ok(Some(vec![
            resolved.marker_path(),
            resolved.absolute_root.join("go.sum"),
        ])),
        ProjectRecipeId::Node | ProjectRecipeId::Python => Ok(None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecipeResolutionError {
    ExecutionRootUnavailable,
    CwdMismatch,
    NotFound,
    ExplicitNotFound {
        expected: ProjectRecipeId,
    },
    ExplicitMismatch {
        recipe_root: String,
        expected: ProjectRecipeId,
        candidates: Vec<ProjectRecipeId>,
    },
    Ambiguous {
        recipe_root: String,
        candidates: Vec<ProjectRecipeId>,
    },
    SourceFileInvalid,
}

pub fn resolve_project_recipe_root(
    execution_root: &Path,
    cwd: Option<&str>,
    hint: Option<ProjectRecipeId>,
) -> Result<ResolvedProjectRecipeRoot, ProjectRecipeResolutionError> {
    let execution_root = execution_root
        .canonicalize()
        .map_err(|_| ProjectRecipeResolutionError::ExecutionRootUnavailable)?;
    let cwd = resolve_cwd(&execution_root, cwd)?;
    let (recipe, absolute_root) = nearest_recipe_root(&execution_root, &cwd, hint)?;
    let relative_root = relative_root(&execution_root, &absolute_root);
    Ok(ResolvedProjectRecipeRoot {
        recipe,
        execution_root,
        absolute_root,
        relative_root,
    })
}

fn rust_workspace_root(
    resolved: &ResolvedProjectRecipeRoot,
) -> Result<PathBuf, ProjectRecipeResolutionError> {
    debug_assert_eq!(resolved.recipe, ProjectRecipeId::Rust);
    let member_manifest = parse_cargo_manifest(&resolved.execution_root, &resolved.marker_path())?;
    if cargo_manifest_has_workspace(&member_manifest)? {
        return Ok(resolved.absolute_root.clone());
    }

    if let Some(workspace) = cargo_manifest_workspace_path(&member_manifest)? {
        let workspace_root = resolved
            .absolute_root
            .join(workspace)
            .canonicalize()
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        if !workspace_root.starts_with(&resolved.execution_root) || !workspace_root.is_dir() {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
        let workspace_manifest =
            parse_cargo_manifest(&resolved.execution_root, &workspace_root.join("Cargo.toml"))?;
        if !cargo_manifest_has_workspace(&workspace_manifest)? {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
        return Ok(workspace_root);
    }

    let mut directory = resolved.absolute_root.clone();
    while directory != resolved.execution_root {
        let Some(parent) = directory.parent() else {
            break;
        };
        if !parent.starts_with(&resolved.execution_root) {
            break;
        }
        directory = parent.to_path_buf();
        let manifest_path = directory.join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let manifest = parse_cargo_manifest(&resolved.execution_root, &manifest_path)?;
        if cargo_manifest_has_workspace(&manifest)? {
            return Ok(directory);
        }
    }
    // Cargo workspace inheritance is authoritative evidence that this package
    // is not standalone. If no workspace root was found inside the registered
    // Project boundary, do not silently hash a package-local lockfile that
    // Cargo itself will not use; the caller must register the containing
    // workspace instead.
    if cargo_manifest_uses_workspace_inheritance(&member_manifest)? {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }
    Ok(resolved.absolute_root.clone())
}

fn cargo_manifest_uses_workspace_inheritance(
    manifest: &toml::Value,
) -> Result<bool, ProjectRecipeResolutionError> {
    let root = manifest
        .as_table()
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;

    if let Some(package) = root.get("package") {
        let package = package
            .as_table()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        for key in [
            "authors",
            "categories",
            "description",
            "documentation",
            "edition",
            "exclude",
            "homepage",
            "include",
            "keywords",
            "license",
            "license-file",
            "publish",
            "readme",
            "repository",
            "rust-version",
            "version",
        ] {
            if package
                .get(key)
                .and_then(toml::Value::as_table)
                .and_then(|table| table.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true)
            {
                return Ok(true);
            }
        }
    }

    if let Some(lints) = root.get("lints") {
        let lints = lints
            .as_table()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        if lints.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
            return Ok(true);
        }
    }

    for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if cargo_dependency_table_uses_workspace(root.get(key))? {
            return Ok(true);
        }
    }

    if let Some(targets) = root.get("target") {
        let targets = targets
            .as_table()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        for target in targets.values() {
            let target = target
                .as_table()
                .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
            for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if cargo_dependency_table_uses_workspace(target.get(key))? {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

fn cargo_dependency_table_uses_workspace(
    value: Option<&toml::Value>,
) -> Result<bool, ProjectRecipeResolutionError> {
    let Some(value) = value else {
        return Ok(false);
    };
    let dependencies = value
        .as_table()
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
    Ok(dependencies.values().any(|dependency| {
        dependency
            .as_table()
            .and_then(|table| table.get("workspace"))
            .and_then(toml::Value::as_bool)
            == Some(true)
    }))
}

fn parse_cargo_manifest(
    execution_root: &Path,
    path: &Path,
) -> Result<toml::Value, ProjectRecipeResolutionError> {
    let bytes = read_project_recipe_file(execution_root, path)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
    toml::from_str(text).map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)
}

fn cargo_manifest_has_workspace(
    manifest: &toml::Value,
) -> Result<bool, ProjectRecipeResolutionError> {
    match manifest.get("workspace") {
        None => Ok(false),
        Some(value) if value.is_table() => Ok(true),
        Some(_) => Err(ProjectRecipeResolutionError::SourceFileInvalid),
    }
}

fn cargo_manifest_workspace_path<'a>(
    manifest: &'a toml::Value,
) -> Result<Option<&'a str>, ProjectRecipeResolutionError> {
    let Some(package) = manifest.get("package") else {
        return Ok(None);
    };
    let package = package
        .as_table()
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
    let Some(workspace) = package.get("workspace") else {
        return Ok(None);
    };
    workspace
        .as_str()
        .map(Some)
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)
}

fn resolve_cwd(root: &Path, raw: Option<&str>) -> Result<PathBuf, ProjectRecipeResolutionError> {
    let raw = raw.unwrap_or(".");
    let path = Path::new(raw);
    if raw.is_empty()
        || raw.contains('\0')
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ProjectRecipeResolutionError::CwdMismatch);
    }
    let cwd = root
        .join(path)
        .canonicalize()
        .map_err(|_| ProjectRecipeResolutionError::CwdMismatch)?;
    if !cwd.starts_with(root) || !cwd.is_dir() {
        return Err(ProjectRecipeResolutionError::CwdMismatch);
    }
    Ok(cwd)
}

fn nearest_recipe_root(
    root: &Path,
    cwd: &Path,
    explicit: Option<ProjectRecipeId>,
) -> Result<(ProjectRecipeId, PathBuf), ProjectRecipeResolutionError> {
    let mut directory = cwd.to_path_buf();
    loop {
        let candidates = ProjectRecipeId::all()
            .into_iter()
            .filter(|recipe| directory.join(recipe.marker()).is_file())
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            let recipe_root = relative_root(root, &directory);
            if let Some(expected) = explicit {
                if candidates.contains(&expected) {
                    return Ok((expected, directory));
                }
                if expected == ProjectRecipeId::Python {
                    return Ok((ProjectRecipeId::Python, cwd.to_path_buf()));
                }
                return Err(ProjectRecipeResolutionError::ExplicitMismatch {
                    recipe_root,
                    expected,
                    candidates,
                });
            }
            if candidates.len() == 1 {
                return Ok((candidates[0], directory));
            }
            let mut candidates = candidates;
            candidates.sort_by_key(|recipe| recipe.as_str());
            return Err(ProjectRecipeResolutionError::Ambiguous {
                recipe_root,
                candidates,
            });
        }
        if directory == root {
            break;
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent.to_path_buf();
    }
    if explicit == Some(ProjectRecipeId::Python) {
        return Ok((ProjectRecipeId::Python, cwd.to_path_buf()));
    }
    match explicit {
        Some(expected) => Err(ProjectRecipeResolutionError::ExplicitNotFound { expected }),
        None => Err(ProjectRecipeResolutionError::NotFound),
    }
}

pub fn read_project_recipe_file(
    execution_root: &Path,
    path: &Path,
) -> Result<Vec<u8>, ProjectRecipeResolutionError> {
    let path = anchored_path(execution_root, path);
    let canonical = path
        .canonicalize()
        .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
    if !canonical.starts_with(execution_root) || !canonical.is_file() {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }
    fs::read(canonical).map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)
}

pub fn digest_project_recipe_files<I>(
    execution_root: &Path,
    paths: I,
) -> Result<String, ProjectRecipeResolutionError>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut hasher = Sha256::new();
    for path in paths {
        let path = anchored_path(execution_root, &path);
        if !path.exists() {
            continue;
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        if !canonical.starts_with(execution_root) || !canonical.is_file() {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
        let content =
            fs::read(&canonical).map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        let relative = canonical
            .strip_prefix(execution_root)
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update((content.len() as u64).to_be_bytes());
        hasher.update(content);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn anchored_path(execution_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        execution_root.join(path)
    }
}

fn relative_root(root: &Path, recipe_root: &Path) -> String {
    let relative = recipe_root.strip_prefix(root).unwrap_or(Path::new(""));
    if relative.as_os_str().is_empty() {
        ".".to_string()
    } else {
        relative.to_string_lossy().replace('\\', "/")
    }
}
