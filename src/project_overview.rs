//! Deterministic, bounded project-structure inspection shared by the runtime
//! local-executor parity path and `webcodex-agent`.
//!
//! This module reads directory entries and file types only. It never reads file
//! contents, follows symlinks, invokes a shell, or consults environment values.

use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Component, Path, PathBuf};

pub(crate) const PROJECT_OVERVIEW_DEFAULT_MAX_DEPTH: usize = 2;
pub(crate) const PROJECT_OVERVIEW_MIN_MAX_DEPTH: usize = 1;
pub(crate) const PROJECT_OVERVIEW_MAX_MAX_DEPTH: usize = 4;
pub(crate) const PROJECT_OVERVIEW_DEFAULT_LIMIT: usize = 200;
pub(crate) const PROJECT_OVERVIEW_MIN_LIMIT: usize = 20;
pub(crate) const PROJECT_OVERVIEW_MAX_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone)]
struct ScanEntry {
    path: String,
    scoped_path: String,
    depth: usize,
    kind: EntryKind,
}

#[derive(Debug)]
struct PendingDirectory {
    absolute_path: PathBuf,
    scoped_path: String,
    depth: usize,
}

#[derive(Debug)]
struct KeyFile {
    path: String,
    kind: &'static str,
    reason: &'static str,
    priority: u8,
    lockfile: bool,
}

/// Normalize a caller-supplied project-relative directory path. The returned
/// string is empty for the project root and otherwise uses `/` separators.
pub(crate) fn normalize_project_overview_path(path: &str) -> Result<String, String> {
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "." {
        return Ok(String::new());
    }
    let raw = Path::new(trimmed);
    if raw.is_absolute() {
        return Err("path must be project-relative".to_string());
    }
    let mut parts = Vec::new();
    for component in raw.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| "path must be valid UTF-8".to_string())?;
                parts.push(part.to_string());
            }
            Component::ParentDir => return Err("path cannot contain parent traversal".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("path must be project-relative".to_string())
            }
        }
    }
    let normalized = parts.join("/");
    if is_project_overview_excluded_path(&normalized) {
        return Err("path is protected or excluded from project overview scans".to_string());
    }
    Ok(normalized)
}

pub(crate) fn effective_project_overview_max_depth(value: Option<usize>) -> usize {
    value.unwrap_or(PROJECT_OVERVIEW_DEFAULT_MAX_DEPTH).clamp(
        PROJECT_OVERVIEW_MIN_MAX_DEPTH,
        PROJECT_OVERVIEW_MAX_MAX_DEPTH,
    )
}

pub(crate) fn effective_project_overview_limit(value: Option<usize>) -> usize {
    value
        .unwrap_or(PROJECT_OVERVIEW_DEFAULT_LIMIT)
        .clamp(PROJECT_OVERVIEW_MIN_LIMIT, PROJECT_OVERVIEW_MAX_LIMIT)
}

