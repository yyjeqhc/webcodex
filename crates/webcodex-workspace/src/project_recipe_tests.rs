use super::project_recipe::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn resolves_each_recipe_marker_and_nearest_nested_root() {
    for (marker, expected) in [
        ("Cargo.toml", ProjectRecipeId::Rust),
        ("package.json", ProjectRecipeId::Node),
        ("pyproject.toml", ProjectRecipeId::Python),
        ("go.mod", ProjectRecipeId::Go),
    ] {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), marker, "");
        let resolved = resolve_project_recipe_root(temp.path(), None, None).unwrap();
        assert_eq!(resolved.recipe, expected);
        assert_eq!(resolved.relative_root, ".");
    }

    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "Cargo.toml", "");
    write(temp.path(), "nested/go.mod", "module example.test/nested\n");
    let resolved = resolve_project_recipe_root(temp.path(), Some("nested"), None).unwrap();
    assert_eq!(resolved.recipe, ProjectRecipeId::Go);
    assert_eq!(resolved.relative_root, "nested");
}

#[test]
fn ambiguity_is_sorted_but_explicit_mismatch_preserves_catalog_order() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "Cargo.toml", "");
    write(temp.path(), "package.json", "{}");
    write(temp.path(), "go.mod", "module example.test/root\n");

    let ambiguous = resolve_project_recipe_root(temp.path(), None, None).unwrap_err();
    assert_eq!(
        ambiguous,
        ProjectRecipeResolutionError::Ambiguous {
            recipe_root: ".".into(),
            candidates: vec![
                ProjectRecipeId::Go,
                ProjectRecipeId::Node,
                ProjectRecipeId::Rust,
            ],
        }
    );

    let python =
        resolve_project_recipe_root(temp.path(), None, Some(ProjectRecipeId::Python)).unwrap();
    assert_eq!(python.recipe, ProjectRecipeId::Python);
    assert_eq!(python.relative_root, ".");

    let node = resolve_project_recipe_root(temp.path(), None, Some(ProjectRecipeId::Node)).unwrap();
    assert_eq!(node.recipe, ProjectRecipeId::Node);

    fs::remove_file(temp.path().join("package.json")).unwrap();
    let mismatch =
        resolve_project_recipe_root(temp.path(), None, Some(ProjectRecipeId::Node)).unwrap_err();
    assert_eq!(
        mismatch,
        ProjectRecipeResolutionError::ExplicitMismatch {
            recipe_root: ".".into(),
            expected: ProjectRecipeId::Node,
            candidates: vec![ProjectRecipeId::Rust, ProjectRecipeId::Go],
        }
    );
}

#[test]
fn explicit_not_found_and_unhinted_not_found_remain_distinct() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        resolve_project_recipe_root(temp.path(), None, Some(ProjectRecipeId::Rust)).unwrap_err(),
        ProjectRecipeResolutionError::ExplicitNotFound {
            expected: ProjectRecipeId::Rust
        }
    );
    assert_eq!(
        resolve_project_recipe_root(temp.path(), None, None).unwrap_err(),
        ProjectRecipeResolutionError::NotFound
    );
}

#[test]
fn cwd_validation_preserves_existing_recipe_predicate() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "Cargo.toml", "");
    for cwd in ["", "../outside", "/tmp", "nul\0byte"] {
        assert_eq!(
            resolve_project_recipe_root(temp.path(), Some(cwd), None).unwrap_err(),
            ProjectRecipeResolutionError::CwdMismatch,
            "{cwd:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn cwd_symlink_escape_is_rejected() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "Cargo.toml", "");
    symlink(outside.path(), root.path().join("outside")).unwrap();
    assert_eq!(
        resolve_project_recipe_root(root.path(), Some("outside"), None).unwrap_err(),
        ProjectRecipeResolutionError::CwdMismatch
    );
}

#[test]
fn explicit_python_keeps_manifestless_cwd_even_with_other_markers() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "Cargo.toml", "");
    write(
        temp.path(),
        "nested/calculator.py",
        "def add(a, b): return a + b\n",
    );
    let resolved =
        resolve_project_recipe_root(temp.path(), Some("nested"), Some(ProjectRecipeId::Python))
            .unwrap();
    assert_eq!(resolved.recipe, ProjectRecipeId::Python);
    assert_eq!(resolved.relative_root, "nested");
    assert_eq!(resolved.absolute_root, temp.path().join("nested"));
}

#[test]
fn source_reads_anchor_relative_paths_to_execution_root() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(temp.path(), "Cargo.toml", "root-manifest\n");
    write(outside.path(), "Cargo.toml", "outside\n");
    let root = temp.path().canonicalize().unwrap();
    assert_eq!(
        read_project_recipe_file(&root, Path::new("Cargo.toml")).unwrap(),
        b"root-manifest\n"
    );
    assert_eq!(
        read_project_recipe_file(&root, &root.join("Cargo.toml")).unwrap(),
        b"root-manifest\n"
    );
    assert_eq!(
        read_project_recipe_file(&root, &outside.path().join("Cargo.toml")).unwrap_err(),
        ProjectRecipeResolutionError::SourceFileInvalid
    );
    fs::create_dir(root.join("not-a-file")).unwrap();
    assert_eq!(
        read_project_recipe_file(&root, Path::new("not-a-file")).unwrap_err(),
        ProjectRecipeResolutionError::SourceFileInvalid
    );
}

#[cfg(unix)]
#[test]
fn marker_symlink_is_detected_before_source_containment_rejects_it() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(
        outside.path(),
        "Cargo.toml",
        "[package]\nname='outside'\nversion='0.1.0'\n",
    );
    symlink(
        outside.path().join("Cargo.toml"),
        root.path().join("Cargo.toml"),
    )
    .unwrap();

    let resolved = resolve_project_recipe_root(root.path(), None, None).unwrap();
    assert_eq!(resolved.recipe, ProjectRecipeId::Rust);
    assert_eq!(
        read_project_recipe_file(&resolved.execution_root, &resolved.marker_path()).unwrap_err(),
        ProjectRecipeResolutionError::SourceFileInvalid
    );
}

#[test]
fn source_digest_preserves_path_length_content_order_and_skips_missing_optional_files() {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "Cargo.toml", "manifest\n");
    write(temp.path(), "Cargo.lock", "lock\n");
    let root = temp.path().canonicalize().unwrap();

    let actual = digest_project_recipe_files(
        &root,
        [
            Path::new("Cargo.toml").to_path_buf(),
            Path::new("missing.lock").to_path_buf(),
            Path::new("Cargo.lock").to_path_buf(),
        ],
    )
    .unwrap();

    let mut hasher = Sha256::new();
    for (path, content) in [
        ("Cargo.toml", b"manifest\n".as_slice()),
        ("Cargo.lock", b"lock\n".as_slice()),
    ] {
        hasher.update(Path::new(path).as_os_str().as_encoded_bytes());
        hasher.update((content.len() as u64).to_be_bytes());
        hasher.update(content);
    }
    assert_eq!(actual, format!("{:x}", hasher.finalize()));
}

#[test]
fn project_recipe_dependency_state_files_are_narrow_rust_go_source_truth() {
    assert_eq!(
        project_recipe_dependency_state_files(ProjectRecipeId::Rust),
        Some(&["Cargo.lock"][..])
    );
    assert_eq!(
        project_recipe_dependency_state_files(ProjectRecipeId::Go),
        Some(&["go.sum"][..])
    );
    assert!(project_recipe_dependency_state_files(ProjectRecipeId::Node).is_none());
    assert!(project_recipe_dependency_state_files(ProjectRecipeId::Python).is_none());
}
