//! Deterministic project-aware plans resolved on the owning Runner.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use webcodex_core::runner_protocol::{normalize_rust_test_filter, ShellJobValidationStep};
use webcodex_workspace::project_recipe::{
    digest_project_cargo_all_packages_provenance, digest_project_recipe_files,
    project_recipe_provenance_files, read_project_recipe_file, resolve_project_recipe_root,
    validate_node_project_marker_chain, ProjectRecipeResolutionError,
};

pub use webcodex_workspace::project_recipe::ProjectRecipeId as RecipeId;

const RECIPE_VERSION: u32 = 1;
const PYTHON_MANIFESTLESS_DIGEST_SEED: &[u8] = b"webcodex.python.manifestless.recipe.v1";
const PYTHON_UNITTEST_ARGS: [&str; 5] = ["-B", "-m", "unittest", "discover", "-v"];
const PYTEST_PROJECT_CONFIG_FILES: [&str; 8] = [
    "pytest.toml",
    ".pytest.toml",
    "pytest.ini",
    ".pytest.ini",
    "pyproject.toml",
    "tox.ini",
    "setup.cfg",
    "setup.py",
];
const NODE_LOCKFILES: [(&str, &str); 6] = [
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "yarn"),
    ("package-lock.json", "npm"),
    ("npm-shrinkwrap.json", "npm"),
    ("bun.lock", "bun"),
    ("bun.lockb", "bun"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub enum SemanticCheck {
    Format,
    Check,
    Test,
}

impl SemanticCheck {
    pub fn as_str(self) -> &'static str {
        ["format", "check", "test"][self as usize]
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedValidationRecipe {
    pub recipe_id: &'static str,
    pub recipe_root_relative: String,
    pub steps: Vec<ShellJobValidationStep>,
    pub invocation_digest: String,
    pub manifest_digest: String,
    /// Normalized (trimmed, validated) test filter actually placed in the plan,
    /// or `None`. Bound into the request hash so retries key on the executed
    /// value, not the raw request string.
    pub test_filter: Option<String>,
}

impl ResolvedValidationRecipe {
    pub fn durable_identity(&self) -> Value {
        json!({
            "recipe_id": self.recipe_id,
            "recipe_version": RECIPE_VERSION,
            "recipe_root_relative": self.recipe_root_relative,
            "semantic_checks": self.steps.iter().map(|step| step.name.as_str()).collect::<Vec<_>>(),
            "tool_identities": self.steps.iter().map(tool_identity).collect::<Vec<_>>(),
            "invocation_digest": self.invocation_digest,
            "manifest_digest": self.manifest_digest
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeError {
    pub code: &'static str,
    pub details: Option<Value>,
}

impl RecipeError {
    fn new(code: &'static str) -> Self {
        Self {
            code,
            details: None,
        }
    }

    fn at(mut self, root: String, candidates: &[RecipeId]) -> Self {
        self.details = Some(json!({
            "recipe_root": root,
            "candidate_recipes": candidates.iter().map(|recipe| recipe.as_str()).collect::<Vec<_>>(),
            "detected_markers": candidates.iter().map(|recipe| recipe.marker()).collect::<Vec<_>>()
        }));
        self
    }
}

fn map_project_recipe_error(error: ProjectRecipeResolutionError) -> RecipeError {
    match error {
        ProjectRecipeResolutionError::ExecutionRootUnavailable
        | ProjectRecipeResolutionError::NotFound => RecipeError::new("validation_recipe_not_found"),
        ProjectRecipeResolutionError::CwdMismatch
        | ProjectRecipeResolutionError::ExplicitNotFound { .. } => {
            RecipeError::new("validation_recipe_mismatch")
        }
        ProjectRecipeResolutionError::ExplicitMismatch {
            recipe_root,
            candidates,
            ..
        } => RecipeError::new("validation_recipe_mismatch").at(recipe_root, &candidates),
        ProjectRecipeResolutionError::Ambiguous {
            recipe_root,
            candidates,
        } => RecipeError::new("validation_recipe_ambiguous").at(recipe_root, &candidates),
        ProjectRecipeResolutionError::SourceFileInvalid => manifest_invalid(),
    }
}

pub fn resolve_validation_recipe(
    execution_root: &Path,
    cwd: Option<&str>,
    explicit_recipe: Option<RecipeId>,
    checks: &[SemanticCheck],
    test_filter: Option<&str>,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    resolve_validation_recipe_with_packages(
        execution_root,
        cwd,
        explicit_recipe,
        checks,
        test_filter,
        None,
    )
}

pub fn resolve_validation_recipe_with_packages(
    execution_root: &Path,
    cwd: Option<&str>,
    explicit_recipe: Option<RecipeId>,
    checks: &[SemanticCheck],
    test_filter: Option<&str>,
    package_scope: Option<&[String]>,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    resolve_validation_recipe_with_project_policy(
        execution_root,
        cwd,
        explicit_recipe,
        checks,
        test_filter,
        package_scope,
        false,
        None,
    )
}

/// Canonical project gateway planning. Ordinary recipe workflows retain their
/// existing Python Ruff/mypy/unittest choices. Project Ruff requires a pinned
/// local manifest; evidence profiles cannot manufacture this command authority.
pub fn resolve_project_validation_recipe(
    execution_root: &Path,
    cwd: Option<&str>,
    explicit_recipe: Option<RecipeId>,
    checks: &[SemanticCheck],
    test_filter: Option<&str>,
    package_scope: Option<&[String]>,
    all_packages: bool,
    dependency_policy: Option<webcodex_core::project_validation::ProjectDependencyPolicy>,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    let resolved = resolve_project_recipe_root(execution_root, cwd, explicit_recipe)
        .map_err(map_project_recipe_error)?;
    if resolved.recipe != RecipeId::Python {
        return resolve_validation_recipe_with_project_policy(
            execution_root,
            cwd,
            explicit_recipe,
            checks,
            test_filter,
            package_scope,
            all_packages,
            dependency_policy,
        );
    }
    let ruff_digest = if checks.iter().any(|check| *check != SemanticCheck::Test) {
        Some(project_ruff_manifest_digest(
            &resolved.execution_root,
            &resolved.absolute_root,
        )?)
    } else {
        None
    };
    let mut steps = Vec::with_capacity(checks.len());
    for check in checks {
        let operation = crate::project_validation_operation(
            "python",
            *check,
            package_scope.map(<[String]>::to_vec),
            all_packages,
        )
        .and_then(|operation| operation.with_dependency_policy(dependency_policy))
        .and_then(|operation| operation.with_test_filter(test_filter))
        .map_err(RecipeError::new)?;
        steps.push(if *check == SemanticCheck::Test {
            operation
                .build_readonly_plan()
                .map_err(|_| check_unavailable())?
                .structured_step
        } else {
            ShellJobValidationStep::python_ruff(check.as_str()).ok_or_else(check_unavailable)?
        });
    }
    let test_filter = test_filter
        .map(webcodex_core::runner_protocol::normalize_pytest_filter)
        .transpose()
        .map_err(|_| filter_unsupported())?
        .flatten();
    // pytest searches configuration from the invocation directory through its
    // ancestors. Bind every candidate inside the registered Project, and fail
    // closed if that search could escape to an ambient parent configuration.
    let pytest_digest = if checks.contains(&SemanticCheck::Test) {
        Some(
            digest_project_recipe_files(
                &resolved.execution_root,
                pytest_project_provenance_files(&resolved.execution_root, &resolved.absolute_root)?,
            )
            .map_err(map_project_recipe_error)?,
        )
    } else {
        None
    };
    let manifest_digest = match (pytest_digest, ruff_digest) {
        (Some(pytest), None) => pytest,
        (None, Some(ruff)) => ruff,
        (Some(pytest), Some(ruff)) => format!("{:x}", Sha256::digest(format!("{pytest}\0{ruff}"))),
        (None, None) => return Err(check_unavailable()),
    };
    let invocation_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&steps).map_err(|_| manifest_invalid())?)
    );
    Ok(ResolvedValidationRecipe {
        recipe_id: "python",
        recipe_root_relative: resolved.relative_root,
        steps,
        manifest_digest,
        invocation_digest,
        test_filter,
    })
}

fn project_ruff_manifest_digest(
    execution_root: &Path,
    recipe_root: &Path,
) -> Result<String, RecipeError> {
    use std::io::Read;
    let path = recipe_root
        .join("pyproject.toml")
        .canonicalize()
        .map_err(|_| check_unavailable())?;
    if !path.starts_with(execution_root) {
        return Err(manifest_invalid());
    }
    if !fs::metadata(&path)
        .map_err(|_| manifest_invalid())?
        .is_file()
    {
        return Err(manifest_invalid());
    }
    // Bound both the parse and digest to the same bytes. Never hash a second
    // read that could differ from the config whose authority was just checked.
    let mut bytes = Vec::new();
    const MAX_BYTES: u64 = 1024 * 1024;
    let file = fs::File::open(&path).map_err(|_| manifest_invalid())?;
    if !file.metadata().map_err(|_| manifest_invalid())?.is_file() {
        return Err(manifest_invalid());
    }
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| manifest_invalid())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(manifest_invalid());
    }
    let value: toml::Value =
        toml::from_str(std::str::from_utf8(&bytes).map_err(|_| manifest_invalid())?)
            .map_err(|_| manifest_invalid())?;
    let ruff = value
        .get("tool")
        .and_then(|tool| tool.get("ruff"))
        .and_then(toml::Value::as_table)
        .ok_or_else(check_unavailable)?;
    if ruff.contains_key("extend") {
        return Err(manifest_invalid());
    }
    let target = ruff
        .get("target-version")
        .and_then(toml::Value::as_str)
        .ok_or_else(manifest_invalid)?;
    if !matches!(
        target,
        "py37" | "py38" | "py39" | "py310" | "py311" | "py312" | "py313" | "py314"
    ) {
        return Err(manifest_invalid());
    }
    let mut digest = Sha256::new();
    digest.update(b"webcodex-project-ruff-manifest-v1\0");
    digest.update(
        path.strip_prefix(execution_root)
            .map_err(|_| manifest_invalid())?
            .as_os_str()
            .as_encoded_bytes(),
    );
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
    Ok(format!("{:x}", digest.finalize()))
}

fn pytest_project_provenance_files(
    execution_root: &Path,
    recipe_root: &Path,
) -> Result<Vec<PathBuf>, RecipeError> {
    let mut files = Vec::new();
    let mut directory = recipe_root.to_path_buf();
    let mut project_config_boundary = false;
    loop {
        files.extend(
            PYTEST_PROJECT_CONFIG_FILES
                .into_iter()
                .map(|name| directory.join(name)),
        );
        project_config_boundary |= pytest_config_boundary_in_dir(execution_root, &directory)?;
        if directory == execution_root {
            break;
        }
        directory = directory
            .parent()
            .filter(|parent| parent.starts_with(execution_root))
            .ok_or_else(manifest_invalid)?
            .to_path_buf();
    }

    // Without a recognized Project-local config, pytest keeps searching above
    // cwd before it falls back to setup.py/rootdir. Never let files outside the
    // registered Project silently select config/rootdir for a structured Job.
    if !project_config_boundary {
        for parent in execution_root.ancestors().skip(1) {
            for name in PYTEST_PROJECT_CONFIG_FILES {
                match fs::symlink_metadata(parent.join(name)) {
                    Ok(_) => return Err(manifest_invalid()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(manifest_invalid()),
                }
            }
        }
    }
    Ok(files)
}

fn pytest_config_boundary_in_dir(
    execution_root: &Path,
    directory: &Path,
) -> Result<bool, RecipeError> {
    for name in ["pytest.ini", ".pytest.ini"] {
        match fs::symlink_metadata(directory.join(name)) {
            Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
                return Ok(true)
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(manifest_invalid()),
        }
    }

    let pyproject = directory.join("pyproject.toml");
    if pyproject.exists() {
        let bytes = read_project_recipe_file(execution_root, &pyproject)
            .map_err(map_project_recipe_error)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| manifest_invalid())?;
        let value: toml::Value = toml::from_str(text).map_err(|_| manifest_invalid())?;
        if value
            .get("tool")
            .and_then(toml::Value::as_table)
            .and_then(|tool| tool.get("pytest"))
            .and_then(toml::Value::as_table)
            .is_some_and(|pytest| pytest.get("ini_options").is_some())
        {
            return Ok(true);
        }
    }

    for (name, section) in [("tox.ini", "pytest"), ("setup.cfg", "tool:pytest")] {
        let path = directory.join(name);
        if !path.exists() {
            continue;
        }
        let bytes =
            read_project_recipe_file(execution_root, &path).map_err(map_project_recipe_error)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| manifest_invalid())?;
        if text
            .lines()
            .any(|line| line.trim() == format!("[{section}]"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn resolve_validation_recipe_with_project_policy(
    execution_root: &Path,
    cwd: Option<&str>,
    explicit_recipe: Option<RecipeId>,
    checks: &[SemanticCheck],
    test_filter: Option<&str>,
    package_scope: Option<&[String]>,
    all_packages: bool,
    dependency_policy: Option<webcodex_core::project_validation::ProjectDependencyPolicy>,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    let resolved_root = resolve_project_recipe_root(execution_root, cwd, explicit_recipe)
        .map_err(map_project_recipe_error)?;
    let root = &resolved_root.execution_root;
    let recipe = resolved_root.recipe;
    let recipe_root = &resolved_root.absolute_root;
    if (package_scope.is_some() || all_packages) && !matches!(recipe, RecipeId::Rust | RecipeId::Go)
    {
        return Err(RecipeError::new("validation_scope_unsupported"));
    }
    let root_relative = resolved_root.relative_root.clone();
    let marker_path = resolved_root.marker_path();
    let manifestless_python = if recipe == RecipeId::Python {
        match fs::symlink_metadata(&marker_path) {
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => return Err(manifest_invalid()),
        }
    } else {
        false
    };
    let test_filter = normalize_test_filter(recipe, test_filter)?;
    let (steps, manifest_digest) = if manifestless_python {
        (
            python_manifestless_steps(checks)?,
            format!("{:x}", Sha256::digest(PYTHON_MANIFESTLESS_DIGEST_SEED)),
        )
    } else {
        let manifest =
            read_project_recipe_file(root, &marker_path).map_err(map_project_recipe_error)?;
        match recipe {
            RecipeId::Rust | RecipeId::Go => {
                let steps = canonical_adapter_steps(
                    recipe,
                    checks,
                    test_filter.as_deref(),
                    package_scope,
                    all_packages,
                    dependency_policy,
                )?;
                let manifest_digest = if recipe == RecipeId::Rust && all_packages {
                    digest_project_cargo_all_packages_provenance(&resolved_root)
                        .map_err(|_| RecipeError::new("validation_scope_unavailable"))?
                } else {
                    let provenance_files = project_recipe_provenance_files(&resolved_root)
                        .map_err(map_project_recipe_error)?
                        .expect("canonical project validation adapters are Rust/Go");
                    digest_project_recipe_files(root, provenance_files)
                        .map_err(map_project_recipe_error)?
                };
                (steps, manifest_digest)
            }
            RecipeId::Node => {
                let (steps, extra_digest_files) = node_steps(&recipe_root, &manifest, checks)?;
                let provenance_files: Vec<_> = std::iter::once(marker_path.clone())
                    .chain(
                        extra_digest_files
                            .into_iter()
                            .map(|file| recipe_root.join(file)),
                    )
                    .collect();
                let manifest_digest = digest_project_recipe_files(root, provenance_files)
                    .map_err(map_project_recipe_error)?;
                (steps, manifest_digest)
            }
            RecipeId::Python => {
                let (steps, extra_digest_files) = python_steps(&manifest, checks)?;
                let provenance_files: Vec<_> = std::iter::once(marker_path.clone())
                    .chain(
                        extra_digest_files
                            .into_iter()
                            .map(|file| recipe_root.join(file)),
                    )
                    .collect();
                let manifest_digest = digest_project_recipe_files(root, provenance_files)
                    .map_err(map_project_recipe_error)?;
                (steps, manifest_digest)
            }
        }
    };
    let invocation_digest = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&steps)
                .map_err(|_| RecipeError::new("validation_manifest_invalid"))?
        )
    );
    debug_assert!(steps
        .iter()
        .all(|step| { step.is_canonical() || (all_packages && step.is_canonical_project_step()) }));
    Ok(ResolvedValidationRecipe {
        recipe_id: recipe.as_str(),
        recipe_root_relative: root_relative,
        steps,
        invocation_digest,
        manifest_digest,
        test_filter,
    })
}

/// Shared project recipe resolution supplies filesystem/root facts; validation
/// owns semantic action planning and canonical Rust/Go executable/argv synthesis.
fn canonical_adapter_steps(
    recipe: RecipeId,
    checks: &[SemanticCheck],
    test_filter: Option<&str>,
    package_scope: Option<&[String]>,
    all_packages: bool,
    dependency_policy: Option<webcodex_core::project_validation::ProjectDependencyPolicy>,
) -> Result<Vec<ShellJobValidationStep>, RecipeError> {
    let mut steps = Vec::with_capacity(checks.len());
    for check in checks {
        let operation = crate::project_validation_operation(
            recipe.as_str(),
            *check,
            package_scope.map(<[String]>::to_vec),
            all_packages,
        )
        .map_err(|code| {
            if package_scope.is_none() && code == "validation_action_unsupported" {
                check_unavailable()
            } else {
                RecipeError::new(code)
            }
        })?;
        let operation = operation
            .with_dependency_policy(dependency_policy)
            .map_err(RecipeError::new)?
            .with_test_filter(
                (*check == SemanticCheck::Test)
                    .then_some(test_filter)
                    .flatten(),
            )
            .map_err(RecipeError::new)?;
        let plan = operation.build_readonly_plan().map_err(|_| {
            if package_scope.is_some() || all_packages {
                RecipeError::new("validation_scope_invalid")
            } else {
                check_unavailable()
            }
        })?;
        steps.push(plan.structured_step);
    }
    Ok(steps)
}

/// First production Node project validation: native built-in script runner.
/// The ordinary package-manager recipe below keeps its established semantics.
/// Shared strict Node marker guard for auto and explicit project planning.
pub fn validate_node_project_markers(
    execution_root: &Path,
    cwd: Option<&str>,
) -> Result<(), RecipeError> {
    validate_node_project_marker_chain(execution_root, cwd).map_err(map_project_recipe_error)
}

pub fn resolve_node_native_project_check(
    execution_root: &Path,
    cwd: Option<&str>,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    resolve_node_native_project_action(execution_root, cwd, SemanticCheck::Check)
}

pub fn resolve_node_native_project_test(
    execution_root: &Path,
    cwd: Option<&str>,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    resolve_node_native_project_action(execution_root, cwd, SemanticCheck::Test)
}

fn resolve_node_native_project_action(
    execution_root: &Path,
    cwd: Option<&str>,
    action: SemanticCheck,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    use std::io::Read;
    const MAX_NODE_MANIFEST_BYTES: u64 = 1024 * 1024;

    validate_node_project_markers(execution_root, cwd)?;
    let resolved = resolve_project_recipe_root(execution_root, cwd, Some(RecipeId::Node))
        .map_err(map_project_recipe_error)?;
    let marker = resolved.marker_path();
    // Reject link-like and special manifest files; the project must actually own
    // the selected package.json at its canonical recipe root.
    if !fs::symlink_metadata(&marker).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Err(manifest_invalid());
    }
    let canonical = marker.canonicalize().map_err(|_| manifest_invalid())?;
    if !canonical.starts_with(&resolved.execution_root) {
        return Err(manifest_invalid());
    }
    let file = fs::File::open(&canonical).map_err(|_| manifest_invalid())?;
    let mut manifest = Vec::new();
    file.take(MAX_NODE_MANIFEST_BYTES + 1)
        .read_to_end(&mut manifest)
        .map_err(|_| manifest_invalid())?;
    if manifest.len() as u64 > MAX_NODE_MANIFEST_BYTES {
        return Err(manifest_invalid());
    }
    let value: Value = serde_json::from_slice(&manifest).map_err(|_| manifest_invalid())?;
    let scripts = value
        .as_object()
        .ok_or_else(manifest_invalid)?
        .get("scripts")
        .map(|value| value.as_object().ok_or_else(manifest_invalid))
        .transpose()?;
    let selected = select_node_script(scripts, action)?;
    if scripts
        .and_then(|scripts| scripts.get(selected))
        .and_then(Value::as_str)
        .is_none_or(|body| body.trim().is_empty())
    {
        return Err(manifest_invalid());
    }
    let step = if action == SemanticCheck::Test {
        let body = scripts
            .and_then(|scripts| scripts.get(selected))
            .and_then(Value::as_str)
            .ok_or_else(manifest_invalid)?;
        if !matches!(body, "node --test" | "node --test --test-reporter=tap") {
            return Err(check_unavailable());
        }
        ShellJobValidationStep {
            name: "test".into(),
            program: "node".into(),
            args: vec!["--test".into(), "--test-reporter=tap".into()],
            env: Vec::new(),
        }
    } else {
        ShellJobValidationStep {
            name: "check".into(),
            program: "node".into(),
            args: vec!["--run".into(), selected.into()],
            env: Vec::new(),
        }
    };
    debug_assert!(step.is_canonical());
    let mut hasher = Sha256::new();
    let relative = canonical
        .strip_prefix(&resolved.execution_root)
        .map_err(|_| manifest_invalid())?;
    hasher.update(relative.as_os_str().as_encoded_bytes());
    hasher.update((manifest.len() as u64).to_be_bytes());
    hasher.update(&manifest);
    let manifest_digest = format!("{:x}", hasher.finalize());
    let invocation_digest = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(std::slice::from_ref(&step)).map_err(|_| manifest_invalid())?
        )
    );
    Ok(ResolvedValidationRecipe {
        recipe_id: "node",
        recipe_root_relative: resolved.relative_root,
        steps: vec![step],
        invocation_digest,
        manifest_digest,
        test_filter: None,
    })
}

