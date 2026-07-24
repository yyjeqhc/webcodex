//! Deterministic project-aware plans for the hosted `checks_run` capability.

use crate::shell_protocol::ShellJobValidationStep;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

const RECIPE_VERSION: u32 = 1;
const RECIPE_NAMES: [&str; 4] = ["rust", "node", "python", "go"];
const RECIPE_MARKERS: [&str; 4] = ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"];
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub(crate) enum RecipeId {
    Rust,
    Node,
    Python,
    Go,
}

impl RecipeId {
    pub(crate) fn as_str(self) -> &'static str {
        RECIPE_NAMES[self as usize]
    }

    fn marker(self) -> &'static str {
        RECIPE_MARKERS[self as usize]
    }

    fn all() -> [Self; 4] {
        [Self::Rust, Self::Node, Self::Python, Self::Go]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub(crate) enum SemanticCheck {
    Format,
    Check,
    Test,
}

impl SemanticCheck {
    pub(crate) fn as_str(self) -> &'static str {
        ["format", "check", "test"][self as usize]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedValidationRecipe {
    pub(crate) recipe_id: &'static str,
    pub(crate) recipe_root_relative: String,
    pub(crate) steps: Vec<ShellJobValidationStep>,
    pub(crate) invocation_digest: String,
    pub(crate) manifest_digest: String,
    /// Normalized (trimmed, validated) test filter actually placed in the plan,
    /// or `None`. Bound into the request hash so retries key on the executed
    /// value, not the raw request string.
    pub(crate) test_filter: Option<String>,
}

impl ResolvedValidationRecipe {
    pub(crate) fn durable_identity(&self) -> Value {
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
pub(crate) struct RecipeError {
    pub(crate) code: &'static str,
    pub(crate) details: Option<Value>,
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

pub(crate) fn resolve_validation_recipe(
    execution_root: &Path,
    cwd: Option<&str>,
    explicit_recipe: Option<RecipeId>,
    checks: &[SemanticCheck],
    test_filter: Option<&str>,
) -> Result<ResolvedValidationRecipe, RecipeError> {
    let root = execution_root
        .canonicalize()
        .map_err(|_| RecipeError::new("validation_recipe_not_found"))?;
    let cwd = resolve_cwd(&root, cwd)?;
    let (recipe, recipe_root) = nearest_recipe_root(&root, &cwd, explicit_recipe)?;
    let root_relative = relative_root(&root, &recipe_root);
    let marker = recipe.marker();
    let marker_path = recipe_root.join(marker);
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
        let manifest = read_manifest(&root, &marker_path)?;
        let (steps, extra_digest_files) = match recipe {
            RecipeId::Rust => rust_steps(checks, test_filter.as_deref()),
            RecipeId::Node => node_steps(&recipe_root, &manifest, checks)?,
            RecipeId::Python => python_steps(&manifest, checks)?,
            RecipeId::Go => go_steps(checks)?,
        };
        let manifest_digest = digest_files(
            &root,
            std::iter::once(marker_path).chain(
                extra_digest_files
                    .into_iter()
                    .map(|file| recipe_root.join(file)),
            ),
        )?;
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

fn resolve_cwd(root: &Path, raw: Option<&str>) -> Result<PathBuf, RecipeError> {
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
        return Err(RecipeError::new("validation_recipe_mismatch"));
    }
    let cwd = root
        .join(path)
        .canonicalize()
        .map_err(|_| RecipeError::new("validation_recipe_mismatch"))?;
    if !cwd.starts_with(root) || !cwd.is_dir() {
        return Err(RecipeError::new("validation_recipe_mismatch"));
    }
    Ok(cwd)
}

fn nearest_recipe_root(
    root: &Path,
    cwd: &Path,
    explicit: Option<RecipeId>,
) -> Result<(RecipeId, PathBuf), RecipeError> {
    let mut directory = cwd.to_path_buf();
    loop {
        let candidates = RecipeId::all()
            .into_iter()
            .filter(|recipe| directory.join(recipe.marker()).is_file())
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            let relative = relative_root(root, &directory);
            if let Some(explicit) = explicit {
                if candidates.contains(&explicit) {
                    return Ok((explicit, directory));
                }
                if explicit == RecipeId::Python {
                    return Ok((RecipeId::Python, cwd.to_path_buf()));
                }
                return Err(
                    RecipeError::new("validation_recipe_mismatch").at(relative, &candidates)
                );
            }
            if candidates.len() == 1 {
                return Ok((candidates[0], directory));
            }
            let mut candidates = candidates;
            candidates.sort_by_key(|recipe| recipe.as_str());
            return Err(RecipeError::new("validation_recipe_ambiguous").at(relative, &candidates));
        }
        if directory == root {
            break;
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent.to_path_buf();
    }
    if explicit == Some(RecipeId::Python) {
        return Ok((RecipeId::Python, cwd.to_path_buf()));
    }
    let code = if explicit.is_some() {
        "validation_recipe_mismatch"
    } else {
        "validation_recipe_not_found"
    };
    Err(RecipeError::new(code))
}

fn rust_steps(
    checks: &[SemanticCheck],
    filter: Option<&str>,
) -> (Vec<ShellJobValidationStep>, Vec<&'static str>) {
    let mut steps = Vec::with_capacity(checks.len());
    for check in checks {
        let args = match check {
            SemanticCheck::Format => {
                vec!["fmt".to_string(), "--".to_string(), "--check".to_string()]
            }
            SemanticCheck::Check => vec!["check".to_string(), "--all-targets".to_string()],
            SemanticCheck::Test => {
                let mut args = vec!["test".to_string()];
                if let Some(filter) = filter {
                    args.push(filter.to_string());
                }
                args
            }
        };
        steps.push(step(*check, "cargo", args));
    }
    (steps, vec!["Cargo.lock"])
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

fn go_steps(
    checks: &[SemanticCheck],
) -> Result<(Vec<ShellJobValidationStep>, Vec<&'static str>), RecipeError> {
    let mut steps = Vec::with_capacity(checks.len());
    for check in checks {
        let args = match check {
            SemanticCheck::Format => return Err(check_unavailable()),
            SemanticCheck::Check => vec!["vet", "./..."],
            SemanticCheck::Test => vec!["test", "./..."],
        };
        steps.push(step(
            *check,
            "go",
            args.into_iter().map(str::to_string).collect(),
        ));
    }
    Ok((steps, vec!["go.sum"]))
}

fn step(check: SemanticCheck, program: &str, args: Vec<String>) -> ShellJobValidationStep {
    ShellJobValidationStep {
        name: check.as_str().to_string(),
        program: program.to_string(),
        args,
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
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else if crate::shell_protocol::valid_rust_test_filter(trimmed) {
        Ok(Some(trimmed.to_string()))
    } else {
        Err(filter_unsupported())
    }
}

fn reject_filter(filter: Option<&str>) -> Result<(), RecipeError> {
    if filter.is_some() {
        Err(filter_unsupported())
    } else {
        Ok(())
    }
}

fn read_manifest(execution_root: &Path, path: &Path) -> Result<Vec<u8>, RecipeError> {
    let canonical = path.canonicalize().map_err(|_| manifest_invalid())?;
    if !canonical.starts_with(execution_root) || !canonical.is_file() {
        return Err(manifest_invalid());
    }
    fs::read(canonical).map_err(|_| manifest_invalid())
}

fn digest_files<I>(execution_root: &Path, paths: I) -> Result<String, RecipeError>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut hasher = Sha256::new();
    for path in paths {
        if !path.exists() {
            continue;
        }
        let canonical = path.canonicalize().map_err(|_| manifest_invalid())?;
        if !canonical.starts_with(execution_root) || !canonical.is_file() {
            return Err(manifest_invalid());
        }
        let content = fs::read(&canonical).map_err(|_| manifest_invalid())?;
        let relative = canonical
            .strip_prefix(execution_root)
            .map_err(|_| manifest_invalid())?;
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update((content.len() as u64).to_be_bytes());
        hasher.update(content);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn relative_root(root: &Path, recipe_root: &Path) -> String {
    let relative = recipe_root.strip_prefix(root).unwrap_or(Path::new(""));
    if relative.as_os_str().is_empty() {
        ".".to_string()
    } else {
        relative.to_string_lossy().replace('\\', "/")
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