/// Scan `requested_path` under `project_root` and build the agent-owned
/// project-overview payload. The runtime adds the opaque runtime project id.
pub(crate) fn build_project_overview(
    project_root: &Path,
    requested_path: &str,
    max_depth: Option<usize>,
    limit: Option<usize>,
) -> Result<Value, String> {
    let path = normalize_project_overview_path(requested_path)?;
    let max_depth = effective_project_overview_max_depth(max_depth);
    let limit = effective_project_overview_limit(limit);

    let canonical_root = project_root
        .canonicalize()
        .map_err(|error| format!("project root does not exist: {error}"))?;
    let requested = if path.is_empty() {
        canonical_root.clone()
    } else {
        canonical_root.join(&path)
    };
    let canonical_scope = requested
        .canonicalize()
        .map_err(|error| format!("path does not exist: {error}"))?;
    if !canonical_scope.starts_with(&canonical_root) {
        return Err("path is outside project directory".to_string());
    }
    if !canonical_scope.is_dir() {
        return Err("path is not a directory".to_string());
    }

    let mut queue = VecDeque::from([PendingDirectory {
        absolute_path: canonical_scope,
        scoped_path: String::new(),
        depth: 0,
    }]);
    let mut entries = Vec::new();
    let mut limit_truncated = false;
    let mut depth_truncated = false;
    let mut skipped_symlink = false;
    let mut skipped_unreadable = false;
    let mut skipped_non_utf8 = false;

    'directories: while let Some(directory) = queue.pop_front() {
        if entries.len() >= limit {
            limit_truncated = true;
            break;
        }
        let read_dir = match std::fs::read_dir(&directory.absolute_path) {
            Ok(read_dir) => read_dir,
            Err(error) if directory.depth == 0 => {
                return Err(format!("failed to read directory: {error}"))
            }
            Err(_) => {
                skipped_unreadable = true;
                continue;
            }
        };
        let mut children = Vec::new();
        for child in read_dir {
            match child {
                Ok(child) => children.push(child),
                Err(_) => skipped_unreadable = true,
            }
        }
        children.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

        for child in children {
            let Some(name) = child.file_name().to_str().map(str::to_string) else {
                skipped_non_utf8 = true;
                continue;
            };
            let scoped_path = join_relative(&directory.scoped_path, &name);
            let project_path = join_relative(&path, &scoped_path);
            if is_project_overview_excluded_path(&project_path) {
                continue;
            }
            let file_type = match child.file_type() {
                Ok(file_type) => file_type,
                Err(_) => {
                    skipped_unreadable = true;
                    continue;
                }
            };
            if file_type.is_symlink() {
                skipped_symlink = true;
                continue;
            }
            let kind = if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_file() {
                EntryKind::File
            } else {
                continue;
            };
            if entries.len() >= limit {
                limit_truncated = true;
                break 'directories;
            }
            let depth = directory.depth + 1;
            entries.push(ScanEntry {
                path: project_path,
                scoped_path: scoped_path.clone(),
                depth,
                kind,
            });
            if kind == EntryKind::Directory {
                if depth < max_depth {
                    queue.push_back(PendingDirectory {
                        absolute_path: child.path(),
                        scoped_path,
                        depth,
                    });
                } else {
                    depth_truncated = true;
                }
            }
        }
    }

    let project_types = project_types(&entries);
    let manifests = manifests(&entries);
    let key_files = key_files(&entries);
    let suggested_next_reads = suggested_next_reads(&key_files);
    let roots = roots(&entries);
    let top_level = top_level(&entries);
    let truncation_reason = match (limit_truncated, depth_truncated) {
        (true, true) => Some("limit_and_max_depth"),
        (true, false) => Some("limit"),
        (false, true) => Some("max_depth"),
        (false, false) => None,
    };
    let mut warnings = Vec::new();
    if skipped_symlink {
        warnings.push("symlinks_skipped");
    }
    if skipped_unreadable {
        warnings.push("unreadable_entries_skipped");
    }
    if skipped_non_utf8 {
        warnings.push("non_utf8_paths_skipped");
    }

    Ok(json!({
        "schema_version": 1,
        "path": path,
        "deterministic": true,
        "project_types": project_types,
        "manifests": manifests,
        "key_files": key_files_to_json(&key_files),
        "roots": roots,
        "top_level": top_level,
        "suggested_next_reads": suggested_next_reads,
        "scan": {
            "max_depth": max_depth,
            "limit": limit,
            "returned_entry_count": entries.len(),
            "truncated": limit_truncated || depth_truncated,
            "truncation_reason": truncation_reason,
        },
        "warnings": warnings,
    }))
}

fn join_relative(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn is_project_overview_excluded_path(path: &str) -> bool {
    crate::workspace_checkpoint::sensitive_path(path) || path.split('/').any(is_excluded_component)
}

fn is_excluded_component(component: &str) -> bool {
    let lower = component.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        ".git"
            | "target"
            | "node_modules"
            | "vendor"
            | "dist"
            | "build"
            | ".next"
            | "coverage"
            | "cache"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".tox"
            | ".cache"
            | ".gradle"
            | ".parcel-cache"
            | ".pnpm-store"
            | ".turbo"
            | "projects.d"
            | "secrets"
            | "secret"
            | "tokens"
            | "token"
            | "credentials"
            | "credential"
            | "passwords"
            | "password"
            | "agent.toml"
            | "webcodex.env"
            | ".env"
            | ".npmrc"
            | ".netrc"
            | ".pypirc"
            | ".ssh"
            | ".aws"
            | "id_rsa"
            | "id_ed25519"
    ) {
        return true;
    }
    lower.starts_with(".env.")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".p12")
        || lower.ends_with(".pfx")
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn project_types(entries: &[ScanEntry]) -> Vec<Value> {
    let mut evidence: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for entry in entries.iter().filter(|entry| entry.kind == EntryKind::File) {
        if let Some((kind, _)) = manifest_kind(&entry.path) {
            evidence.entry(kind).or_default().push(entry.path.clone());
        }
    }
    const ORDER: &[&str] = &[
        "rust", "node", "python", "go", "jvm", "dotnet", "ruby", "php", "cpp",
    ];
    ORDER
        .iter()
        .filter_map(|kind| {
            evidence.remove(kind).map(|mut paths| {
                paths.sort();
                paths.dedup();
                json!({"kind": kind, "evidence": paths})
            })
        })
        .collect()
}

