use super::{resolve_validation_recipe, RecipeError, RecipeId, SemanticCheck};
use std::fs;
use std::path::Path;

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn resolve(
    root: &Path,
    cwd: Option<&str>,
    recipe: Option<RecipeId>,
    checks: &[SemanticCheck],
    test_filter: Option<&str>,
) -> Result<super::recipe::ResolvedValidationRecipe, RecipeError> {
    resolve_validation_recipe(root, cwd, recipe, checks, test_filter)
}

#[test]
fn recipe_resolution_is_nearest_deterministic_and_project_bounded() {
    struct Case {
        name: &'static str,
        files: &'static [(&'static str, &'static str)],
        cwd: Option<&'static str>,
        explicit: Option<RecipeId>,
        expected_id: &'static str,
        expected_root: &'static str,
    }
    let cases = [
        Case {
            name: "rust root",
            files: &[("Cargo.toml", "[package]\nname='fixture'\nversion='0.1.0'\n")],
            cwd: None,
            explicit: None,
            expected_id: "rust",
            expected_root: ".",
        },
        Case {
            name: "node root",
            files: &[
                (
                    "package.json",
                    r#"{"packageManager":"npm@10.0.0","scripts":{"check":"eslint ."}}"#,
                ),
                ("package-lock.json", "{}"),
            ],
            cwd: None,
            explicit: None,
            expected_id: "node",
            expected_root: ".",
        },
        Case {
            name: "python root",
            files: &[("pyproject.toml", "[tool.ruff]\nline-length=88\n")],
            cwd: None,
            explicit: None,
            expected_id: "python",
            expected_root: ".",
        },
        Case {
            name: "go root",
            files: &[("go.mod", "module example.test/fixture\n\ngo 1.22\n")],
            cwd: None,
            explicit: None,
            expected_id: "go",
            expected_root: ".",
        },
        Case {
            name: "nested node is nearer than rust",
            files: &[
                ("Cargo.toml", "[workspace]\nmembers=[]\n"),
                (
                    "frontend/package.json",
                    r#"{"packageManager":"pnpm@9.0.0","scripts":{"check":"eslint .","test":"vitest"}}"#,
                ),
                ("frontend/pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
                ("frontend/src/.keep", ""),
            ],
            cwd: Some("frontend/src"),
            explicit: None,
            expected_id: "node",
            expected_root: "frontend",
        },
    ];

    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        for (path, content) in case.files {
            write(temp.path(), path, content);
        }
        let plan = resolve(
            temp.path(),
            case.cwd,
            case.explicit,
            &[SemanticCheck::Check],
            None,
        )
        .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        assert_eq!(plan.recipe_id, case.expected_id, "{}", case.name);
        assert_eq!(
            plan.recipe_root_relative, case.expected_root,
            "{}",
            case.name
        );
    }
}

#[test]
fn recipe_resolution_fails_closed_for_ambiguity_mismatch_and_missing_marker() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "package.json",
        r#"{"packageManager":"npm@10.0.0","scripts":{"check":"eslint ."}}"#,
    );
    write(temp.path(), "package-lock.json", "{}");
    write(temp.path(), "go.mod", "module example.test/fixture\n");

    let ambiguous = resolve(temp.path(), None, None, &[SemanticCheck::Check], None).unwrap_err();
    assert_eq!(ambiguous.code, "validation_recipe_ambiguous");
    assert_eq!(
        ambiguous.details.as_ref().unwrap()["candidate_recipes"],
        serde_json::json!(["go", "node"])
    );

    let node = resolve(
        temp.path(),
        None,
        Some(RecipeId::Node),
        &[SemanticCheck::Check],
        None,
    )
    .unwrap();
    assert_eq!(node.recipe_id, "node");

    let mismatch = resolve(
        temp.path(),
        None,
        Some(RecipeId::Python),
        &[SemanticCheck::Check],
        None,
    )
    .unwrap_err();
    assert_eq!(mismatch.code, "validation_recipe_mismatch");

    let empty = tempfile::tempdir().unwrap();
    let missing = resolve(empty.path(), None, None, &[SemanticCheck::Check], None).unwrap_err();
    assert_eq!(missing.code, "validation_recipe_not_found");
}