/// Shared first-present script selector. Validation of body contents is separate:
/// ordinary recipes preserve their old string-only rule; native check rejects
/// empty/whitespace-only bodies without changing the ordinary workflow.
fn select_node_script(
    scripts: Option<&serde_json::Map<String, Value>>,
    check: SemanticCheck,
) -> Result<&'static str, RecipeError> {
    let names: &[&str] = match check {
        SemanticCheck::Format => &["format:check", "format-check", "check:format"],
        SemanticCheck::Check => &["check", "typecheck", "lint"],
        SemanticCheck::Test => &["test"],
    };
    let name = names
        .iter()
        .copied()
        .find(|name| scripts.is_some_and(|scripts| scripts.contains_key(*name)))
        .ok_or_else(check_unavailable)?;
    if !scripts
        .and_then(|scripts| scripts.get(name))
        .is_some_and(Value::is_string)
    {
        return Err(manifest_invalid());
    }
    Ok(name)
}

fn node_steps(
    root: &Path,
    manifest: &[u8],
    checks: &[SemanticCheck],
) -> Result<(Vec<ShellJobValidationStep>, Vec<&'static str>), RecipeError> {
    let value: Value = serde_json::from_slice(manifest).map_err(|_| manifest_invalid())?;
    let object = value.as_object().ok_or_else(manifest_invalid)?;
    let scripts = object
        .get("scripts")
        .map(|value| value.as_object().ok_or_else(manifest_invalid))
        .transpose()?;
    let mut managers = BTreeSet::new();
    if let Some(package_manager) = object.get("packageManager") {
        let raw = package_manager.as_str().ok_or_else(manifest_invalid)?;
        let (manager, version) = raw.split_once('@').ok_or_else(manifest_invalid)?;
        let manager = match manager {
            "npm" => "npm",
            "pnpm" => "pnpm",
            "yarn" => "yarn",
            "bun" => "bun",
            _ => return Err(manifest_invalid()),
        };
        if version.is_empty() || version.chars().any(char::is_whitespace) {
            return Err(manifest_invalid());
        }
        managers.insert(manager);
    }
    let present_locks = NODE_LOCKFILES
        .iter()
        .filter(|(file, _)| root.join(file).is_file())
        .copied()
        .collect::<Vec<_>>();
    managers.extend(present_locks.iter().map(|(_, manager)| *manager));
    if managers.len() != 1 || present_locks.len() > 1 {
        let mut error = RecipeError::new("package_manager_ambiguous");
        error.details = Some(json!({
            "recipe_root": null,
            "candidate_recipes": managers,
            "detected_markers": present_locks.iter().map(|(file, _)| *file).collect::<Vec<_>>()
        }));
        return Err(error);
    }
    let manager = managers.into_iter().next().unwrap();
    let mut steps = Vec::with_capacity(checks.len());
    for check in checks {
        let script = select_node_script(scripts, *check)?;
        let args = vec![
            "run".to_string(),
            "--silent".to_string(),
            script.to_string(),
        ];
        steps.push(step(*check, manager, args));
    }
    Ok((
        steps,
        present_locks.into_iter().map(|(file, _)| file).collect(),
    ))
}