fn manifests(entries: &[ScanEntry]) -> Vec<Value> {
    let mut values = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .filter_map(|entry| {
            manifest_kind(&entry.path).map(|(_, kind)| {
                json!({
                    "path": entry.path,
                    "kind": kind,
                })
            })
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    values
}

fn manifest_kind(path: &str) -> Option<(&'static str, &'static str)> {
    let name = basename(path);
    match name {
        "Cargo.toml" => Some(("rust", "rust_manifest")),
        "package.json" => Some(("node", "node_manifest")),
        "pyproject.toml" | "setup.py" | "setup.cfg" | "requirements.txt" | "Pipfile" => {
            Some(("python", "python_manifest"))
        }
        "go.mod" => Some(("go", "go_manifest")),
        "pom.xml" | "build.gradle" | "build.gradle.kts" => Some(("jvm", "jvm_manifest")),
        "Gemfile" => Some(("ruby", "ruby_manifest")),
        "composer.json" => Some(("php", "php_manifest")),
        "CMakeLists.txt" | "meson.build" => Some(("cpp", "cpp_manifest")),
        _ if name.ends_with(".sln") => Some(("dotnet", "dotnet_solution")),
        _ if name.ends_with(".csproj") => Some(("dotnet", "dotnet_project")),
        _ => None,
    }
}

fn key_files(entries: &[ScanEntry]) -> Vec<KeyFile> {
    let mut keys = entries
        .iter()
        .filter_map(classify_key_file)
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.path.cmp(&right.path))
    });
    keys
}

fn classify_key_file(entry: &ScanEntry) -> Option<KeyFile> {
    let name = basename(&entry.path);
    let lower_name = name.to_ascii_lowercase();
    let lower_scoped = entry.scoped_path.to_ascii_lowercase();
    let file = entry.kind == EntryKind::File;
    let directory = entry.kind == EntryKind::Directory;

    if file
        && (lower_name == "agents.md"
            || lower_name == "claude.md"
            || lower_scoped == ".github/copilot-instructions.md")
    {
        return Some(key(
            entry,
            "agent_instructions",
            "project-local agent guidance",
            0,
            false,
        ));
    }
    if file && is_readme(&lower_name) {
        return Some(key(entry, "readme", "project overview", 1, false));
    }
    if file && is_primary_key_manifest(name) {
        return Some(key(entry, "manifest", manifest_reason(name), 2, false));
    }
    if file && is_documentation_entrypoint(&lower_scoped, &lower_name) {
        return Some(key(
            entry,
            "documentation",
            "contribution or development documentation",
            3,
            false,
        ));
    }
    if file && is_lockfile(name) {
        return Some(key(entry, "lockfile", "dependency lockfile", 4, true));
    }
    if (file && is_ci_or_container_file(&lower_scoped, &lower_name))
        || (directory && lower_scoped == ".github/workflows")
    {
        return Some(key(
            entry,
            if lower_name.contains("docker") || lower_name.starts_with("compose.") {
                "container"
            } else {
                "ci"
            },
            "container or continuous-integration entrypoint",
            5,
            false,
        ));
    }
    None
}

fn key(
    entry: &ScanEntry,
    kind: &'static str,
    reason: &'static str,
    priority: u8,
    lockfile: bool,
) -> KeyFile {
    KeyFile {
        path: entry.path.clone(),
        kind,
        reason,
        priority,
        lockfile,
    }
}

fn is_readme(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "readme" | "readme.md" | "readme.rst" | "readme.txt"
    )
}

fn is_primary_key_manifest(name: &str) -> bool {
    matches!(
        name,
        "Cargo.toml"
            | "package.json"
            | "pyproject.toml"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "Gemfile"
            | "composer.json"
            | "CMakeLists.txt"
            | "meson.build"
    ) || name.ends_with(".sln")
        || name.ends_with(".csproj")
}