#[test]
fn manifests_and_node_package_manager_evidence_are_validated_without_script_bodies() {
    struct Case {
        files: &'static [(&'static str, &'static str)],
        recipe: RecipeId,
        code: &'static str,
        secret_script_fragment: Option<&'static str>,
    }
    let cases = [
        Case {
            files: &[("package.json", "{not-json")],
            recipe: RecipeId::Node,
            code: "validation_manifest_invalid",
            secret_script_fragment: None,
        },
        Case {
            files: &[("pyproject.toml", "[tool.ruff\n")],
            recipe: RecipeId::Python,
            code: "validation_manifest_invalid",
            secret_script_fragment: None,
        },
        Case {
            files: &[
                (
                    "package.json",
                    r#"{"packageManager":"npm@10","scripts":{"check":"echo TOP_SECRET_BODY"}}"#,
                ),
                ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
            ],
            recipe: RecipeId::Node,
            code: "package_manager_ambiguous",
            secret_script_fragment: Some("TOP_SECRET_BODY"),
        },
        Case {
            files: &[
                (
                    "package.json",
                    r#"{"scripts":{"check; echo LEAK":"echo LEAK"}}"#,
                ),
                ("package-lock.json", "{}"),
                ("npm-shrinkwrap.json", "{}"),
            ],
            recipe: RecipeId::Node,
            code: "package_manager_ambiguous",
            secret_script_fragment: Some("echo LEAK"),
        },
    ];
    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        for (path, content) in case.files {
            write(temp.path(), path, content);
        }
        let error = resolve(
            temp.path(),
            None,
            Some(case.recipe),
            &[SemanticCheck::Check],
            None,
        )
        .unwrap_err();
        assert_eq!(error.code, case.code);
        if let Some(secret) = case.secret_script_fragment {
            assert!(!format!("{error:?}").contains(secret));
        }
    }
}

