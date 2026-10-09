//! Neutral project recipe/root and source-file resolution.
//!
//! This module owns filesystem facts shared by project operations. It does not
//! select validation actions, synthesize argv, or attach validation evidence.

use glob::{MatchOptions, Pattern};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
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

fn cargo_workspace_excludes_manifest(
    root_manifest: &toml::Value,
    workspace_root: &Path,
    manifest_path: &Path,
) -> Result<bool, ProjectRecipeResolutionError> {
    // Cargo's exclusion test uses literal path prefixes. An explicit member
    // prefix takes precedence; these lists are not an independent glob API.
    let has_prefix = |key: &str| -> Result<bool, ProjectRecipeResolutionError> {
        let Some(value) = root_manifest
            .get("workspace")
            .and_then(|table| table.get(key))
        else {
            return Ok(false);
        };
        let values = value
            .as_array()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        let mut matched = false;
        for value in values {
            let path = value
                .as_str()
                .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
            matched |= manifest_path.starts_with(workspace_root.join(path));
        }
        Ok(matched)
    };
    Ok(has_prefix("exclude")? && !has_prefix("members")?)
}

/// Resolve and fence the package-selection authority behind portable Cargo
/// `all_packages`. The effective Cargo workspace must be the exact registered
/// Project root. A bounded conservative manifest scan supplies content
/// witnesses; the digest adds declared member and path-dependency routes.
/// Parent workspace discovery and external dependency membership must also
/// remain provably within this Project before `--workspace` can be admitted.
pub fn project_cargo_all_packages_provenance_files(
    resolved: &ResolvedProjectRecipeRoot,
) -> Result<Vec<PathBuf>, ProjectRecipeResolutionError> {
    if resolved.recipe != ProjectRecipeId::Rust {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }
    let workspace_root = rust_workspace_root(resolved)?;
    if workspace_root != resolved.execution_root {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }

    let root_manifest_path = workspace_root.join("Cargo.toml");
    let root_manifest = parse_cargo_manifest(&resolved.execution_root, &root_manifest_path)?;
    if resolved.absolute_root != workspace_root
        && cargo_workspace_excludes_manifest(
            &root_manifest,
            &workspace_root,
            &resolved.marker_path(),
        )?
    {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }
    if !cargo_manifest_has_workspace(&root_manifest)? {
        // Cargo searches parents even when a package inherits no fields. Only
        // probe marker metadata outside the Project; never parse external
        // manifests. An explicit local [workspace] stops Cargo's parent search.
        for parent in workspace_root.ancestors().skip(1) {
            match fs::symlink_metadata(parent.join("Cargo.toml")) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(ProjectRecipeResolutionError::SourceFileInvalid),
            }
        }
    }
    let canonical_root_manifest = root_manifest_path
        .canonicalize()
        .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
    let workspace = root_manifest
        .get("workspace")
        .and_then(toml::Value::as_table);
    if let Some(members) = workspace.and_then(|workspace| workspace.get("members")) {
        let members = members
            .as_array()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        for member in members {
            let member = member
                .as_str()
                .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
            validate_workspace_member_pattern(member)?;
        }
    }

    let mut manifests = BTreeSet::new();
    let mut pending = vec![workspace_root.clone()];
    let mut visited_dirs = BTreeSet::new();
    let mut scanned = 0usize;
    while let Some(directory) = pending.pop() {
        let canonical_directory = directory
            .canonicalize()
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        if !canonical_directory.starts_with(&workspace_root)
            || !visited_dirs.insert(canonical_directory.clone())
        {
            continue;
        }
        let entries = fs::read_dir(&canonical_directory)
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        for entry in entries {
            let entry = entry.map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
            scanned += 1;
            if scanned > CARGO_ALL_PACKAGES_MAX_SCAN_ENTRIES {
                return Err(ProjectRecipeResolutionError::SourceFileInvalid);
            }
            let name = entry.file_name();
            let file_type = entry
                .file_type()
                .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
            if matches!(
                name.to_str(),
                Some(".git" | "target" | "node_modules" | ".venv")
            ) {
                continue;
            }

            let mut path = entry.path();
            let mut effective_type = file_type;
            if effective_type.is_symlink() {
                // Declared directory aliases are witnessed by member/pattern
                // and dependency traversal. Unrelated links carry no manifest
                // content authority and must not widen this broad scan.
                if name != "Cargo.toml" {
                    continue;
                }
                path = path
                    .canonicalize()
                    .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
                if !path.starts_with(&workspace_root) {
                    return Err(ProjectRecipeResolutionError::SourceFileInvalid);
                }
                effective_type = fs::metadata(&path)
                    .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?
                    .file_type();
            }
            if effective_type.is_dir() {
                pending.push(path);
            } else if effective_type.is_file() && name == "Cargo.toml" {
                manifests.insert(path);
                if manifests.len() > CARGO_ALL_PACKAGES_MAX_MANIFESTS {
                    return Err(ProjectRecipeResolutionError::SourceFileInvalid);
                }
            }
        }
    }
    if !manifests.contains(&canonical_root_manifest) {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }
    let mut files = manifests.into_iter().collect::<Vec<_>>();
    files.push(workspace_root.join("Cargo.lock"));
    Ok(files)
}