fn python_steps(
    manifest: &[u8],
    checks: &[SemanticCheck],
) -> Result<(Vec<ShellJobValidationStep>, Vec<&'static str>), RecipeError> {
    let value: toml::Value =
        toml::from_str(std::str::from_utf8(manifest).map_err(|_| manifest_invalid())?)
            .map_err(|_| manifest_invalid())?;
    let tool = value.get("tool").and_then(toml::Value::as_table);
    let has = |name| tool.is_some_and(|tools| tools.contains_key(name));
    let mut steps = Vec::with_capacity(checks.len());
    for check in checks {
        let (module, args) = match check {
            SemanticCheck::Format if has("ruff") => {
                ("ruff", vec!["-m", "ruff", "format", "--check"])
            }
            SemanticCheck::Format if has("black") => ("black", vec!["-m", "black", "--check"]),
            SemanticCheck::Check if has("ruff") => ("ruff", vec!["-m", "ruff", "check"]),
            SemanticCheck::Check if has("mypy") => ("mypy", vec!["-m", "mypy"]),
            SemanticCheck::Test if has("pytest") => ("pytest", vec!["-m", "pytest"]),
            _ => return Err(check_unavailable()),
        };
        debug_assert_eq!(
            args.windows(2)
                .find(|pair| pair[0] == "-m")
                .map(|pair| pair[1]),
            Some(module)
        );
        steps.push(step(
            *check,
            "python",
            args.into_iter().map(str::to_string).collect(),
        ));
    }
    Ok((steps, Vec::new()))
}