fn manifest_reason(name: &str) -> &'static str {
    match name {
        "Cargo.toml" => "Rust workspace or package metadata",
        "package.json" => "Node package metadata",
        "pyproject.toml" => "Python project metadata",
        "go.mod" => "Go module metadata",
        "pom.xml" | "build.gradle" | "build.gradle.kts" => "JVM build metadata",
        "Gemfile" => "Ruby dependency metadata",
        "composer.json" => "PHP package metadata",
        "CMakeLists.txt" | "meson.build" => "C or C++ build metadata",
        _ if name.ends_with(".sln") || name.ends_with(".csproj") => ".NET project metadata",
        _ => "build or package metadata",
    }
}

fn is_documentation_entrypoint(lower_scoped: &str, lower_name: &str) -> bool {
    matches!(lower_name, "contributing.md" | "developing.md")
        || matches!(lower_scoped, "docs/index.md" | "docs/readme.md")
}

fn is_lockfile(name: &str) -> bool {
    matches!(
        name,
        "Cargo.lock"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "poetry.lock"
            | "uv.lock"
            | "Pipfile.lock"
            | "go.sum"
            | "composer.lock"
            | "Gemfile.lock"
    )
}

fn is_ci_or_container_file(lower_scoped: &str, lower_name: &str) -> bool {
    lower_name == "dockerfile"
        || matches!(
            lower_name,
            "docker-compose.yml"
                | "docker-compose.yaml"
                | "compose.yml"
                | "compose.yaml"
                | ".gitlab-ci.yml"
                | ".gitlab-ci.yaml"
                | "azure-pipelines.yml"
                | "jenkinsfile"
        )
        || lower_scoped.starts_with(".github/workflows/")
        || lower_scoped == ".circleci/config.yml"
}

fn key_files_to_json(keys: &[KeyFile]) -> Vec<Value> {
    keys.iter()
        .map(|key| {
            json!({
                "path": key.path,
                "kind": key.kind,
                "reason": key.reason,
            })
        })
        .collect()
}

fn suggested_next_reads(keys: &[KeyFile]) -> Vec<Value> {
    let mut candidates = keys.iter().filter(|key| !key.lockfile).collect::<Vec<_>>();
    if candidates.is_empty() {
        candidates = keys.iter().collect();
    }
    candidates
        .into_iter()
        .take(12)
        .map(|key| json!({"path": key.path, "reason": key.reason}))
        .collect()
}

fn roots(entries: &[ScanEntry]) -> Value {
    let mut source = Vec::new();
    let mut tests = Vec::new();
    let mut docs = Vec::new();
    let mut examples = Vec::new();
    let mut scripts = Vec::new();
    let mut ci = Vec::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Directory && entry.depth == 1)
    {
        let name = basename(&entry.path).to_ascii_lowercase();
        let target = match name.as_str() {
            "src" | "crates" | "packages" | "app" | "apps" | "lib" | "cmd" | "internal" => {
                Some(&mut source)
            }
            "tests" | "test" | "spec" | "__tests__" => Some(&mut tests),
            "docs" | "doc" => Some(&mut docs),
            "examples" | "example" => Some(&mut examples),
            "scripts" | "tools" => Some(&mut scripts),
            ".github" | ".gitlab" | "ci" => Some(&mut ci),
            _ => None,
        };
        if let Some(target) = target {
            target.push(entry.path.clone());
        }
    }
    for paths in [
        &mut source,
        &mut tests,
        &mut docs,
        &mut examples,
        &mut scripts,
        &mut ci,
    ] {
        paths.sort();
        paths.dedup();
    }
    json!({
        "source": source,
        "tests": tests,
        "docs": docs,
        "examples": examples,
        "scripts": scripts,
        "ci": ci,
        "classification_basis": "conventional_directory_name",
    })
}

