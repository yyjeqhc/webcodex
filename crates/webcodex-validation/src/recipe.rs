//! Deterministic project-aware plans resolved on the owning Runner.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use webcodex_core::runner_protocol::{normalize_rust_test_filter, ShellJobValidationStep};
use webcodex_workspace::project_recipe::{
    digest_project_recipe_files, read_project_recipe_file, resolve_project_recipe_root,
    ProjectRecipeResolutionError,
};

pub use webcodex_workspace::project_recipe::ProjectRecipeId as RecipeId;

const RECIPE_VERSION: u32 = 1;
const PYTHON_MANIFESTLESS_DIGEST_SEED: &[u8] = b"webcodex.python.manifestless.recipe.v1";
const PYTHON_UNITTEST_ARGS: [&str; 5] = ["-B", "-m", "unittest", "discover", "-v"];
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
    let resolved_root = resolve_project_recipe_root(execution_root, cwd, explicit_recipe)
        .map_err(map_project_recipe_error)?;
    let root = &resolved_root.execution_root;
    let recipe = resolved_root.recipe;
    let recipe_root = &resolved_root.absolute_root;
    if package_scope.is_some() && !matches!(recipe, RecipeId::Rust | RecipeId::Go) {
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
        let (steps, extra_digest_files) = match recipe {
            RecipeId::Rust | RecipeId::Go => {
                canonical_adapter_steps(recipe, checks, test_filter.as_deref(), package_scope)?
            }
            RecipeId::Node => node_steps(&recipe_root, &manifest, checks)?,
            RecipeId::Python => python_steps(&manifest, checks)?,
        };
        let manifest_digest = digest_project_recipe_files(
            root,
            std::iter::once(marker_path).chain(
                extra_digest_files
                    .into_iter()
                    .map(|file| recipe_root.join(file)),
            ),
        )
        .map_err(map_project_recipe_error)?;
        (steps, manifest_digest)
    };
    let invocation_digest = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&steps)
                .map_err(|_| RecipeError::new("validation_manifest_invalid"))?
        )
    );
    debug_assert!(steps.iter().all(ShellJobValidationStep::is_canonical));
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
) -> Result<(Vec<ShellJobValidationStep>, Vec<&'static str>), RecipeError> {
    let mut steps = Vec::with_capacity(checks.len());
    for check in checks {
        let operation = crate::project_validation_operation(
            recipe.as_str(),
            *check,
            package_scope.map(<[String]>::to_vec),
        )
        .map_err(|code| {
            if package_scope.is_none() && code == "validation_action_unsupported" {
                check_unavailable()
            } else {
                RecipeError::new(code)
            }
        })?;
        let operation = operation
            .with_test_filter(
                (*check == SemanticCheck::Test)
                    .then_some(test_filter)
                    .flatten(),
            )
            .map_err(RecipeError::new)?;
        let plan = operation.build_readonly_plan().map_err(|_| {
            if package_scope.is_some() {
                RecipeError::new("validation_scope_invalid")
            } else {
                check_unavailable()
            }
        })?;
        steps.push(plan.structured_step);
    }
    let extra_digest_files = match recipe {
        RecipeId::Rust => vec!["Cargo.lock"],
        RecipeId::Go => vec!["go.sum"],
        RecipeId::Node | RecipeId::Python => unreachable!("canonical project adapters are Rust/Go"),
    };
    Ok((steps, extra_digest_files))
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
        let names: &[&str] = match check {
            SemanticCheck::Format => &["format:check", "format-check", "check:format"],
            SemanticCheck::Check => &["check", "typecheck", "lint"],
            SemanticCheck::Test => &["test"],
        };
        let script = names
            .iter()
            .copied()
            .find(|name| scripts.is_some_and(|scripts| scripts.get(*name).is_some()))
            .ok_or_else(check_unavailable)?;
        if !scripts
            .and_then(|scripts| scripts.get(script))
            .is_some_and(Value::is_string)
        {
            return Err(manifest_invalid());
        }
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