fn python_manifestless_steps(
    checks: &[SemanticCheck],
) -> Result<Vec<ShellJobValidationStep>, RecipeError> {
    checks
        .iter()
        .map(|check| match check {
            SemanticCheck::Test => Ok(step(
                *check,
                "python",
                PYTHON_UNITTEST_ARGS
                    .iter()
                    .map(|a| (*a).to_string())
                    .collect(),
            )),
            SemanticCheck::Format | SemanticCheck::Check => Err(check_unavailable()),
        })
        .collect()
}

fn step(check: SemanticCheck, program: &str, args: Vec<String>) -> ShellJobValidationStep {
    ShellJobValidationStep {
        name: check.as_str().to_string(),
        program: program.to_string(),
        args,
        env: Vec::new(),
    }
}

fn tool_identity(step: &ShellJobValidationStep) -> String {
    match step.program.as_str() {
        "cargo" if step.name == "format" => "cargo_fmt".to_string(),
        "cargo" => format!("cargo_{}", step.name),
        "python" => format!(
            "python:{}:{}",
            python_module_name(&step.args).unwrap_or("unknown"),
            step.name
        ),
        "go" => format!("go_{}", step.name),
        manager => format!(
            "{manager}:{}",
            step.args.last().map(String::as_str).unwrap_or("unknown")
        ),
    }
}