fn validate_workspace_member_pattern(member: &str) -> Result<(), ProjectRecipeResolutionError> {
    let path = Path::new(member);
    if member.is_empty()
        || member.contains('\0')
        || member.contains('\\')
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }
    Ok(())
}

/// Hash Cargo all-packages provenance as a conservative superset.
///
/// The existing bounded project-wide manifest scan remains the content source.
/// This adds only topology that scan can lose: workspace-member glob matches,
/// symlink targets, manifests below pruned directories, and in-project path
/// dependencies. It does not reproduce Cargo's final membership algorithm.
pub fn digest_project_cargo_all_packages_provenance(
    resolved: &ResolvedProjectRecipeRoot,
) -> Result<String, ProjectRecipeResolutionError> {
    let mut files = project_cargo_all_packages_provenance_files(resolved)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let workspace_root = rust_workspace_root(resolved)?;
    if workspace_root != resolved.execution_root {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }

    let root_manifest_path = workspace_root.join("Cargo.toml");
    let root_manifest = parse_cargo_manifest(&workspace_root, &root_manifest_path)?;
    let workspace = root_manifest
        .get("workspace")
        .and_then(toml::Value::as_table);
    let mut topology = BTreeSet::new();
    let mut manifest_routes = BTreeSet::new();
    let root_route = cargo_manifest_route_if_present(&workspace_root, &workspace_root)?
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
    manifest_routes.insert(root_route.clone());
    let mut workspace_member_routes = BTreeSet::new();
    if root_manifest.get("package").is_some() {
        workspace_member_routes.insert(root_route.clone());
    }
    for manifest in files
        .iter()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml"))
    {
        manifest_routes.insert(CargoManifestRoute {
            logical: manifest.clone(),
            canonical: manifest.clone(),
        });
    }
    ensure_manifest_route_bound(&manifest_routes)?;
    ensure_canonical_manifest_bound(&manifest_routes)?;

    let mut scanned = 0usize;
    if let Some(members) = workspace.and_then(|workspace| workspace.get("members")) {
        let members = members
            .as_array()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        for member in members {
            let member = member
                .as_str()
                .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
            validate_workspace_member_pattern(member)?;
            for logical in
                workspace_member_witnesses(&workspace_root, member, &mut scanned, &mut topology)?
            {
                record_workspace_member(
                    &workspace_root,
                    member,
                    &logical,
                    &mut topology,
                    &mut files,
                    &mut manifest_routes,
                    &mut workspace_member_routes,
                )?;
            }
        }
    }

    add_path_dependency_witnesses(
        &workspace_root,
        &mut files,
        &mut manifest_routes,
        &mut topology,
    )?;
    extend_cargo_workspace_member_routes(
        &workspace_root,
        &root_manifest,
        &mut workspace_member_routes,
    )?;
    if resolved.absolute_root != workspace_root {
        let requested_manifest = resolved
            .marker_path()
            .canonicalize()
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        if !workspace_member_routes
            .iter()
            .any(|route| route.canonical == requested_manifest)
        {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(b"webcodex:cargo-all-packages-provenance:v3\0");
    let files_digest = digest_project_recipe_files(&workspace_root, files)?;
    hash_field(&mut hasher, files_digest.as_bytes());
    for route in manifest_routes {
        hash_field(&mut hasher, b"manifest_route");
        hash_field(
            &mut hasher,
            &logical_path_identity(&workspace_root, &route.logical),
        );
        hash_field(
            &mut hasher,
            &relative_path_identity(&workspace_root, &route.canonical)?,
        );
    }
    for entry in topology {
        hash_field(&mut hasher, entry.kind.as_bytes());
        hash_field(&mut hasher, &entry.owner);
        hash_field(&mut hasher, entry.declaration.as_bytes());
        hash_field(&mut hasher, &entry.logical);
        hash_field(&mut hasher, &entry.resolved);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

const CARGO_ALL_PACKAGES_MAX_MANIFESTS: usize = 256;
const CARGO_ALL_PACKAGES_MAX_SCAN_ENTRIES: usize = 8192;
const CARGO_ALL_PACKAGES_MAX_TOPOLOGY_ENTRIES: usize = 1024;
const CARGO_DEPENDENCY_TABLE_KEYS: [&str; 5] = [
    "dependencies",
    "dev-dependencies",
    "dev_dependencies",
    "build-dependencies",
    "build_dependencies",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CargoTopologyEntry {
    kind: &'static str,
    owner: Vec<u8>,
    declaration: String,
    logical: Vec<u8>,
    resolved: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CargoPathDependencyWitness {
    declaration: String,
    logical: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CargoManifestRoute {
    logical: PathBuf,
    canonical: PathBuf,
}

fn workspace_member_witnesses(
    workspace_root: &Path,
    declaration: &str,
    scanned: &mut usize,
    topology: &mut BTreeSet<CargoTopologyEntry>,
) -> Result<Vec<PathBuf>, ProjectRecipeResolutionError> {
    if !workspace_pattern_has_glob(declaration) {
        return Ok(vec![workspace_root.join(declaration)]);
    }

    // Cargo feeds the joined member path through glob(), which decomposes it
    // with Path::components(). Mirror that normalization so repeated
    // separators and interior dot components cannot create a provenance-only
    // mismatch for an otherwise valid Cargo workspace.
    let normalized = Path::new(declaration)
        .components()
        .filter_map(|component| match component {
            Component::CurDir => None,
            Component::Normal(component) => component.to_str(),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    let pattern =
        Pattern::new(&normalized).map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
    let components = normalized.split('/').collect::<Vec<_>>();
    let prefix_len = components
        .iter()
        .position(|component| workspace_component_has_glob(component))
        .unwrap_or(components.len());
    let mut start = workspace_root.to_path_buf();
    for component in &components[..prefix_len] {
        if *component != "." {
            start.push(component);
        }
    }
    if fs::symlink_metadata(&start).is_err() {
        return Ok(Vec::new());
    }

    let max_depth = (!components.iter().any(|component| *component == "**"))
        .then_some(components.len().saturating_sub(prefix_len));
    let component_patterns = if max_depth.is_some() {
        Some(
            components[prefix_len..]
                .iter()
                .map(|component| Pattern::new(component))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?,
        )
    } else {
        None
    };
    // Cargo expands workspace members with glob::glob(), whose glob_with()
    // path iterator always requires wildcard tokens to respect path-component
    // separators. Match the same rule while retaining our explicit scan bound.
    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };
    let mut pending = vec![(start, 0usize, Vec::<PathBuf>::new())];
    let mut matches = BTreeSet::new();

    while let Some((directory, depth, ancestry)) = pending.pop() {
        let canonical_directory = directory
            .canonicalize()
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        if !canonical_directory.starts_with(workspace_root) {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
        if max_depth.is_none() && ancestry.contains(&canonical_directory) {
            // Recursive glob matches can take a different logical route through
            // this cycle. Pruning it would certify an incomplete witness set.
            // Finite-depth patterns remain bounded by their component count.
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
        let mut branch_ancestry = ancestry;
        if max_depth.is_none() {
            branch_ancestry.push(canonical_directory);
        }

        let entries = fs::read_dir(&directory)
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        let mut bounded_entries = Vec::new();
        for entry in entries {
            *scanned += 1;
            if *scanned > CARGO_ALL_PACKAGES_MAX_SCAN_ENTRIES {
                return Err(ProjectRecipeResolutionError::SourceFileInvalid);
            }
            bounded_entries
                .push(entry.map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?);
        }
        bounded_entries.sort_by_key(|entry| entry.file_name());

        for entry in bounded_entries {
            // Finite glob expansion only visits matching path components.
            // Filter before following links or adding topology witnesses.
            if let Some(patterns) = &component_patterns {
                let component = patterns
                    .get(depth)
                    .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
                if !component.matches_path_with(Path::new(&entry.file_name()), options) {
                    continue;
                }
            }
            let path = entry.path();
            let symlink = entry
                .file_type()
                .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?
                .is_symlink();
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if !metadata.is_dir() {
                continue;
            }
            let canonical = path
                .canonicalize()
                .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
            if !canonical.starts_with(workspace_root) {
                return Err(ProjectRecipeResolutionError::SourceFileInvalid);
            }

            if symlink {
                insert_topology(
                    topology,
                    CargoTopologyEntry {
                        kind: "member_glob_symlink",
                        owner: b"workspace.members".to_vec(),
                        declaration: declaration.into(),
                        logical: relative_path_identity(workspace_root, &path)?,
                        resolved: relative_path_identity(workspace_root, &canonical)?,
                    },
                )?;
            }

            let relative = path
                .strip_prefix(workspace_root)
                .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
            if pattern.matches_path_with(relative, options) {
                matches.insert(path.clone());
                if matches.len() > CARGO_ALL_PACKAGES_MAX_TOPOLOGY_ENTRIES {
                    return Err(ProjectRecipeResolutionError::SourceFileInvalid);
                }
            }

            let next_depth = depth + 1;
            if max_depth.is_none_or(|limit| next_depth < limit) {
                pending.push((path, next_depth, branch_ancestry.clone()));
            }
        }
    }
    Ok(matches.into_iter().collect())
}
fn record_workspace_member(
    workspace_root: &Path,
    declaration: &str,
    logical: &Path,
    topology: &mut BTreeSet<CargoTopologyEntry>,
    files: &mut BTreeSet<PathBuf>,
    manifest_routes: &mut BTreeSet<CargoManifestRoute>,
    workspace_member_routes: &mut BTreeSet<CargoManifestRoute>,
) -> Result<(), ProjectRecipeResolutionError> {
    let logical_id = logical_path_identity(workspace_root, logical);
    let Some(canonical) = canonicalize_optional_path(logical)? else {
        insert_topology(
            topology,
            CargoTopologyEntry {
                kind: "workspace_member",
                owner: b"workspace.members".to_vec(),
                declaration: declaration.into(),
                logical: logical_id,
                resolved: b"missing\0".to_vec(),
            },
        )?;
        return Ok(());
    };
    if !canonical.starts_with(workspace_root) {
        return Err(ProjectRecipeResolutionError::SourceFileInvalid);
    }
    insert_topology(
        topology,
        CargoTopologyEntry {
            kind: "workspace_member",
            owner: b"workspace.members".to_vec(),
            declaration: declaration.into(),
            logical: logical_id,
            resolved: relative_path_identity(workspace_root, &canonical)?,
        },
    )?;

    if let Some(route) = cargo_manifest_route_if_present(workspace_root, logical)? {
        files.insert(route.canonical.clone());
        manifest_routes.insert(route.clone());
        workspace_member_routes.insert(route);
        ensure_manifest_route_bound(manifest_routes)?;
        ensure_manifest_route_bound(workspace_member_routes)?;
        ensure_canonical_manifest_bound(manifest_routes)?;
    }
    Ok(())
}

fn extend_cargo_workspace_member_routes(
    workspace_root: &Path,
    root_manifest: &toml::Value,
    routes: &mut BTreeSet<CargoManifestRoute>,
) -> Result<(), ProjectRecipeResolutionError> {
    let workspace_dependencies = root_manifest
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("dependencies"))
        .map(|dependencies| {
            dependencies
                .as_table()
                .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)
        })
        .transpose()?;
    let mut pending = routes.iter().cloned().collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some(route) = pending.pop() {
        let logical_parent = route
            .logical
            .parent()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?
            .canonicalize()
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        if !logical_parent.starts_with(workspace_root)
            || !visited.insert((route.canonical.clone(), logical_parent))
        {
            continue;
        }
        let manifest = parse_cargo_manifest(workspace_root, &route.canonical)?;
        let manifest_dir = route
            .logical
            .parent()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        for dependency in cargo_workspace_member_path_dependencies(
            &manifest,
            manifest_dir,
            workspace_root,
            workspace_dependencies,
        )? {
            let Some(target) = canonicalize_optional_path(&dependency.logical)? else {
                continue;
            };
            if !target.starts_with(workspace_root) {
                return Err(ProjectRecipeResolutionError::SourceFileInvalid);
            }
            let Some(target_route) =
                cargo_manifest_route_if_present(workspace_root, &dependency.logical)?
            else {
                continue;
            };
            if cargo_workspace_excludes_manifest(
                root_manifest,
                workspace_root,
                &target_route.logical,
            )? {
                continue;
            }
            if routes.insert(target_route.clone()) {
                ensure_manifest_route_bound(routes)?;
                ensure_canonical_manifest_bound(routes)?;
                pending.push(target_route);
            }
        }
    }
    Ok(())
}

fn cargo_workspace_member_path_dependencies(
    manifest: &toml::Value,
    manifest_dir: &Path,
    workspace_root: &Path,
    workspace_dependencies: Option<&toml::map::Map<String, toml::Value>>,
) -> Result<Vec<CargoPathDependencyWitness>, ProjectRecipeResolutionError> {
    let root = manifest
        .as_table()
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
    let mut witnesses = BTreeSet::new();
    for key in CARGO_DEPENDENCY_TABLE_KEYS {
        collect_workspace_member_dependencies(
            root.get(key),
            key,
            manifest_dir,
            workspace_root,
            workspace_dependencies,
            &mut witnesses,
        )?;
    }
    if let Some(targets) = root.get("target").and_then(toml::Value::as_table) {
        for (target_name, target) in targets {
            let target = target
                .as_table()
                .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
            for key in CARGO_DEPENDENCY_TABLE_KEYS {
                collect_workspace_member_dependencies(
                    target.get(key),
                    &format!("target.{target_name}.{key}"),
                    manifest_dir,
                    workspace_root,
                    workspace_dependencies,
                    &mut witnesses,
                )?;
            }
        }
    }
    Ok(witnesses.into_iter().collect())
}

fn collect_workspace_member_dependencies(
    value: Option<&toml::Value>,
    table_name: &str,
    manifest_dir: &Path,
    workspace_root: &Path,
    workspace_dependencies: Option<&toml::map::Map<String, toml::Value>>,
    witnesses: &mut BTreeSet<CargoPathDependencyWitness>,
) -> Result<(), ProjectRecipeResolutionError> {
    let Some(dependencies) = value else {
        return Ok(());
    };
    let dependencies = dependencies
        .as_table()
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
    for (name, dependency) in dependencies {
        let Some(table) = dependency.as_table() else {
            continue;
        };
        let (path, base) = if let Some(path) = table.get("path") {
            (
                path.as_str()
                    .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?,
                manifest_dir,
            )
        } else if table.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
            let Some(inherited) = workspace_dependencies
                .and_then(|dependencies| dependencies.get(name))
                .and_then(toml::Value::as_table)
            else {
                continue;
            };
            let Some(path) = inherited.get("path") else {
                continue;
            };
            (
                path.as_str()
                    .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?,
                workspace_root,
            )
        } else {
            continue;
        };
        if path.is_empty() || path.as_bytes().contains(&0) {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
        let declared = Path::new(path);
        witnesses.insert(CargoPathDependencyWitness {
            declaration: format!("{table_name}.{name}:path={path}"),
            logical: if declared.is_absolute() {
                declared.to_path_buf()
            } else {
                base.join(declared)
            },
        });
        if witnesses.len() > CARGO_ALL_PACKAGES_MAX_TOPOLOGY_ENTRIES {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
    }
    Ok(())
}

fn add_path_dependency_witnesses(
    workspace_root: &Path,
    files: &mut BTreeSet<PathBuf>,
    manifest_routes: &mut BTreeSet<CargoManifestRoute>,
    topology: &mut BTreeSet<CargoTopologyEntry>,
) -> Result<(), ProjectRecipeResolutionError> {
    let mut pending = manifest_routes.iter().cloned().collect::<Vec<_>>();
    let mut visited = BTreeSet::new();

    while let Some(route) = pending.pop() {
        if !route.canonical.starts_with(workspace_root) {
            continue;
        }
        let logical_parent = route
            .logical
            .parent()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?
            .canonicalize()
            .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
        if !logical_parent.starts_with(workspace_root)
            || !visited.insert((route.canonical.clone(), logical_parent))
        {
            continue;
        }
        let manifest = parse_cargo_manifest(workspace_root, &route.canonical)?;
        let manifest_dir = route
            .logical
            .parent()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
        let owner = logical_path_identity(workspace_root, &route.logical);

        for dependency in cargo_path_dependency_witnesses(&manifest, manifest_dir)? {
            let logical_id = logical_path_identity(workspace_root, &dependency.logical);
            let resolved = match canonicalize_optional_path(&dependency.logical)? {
                Some(target) if target.starts_with(workspace_root) => {
                    if let Some(target_route) =
                        cargo_manifest_route_if_present(workspace_root, &dependency.logical)?
                    {
                        files.insert(target_route.canonical.clone());
                        if manifest_routes.insert(target_route.clone()) {
                            ensure_manifest_route_bound(manifest_routes)?;
                            ensure_canonical_manifest_bound(manifest_routes)?;
                            pending.push(target_route);
                        }
                    }
                    relative_path_identity(workspace_root, &target)?
                }
                Some(_) => {
                    // An external path dependency can opt into this workspace
                    // with package.workspace. Its membership cannot be proven
                    // without reading beyond the registered Project authority.
                    return Err(ProjectRecipeResolutionError::SourceFileInvalid);
                }
                None => b"missing\0".to_vec(),
            };
            insert_topology(
                topology,
                CargoTopologyEntry {
                    kind: "path_dependency",
                    owner: owner.clone(),
                    declaration: dependency.declaration,
                    logical: logical_id,
                    resolved,
                },
            )?;
        }
    }
    Ok(())
}

fn canonicalize_optional_path(
    path: &Path,
) -> Result<Option<PathBuf>, ProjectRecipeResolutionError> {
    match path.canonicalize() {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ProjectRecipeResolutionError::SourceFileInvalid),
    }
}

fn cargo_manifest_route_if_present(
    workspace_root: &Path,
    directory: &Path,
) -> Result<Option<CargoManifestRoute>, ProjectRecipeResolutionError> {
    let logical = directory.join("Cargo.toml");
    match fs::symlink_metadata(&logical) {
        Ok(_) => {
            let canonical = logical
                .canonicalize()
                .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
            if !canonical.starts_with(workspace_root) || !canonical.is_file() {
                return Err(ProjectRecipeResolutionError::SourceFileInvalid);
            }
            Ok(Some(CargoManifestRoute { logical, canonical }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ProjectRecipeResolutionError::SourceFileInvalid),
    }
}

fn cargo_path_dependency_witnesses(
    manifest: &toml::Value,
    manifest_dir: &Path,
) -> Result<Vec<CargoPathDependencyWitness>, ProjectRecipeResolutionError> {
    let root = manifest
        .as_table()
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
    let mut witnesses = BTreeSet::new();
    for key in CARGO_DEPENDENCY_TABLE_KEYS {
        collect_path_dependencies(root.get(key), key, manifest_dir, &mut witnesses)?;
    }
    if let Some(workspace) = root.get("workspace").and_then(toml::Value::as_table) {
        collect_path_dependencies(
            workspace.get("dependencies"),
            "workspace.dependencies",
            manifest_dir,
            &mut witnesses,
        )?;
    }
    if let Some(targets) = root.get("target").and_then(toml::Value::as_table) {
        for (target_name, target) in targets {
            let target = target
                .as_table()
                .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?;
            for key in CARGO_DEPENDENCY_TABLE_KEYS {
                collect_path_dependencies(
                    target.get(key),
                    &format!("target.{target_name}.{key}"),
                    manifest_dir,
                    &mut witnesses,
                )?;
            }
        }
    }
    Ok(witnesses.into_iter().collect())
}

fn collect_path_dependencies(
    value: Option<&toml::Value>,
    table_name: &str,
    manifest_dir: &Path,
    witnesses: &mut BTreeSet<CargoPathDependencyWitness>,
) -> Result<(), ProjectRecipeResolutionError> {
    let Some(dependencies) = value.and_then(toml::Value::as_table) else {
        return Ok(());
    };
    for (name, dependency) in dependencies {
        let Some(path) = dependency
            .as_table()
            .and_then(|table| table.get("path"))
            .and_then(toml::Value::as_str)
        else {
            continue;
        };
        if path.is_empty() || path.as_bytes().contains(&0) {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
        let declared = Path::new(path);
        witnesses.insert(CargoPathDependencyWitness {
            declaration: format!("{table_name}.{name}:path={path}"),
            logical: if declared.is_absolute() {
                declared.to_path_buf()
            } else {
                manifest_dir.join(declared)
            },
        });
        if witnesses.len() > CARGO_ALL_PACKAGES_MAX_TOPOLOGY_ENTRIES {
            return Err(ProjectRecipeResolutionError::SourceFileInvalid);
        }
    }
    Ok(())
}

fn ensure_canonical_manifest_bound(
    routes: &BTreeSet<CargoManifestRoute>,
) -> Result<(), ProjectRecipeResolutionError> {
    let canonical = routes
        .iter()
        .map(|route| route.canonical.as_path())
        .collect::<BTreeSet<_>>();
    (canonical.len() <= CARGO_ALL_PACKAGES_MAX_MANIFESTS)
        .then_some(())
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)
}

fn ensure_manifest_route_bound(
    routes: &BTreeSet<CargoManifestRoute>,
) -> Result<(), ProjectRecipeResolutionError> {
    (routes.len() <= CARGO_ALL_PACKAGES_MAX_TOPOLOGY_ENTRIES)
        .then_some(())
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)
}

fn insert_topology(
    topology: &mut BTreeSet<CargoTopologyEntry>,
    entry: CargoTopologyEntry,
) -> Result<(), ProjectRecipeResolutionError> {
    topology.insert(entry);
    (topology.len() <= CARGO_ALL_PACKAGES_MAX_TOPOLOGY_ENTRIES)
        .then_some(())
        .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn relative_path_identity(
    workspace_root: &Path,
    path: &Path,
) -> Result<Vec<u8>, ProjectRecipeResolutionError> {
    let relative = path
        .strip_prefix(workspace_root)
        .map_err(|_| ProjectRecipeResolutionError::SourceFileInvalid)?;
    Ok(path_identity(b"project-relative", relative))
}

fn logical_path_identity(workspace_root: &Path, path: &Path) -> Vec<u8> {
    match path.strip_prefix(workspace_root) {
        Ok(relative) => path_identity(b"project-relative", relative),
        Err(_) => b"outside-project\0".to_vec(),
    }
}

fn path_identity(tag: &[u8], path: &Path) -> Vec<u8> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut identity = Vec::with_capacity(tag.len() + 9 + bytes.len());
    identity.extend_from_slice(tag);
    identity.push(0);
    identity.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    identity.extend_from_slice(bytes);
    identity
}

fn workspace_pattern_has_glob(pattern: &str) -> bool {
    pattern
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'*' | b'?' | b'['))
}

fn workspace_component_has_glob(component: &str) -> bool {
    component
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'*' | b'?' | b'['))
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
            "license_file",
            "publish",
            "readme",
            "repository",
            "rust-version",
            "rust_version",
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

    for key in CARGO_DEPENDENCY_TABLE_KEYS {
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
            for key in CARGO_DEPENDENCY_TABLE_KEYS {
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

/// Reject an invalid Node marker before ordinary nearest-root detection can
/// silently fall back to an ancestor project. This validates filesystem facts;
/// it does not independently select or synthesize a validation operation.
pub fn validate_node_project_marker_chain(
    execution_root: &Path,
    cwd: Option<&str>,
) -> Result<(), ProjectRecipeResolutionError> {
    let root = execution_root
        .canonicalize()
        .map_err(|_| ProjectRecipeResolutionError::ExecutionRootUnavailable)?;
    let mut directory = resolve_cwd(&root, cwd)?;
    loop {
        match fs::symlink_metadata(directory.join(ProjectRecipeId::Node.marker())) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(ProjectRecipeResolutionError::SourceFileInvalid);
            }
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ProjectRecipeResolutionError::SourceFileInvalid),
        }
        // The existing resolver stops at the nearest project marker. Ignore
        // unrelated ancestor packages after such a valid boundary.
        if [
            ProjectRecipeId::Rust,
            ProjectRecipeId::Go,
            ProjectRecipeId::Python,
        ]
        .into_iter()
        .any(|recipe| directory.join(recipe.marker()).is_file())
        {
            return Ok(());
        }
        if directory == root {
            return Ok(());
        }
        directory = directory
            .parent()
            .ok_or(ProjectRecipeResolutionError::SourceFileInvalid)?
            .to_path_buf();
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