fn top_level(entries: &[ScanEntry]) -> Vec<Value> {
    let mut values = entries
        .iter()
        .filter(|entry| entry.depth == 1)
        .map(|entry| {
            json!({
                "path": entry.path,
                "kind": match entry.kind {
                    EntryKind::File => "file",
                    EntryKind::Directory => "directory",
                },
            })
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(root: &Path, path: &str) {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"fixture contents must never be read").unwrap();
    }

    #[test]
    fn project_overview_rust_fixture_is_bounded_safe_and_deterministic() {
        let temp = tempfile::tempdir().unwrap();
        for path in [
            "AGENTS.md",
            "README.md",
            "Cargo.toml",
            "Cargo.lock",
            "src/lib.rs",
            "tests/basic.rs",
            "docs/README.md",
            ".github/workflows/ci.yml",
            "target/debug/output",
            ".env",
        ] {
            touch(temp.path(), path);
        }

        let first = build_project_overview(temp.path(), "", None, None).unwrap();
        let second = build_project_overview(temp.path(), "", None, None).unwrap();
        assert_eq!(first, second);
        assert_eq!(first["deterministic"], true);
        assert!(first["project_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|kind| kind["kind"] == "rust"));
        let serialized = first.to_string();
        for expected in ["AGENTS.md", "README.md", "Cargo.toml"] {
            assert!(
                serialized.contains(expected),
                "missing {expected}: {serialized}"
            );
        }
        assert!(!serialized.contains("target"));
        assert!(!serialized.contains(".env"));
        assert!(!serialized.contains("fixture contents"));
        assert!(!serialized.contains(&temp.path().display().to_string()));
        assert_eq!(first["roots"]["source"], json!(["src"]));
        assert_eq!(first["roots"]["tests"], json!(["tests"]));
        assert_eq!(first["roots"]["docs"], json!(["docs"]));
        assert_eq!(first["roots"]["ci"], json!([".github"]));
    }

    #[test]
    fn project_overview_detects_stably_ordered_monorepo_types() {
        let temp = tempfile::tempdir().unwrap();
        for path in [
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "crates/example/Cargo.toml",
            "packages/app/package.json",
        ] {
            touch(temp.path(), path);
        }
        let output = build_project_overview(temp.path(), "", Some(4), None).unwrap();
        let kinds = output["project_types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["kind"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(kinds, ["rust", "node", "python"]);
        assert_eq!(output["roots"]["source"], json!(["crates", "packages"]));
    }

    #[test]
    fn project_overview_scopes_paths_and_clamps_bounds() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..25 {
            touch(
                temp.path(),
                &format!("crates/example/src/file_{index:02}.rs"),
            );
        }
        touch(temp.path(), "outside/Cargo.toml");
        let output =
            build_project_overview(temp.path(), "crates/example", Some(0), Some(1)).unwrap();
        assert_eq!(output["path"], "crates/example");
        assert_eq!(output["scan"]["max_depth"], 1);
        assert_eq!(output["scan"]["limit"], 20);
        assert_eq!(output["scan"]["truncated"], true);
        assert!(
            output["scan"]["truncation_reason"] == "max_depth"
                || output["scan"]["truncation_reason"] == "limit_and_max_depth"
        );
        let serialized = output.to_string();
        assert!(!serialized.contains("outside/Cargo.toml"));
        for entry in output["top_level"].as_array().unwrap() {
            assert!(entry["path"]
                .as_str()
                .unwrap()
                .starts_with("crates/example/"));
        }
    }

    #[test]
    fn project_overview_limit_truncation_is_normal_bounded_output() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..25 {
            touch(temp.path(), &format!("file_{index:02}.txt"));
        }

        let output = build_project_overview(temp.path(), "", Some(99), Some(1)).unwrap();
        assert_eq!(output["scan"]["max_depth"], 4);
        assert_eq!(output["scan"]["limit"], 20);
        assert_eq!(output["scan"]["returned_entry_count"], 20);
        assert_eq!(output["scan"]["truncated"], true);
        assert_eq!(output["scan"]["truncation_reason"], "limit");
        assert_eq!(output["top_level"].as_array().unwrap().len(), 20);
    }

    #[test]
    fn project_overview_rejects_escape_and_does_not_follow_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        assert!(build_project_overview(temp.path(), "../outside", None, None).is_err());
        assert!(build_project_overview(temp.path(), "/tmp", None, None).is_err());
        for protected in [".git", "target", "node_modules", ".env", "secrets"] {
            assert!(
                build_project_overview(temp.path(), protected, None, None).is_err(),
                "protected scope {protected} must be rejected before scanning"
            );
        }

        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            touch(outside.path(), "Cargo.toml");
            std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).unwrap();
            let output = build_project_overview(temp.path(), "", None, None).unwrap();
            assert!(!output.to_string().contains("escape"));
            assert_eq!(output["warnings"], json!(["symlinks_skipped"]));
        }
    }
}