fn python_module_name(args: &[String]) -> Option<&str> {
    args.windows(2)
        .find(|pair| pair[0] == "-m")
        .map(|pair| pair[1].as_str())
}

fn normalize_test_filter(
    recipe: RecipeId,
    filter: Option<&str>,
) -> Result<Option<String>, RecipeError> {
    match recipe {
        RecipeId::Rust => safe_rust_filter(filter),
        RecipeId::Go => filter
            .map(webcodex_core::runner_protocol::normalize_go_test_filter)
            .transpose()
            .map(Option::flatten)
            .map_err(|_| filter_unsupported()),
        _ => {
            reject_filter(filter)?;
            Ok(None)
        }
    }
}

fn safe_rust_filter(filter: Option<&str>) -> Result<Option<String>, RecipeError> {
    let Some(raw) = filter else {
        return Ok(None);
    };
    // The shared filter contract performs the single trim, rejects option-like
    // values and control bytes, and bounds the length.
    match normalize_rust_test_filter(raw) {
        Ok(Some(normalized)) => Ok(Some(normalized)),
        Ok(None) => Ok(None),
        Err(_) => Err(filter_unsupported()),
    }
}

fn reject_filter(filter: Option<&str>) -> Result<(), RecipeError> {
    if filter.is_some() {
        Err(filter_unsupported())
    } else {
        Ok(())
    }
}

fn manifest_invalid() -> RecipeError {
    RecipeError::new("validation_manifest_invalid")
}

fn check_unavailable() -> RecipeError {
    RecipeError::new("validation_check_unavailable")
}

fn filter_unsupported() -> RecipeError {
    RecipeError::new("test_filter_unsupported")
}

/// Detect using the same nearest-root and ambiguity rules as recipe resolution.
/// This allows production admission to reject deferred backends before asking
/// those backends for scripts or installed tooling.
pub fn detect_validation_recipe(
    root: &Path,
    cwd: Option<&str>,
    hint: Option<RecipeId>,
) -> Result<RecipeId, RecipeError> {
    resolve_project_recipe_root(root, cwd, hint)
        .map(|resolved| resolved.recipe)
        .map_err(map_project_recipe_error)
}
