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

/// Canonical optional dependency-state files for the portable Rust/Go recipe
/// consumers. Missing files remain valid and are skipped by the digest helper.
/// Node/Python intentionally return None because their dependency/package-manager
/// provenance is not represented by this narrow source-truth contract.
pub fn project_recipe_dependency_state_files(
    recipe: ProjectRecipeId,
) -> Option<&'static [&'static str]> {
    match recipe {
        ProjectRecipeId::Rust => Some(&["Cargo.lock"]),
        ProjectRecipeId::Go => Some(&["go.sum"]),
        ProjectRecipeId::Node | ProjectRecipeId::Python => None,
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