#[test]
fn recipes_emit_only_canonical_argv_and_never_select_mutating_node_format() {
    struct Case {
        recipe: RecipeId,
        manifest: &'static str,
        extra: Option<(&'static str, &'static str)>,
        checks: &'static [SemanticCheck],
        expected: &'static [(&'static str, &'static str, &'static [&'static str])],
    }
    let cases = [
        Case {
            recipe: RecipeId::Rust,
            manifest: "[package]\nname='fixture'\nversion='0.1.0'\n",
            extra: None,
            checks: &[
                SemanticCheck::Format,
                SemanticCheck::Check,
                SemanticCheck::Test,
            ],
            expected: &[
                ("format", "cargo", &["fmt", "--", "--check"]),
                ("check", "cargo", &["check", "--all-targets"]),
                ("test", "cargo", &["test"]),
            ],
        },
        Case {
            recipe: RecipeId::Python,
            manifest: "[tool.ruff]\nline-length=88\n[tool.pytest.ini_options]\n",
            extra: None,
            checks: &[
                SemanticCheck::Format,
                SemanticCheck::Check,
                SemanticCheck::Test,
            ],
            expected: &[
                ("format", "python", &["-m", "ruff", "format", "--check"]),
                ("check", "python", &["-m", "ruff", "check"]),
                ("test", "python", &["-m", "pytest"]),
            ],
        },
        Case {
            recipe: RecipeId::Go,
            manifest: "module example.test/fixture\n",
            extra: None,
            checks: &[SemanticCheck::Check, SemanticCheck::Test],
            expected: &[
                ("check", "go", &["vet", "./..."]),
                ("test", "go", &["test", "./..."]),
            ],
        },
        Case {
            recipe: RecipeId::Node,
            manifest: r#"{"packageManager":"npm@10","scripts":{"format":"prettier --write .","format-check":"prettier --check .","check":"eslint .","test":"vitest"}}"#,
            extra: Some(("package-lock.json", "{}")),
            checks: &[
                SemanticCheck::Format,
                SemanticCheck::Check,
                SemanticCheck::Test,
            ],
            expected: &[
                ("format", "npm", &["run", "--silent", "format-check"]),
                ("check", "npm", &["run", "--silent", "check"]),
                ("test", "npm", &["run", "--silent", "test"]),
            ],
        },
    ];

    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let marker = match case.recipe {
            RecipeId::Rust => "Cargo.toml",
            RecipeId::Node => "package.json",
            RecipeId::Python => "pyproject.toml",
            RecipeId::Go => "go.mod",
        };
        write(temp.path(), marker, case.manifest);
        if let Some((path, content)) = case.extra {
            write(temp.path(), path, content);
        }
        let plan = resolve(temp.path(), None, Some(case.recipe), case.checks, None).unwrap();
        if case.recipe == RecipeId::Node {
            assert!(!serde_json::to_string(&plan.steps)
                .unwrap()
                .contains("prettier"));
        }
        let actual = plan
            .steps
            .iter()
            .map(|step| {
                (
                    step.name.as_str(),
                    step.program.as_str(),
                    step.args.iter().map(String::as_str).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let expected = case
            .expected
            .iter()
            .map(|(name, program, args)| (*name, *program, args.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{}", plan.recipe_id);
    }
}

#[test]
fn node_package_manager_resolution_covers_declared_and_lockfile_evidence() {
    for (manager, lockfile) in [
        ("npm", "package-lock.json"),
        ("npm", "npm-shrinkwrap.json"),
        ("pnpm", "pnpm-lock.yaml"),
        ("yarn", "yarn.lock"),
        ("bun", "bun.lock"),
        ("bun", "bun.lockb"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        write(
            temp.path(),
            "package.json",
            &format!(r#"{{"scripts":{{"check":"private body for {manager}"}}}}"#),
        );
        write(temp.path(), lockfile, "");
        let plan = resolve(
            temp.path(),
            None,
            Some(RecipeId::Node),
            &[SemanticCheck::Check],
            None,
        )
        .unwrap();
        assert_eq!(plan.steps[0].program, manager);
        assert_eq!(plan.steps[0].args, ["run", "--silent", "check"]);
        assert!(!serde_json::to_string(&plan.steps)
            .unwrap()
            .contains("private body"));
    }
}

#[test]
fn unavailable_checks_and_unsupported_filters_fail_before_execution() {
    let go = tempfile::tempdir().unwrap();
    write(go.path(), "go.mod", "module example.test/fixture\n");
    let unavailable = resolve(
        go.path(),
        None,
        Some(RecipeId::Go),
        &[SemanticCheck::Format],
        None,
    )
    .unwrap_err();
    assert_eq!(unavailable.code, "validation_check_unavailable");

    let node = tempfile::tempdir().unwrap();
    write(
        node.path(),
        "package.json",
        r#"{"packageManager":"npm@10","scripts":{"test":"vitest"}}"#,
    );
    write(node.path(), "package-lock.json", "{}");
    let unsupported = resolve(
        node.path(),
        None,
        Some(RecipeId::Node),
        &[SemanticCheck::Test],
        Some("safe-looking-filter"),
    )
    .unwrap_err();
    assert_eq!(unsupported.code, "test_filter_unsupported");

    write(
        node.path(),
        "package.json",
        r#"{"packageManager":"npm@10","scripts":{"test; touch escaped":"ignored"}}"#,
    );
    let unavailable = resolve(
        node.path(),
        None,
        Some(RecipeId::Node),
        &[SemanticCheck::Test],
        None,
    )
    .unwrap_err();
    assert_eq!(unavailable.code, "validation_check_unavailable");
    assert!(!format!("{unavailable:?}").contains("touch escaped"));
}

#[test]
fn rust_filter_contract_normalizes_rejects_and_binds_identity() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n",
    );
    let plan = |filter: Option<&str>| {
        resolve(
            temp.path(),
            None,
            Some(RecipeId::Rust),
            &[SemanticCheck::Test],
            filter,
        )
    };
    // Representative option-like and control-char filters are rejected before
    // planning (the exhaustive list is covered by the protocol is_canonical
    // test, which shares the same validator).
    for filter in [
        "--manifest-path=/tmp/outside/Cargo.toml",
        " --",
        "line\nbreak",
        "nul\0byte",
    ] {
        assert_eq!(
            plan(Some(filter)).unwrap_err().code,
            "test_filter_unsupported",
            "{filter:?}"
        );
    }
    // Empty / whitespace-only means "no filter".
    for filter in ["", "   ", "\n"] {
        let resolved = plan(Some(filter)).unwrap();
        assert_eq!(resolved.steps[0].args, vec!["test"], "{filter:?}");
        assert!(resolved.test_filter.is_none(), "{filter:?}");
    }
    // Valid substrings (Unicode, Rust path form, shell metachars in one argv)
    // are accepted verbatim as a single argv value.
    for filter in ["module::nested::test", "测试::筛选", "name; $(sub)"] {
        let resolved = plan(Some(filter)).unwrap();
        assert_eq!(resolved.steps[0].args, vec!["test", filter]);
        assert_eq!(resolved.test_filter.as_deref(), Some(filter));
    }
    // Durable identity binds the normalized value: a padded variant matches its
    // trimmed form, and a different filter changes the invocation digest.
    let trimmed = plan(Some("module::inner")).unwrap();
    let padded = plan(Some("  module::inner  ")).unwrap();
    assert_eq!(padded.steps, trimmed.steps);
    assert_eq!(padded.invocation_digest, trimmed.invocation_digest);
    assert_ne!(
        plan(Some("module::other")).unwrap().invocation_digest,
        trimmed.invocation_digest
    );

    // A --manifest-path filter pointing at a real outside project is rejected,
    // and planning never compiles it (planning only builds argv).
    let outside = tempfile::tempdir().unwrap();
    write(
        outside.path(),
        "Cargo.toml",
        "[package]\nname='outside'\nversion='0.1.0'\n",
    );
    let manifest_filter = format!(
        "--manifest-path={}",
        outside.path().join("Cargo.toml").display()
    );
    assert_eq!(
        plan(Some(&manifest_filter)).unwrap_err().code,
        "test_filter_unsupported"
    );
    assert!(!outside.path().join("target").exists());
}

#[cfg(unix)]
#[test]
fn cwd_symlink_escape_is_rejected() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "Cargo.toml", "[workspace]\n");
    symlink(outside.path(), root.path().join("outside")).unwrap();
    let error = resolve(
        root.path(),
        Some("outside"),
        Some(RecipeId::Rust),
        &[SemanticCheck::Check],
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "validation_recipe_mismatch");
    assert!(!format!("{error:?}").contains(&outside.path().display().to_string()));

    for cwd in ["../outside", "/outside"] {
        let error = resolve(
            root.path(),
            Some(cwd),
            Some(RecipeId::Rust),
            &[SemanticCheck::Check],
            None,
        )
        .unwrap_err();
        assert_eq!(error.code, "validation_recipe_mismatch");
    }
}
