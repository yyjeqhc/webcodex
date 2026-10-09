use super::{
    resolve_project_validation_recipe, resolve_validation_recipe,
    resolve_validation_recipe_with_packages, RecipeError, RecipeId, SemanticCheck,
};
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
) -> Result<super::ResolvedValidationRecipe, RecipeError> {
    resolve_validation_recipe(root, cwd, recipe, checks, test_filter)
}

#[test]
fn native_node_rejects_nonregular_nested_marker_before_ancestor_fallback() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "package.json",
        r#"{"scripts":{"check":"echo ancestor"}}"#,
    );
    write(temp.path(), "frontend/src/.keep", "");
    fs::create_dir(temp.path().join("frontend/package.json")).unwrap();
    let error =
        super::resolve_node_native_project_check(temp.path(), Some("frontend/src")).unwrap_err();
    assert_eq!(error.code, "validation_manifest_invalid");
    fs::remove_dir(temp.path().join("frontend/package.json")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            temp.path().join("missing-package.json"),
            temp.path().join("frontend/package.json"),
        )
        .unwrap();
        assert_eq!(
            super::resolve_node_native_project_check(temp.path(), Some("frontend/src"))
                .unwrap_err()
                .code,
            "validation_manifest_invalid"
        );
    }
}

#[test]
fn native_node_project_script_selection_is_closed_and_digest_fenced() {
    let temp = tempfile::tempdir().unwrap();
    // Native Node deliberately ignores package-manager selection/lockfiles.
    write(temp.path(), "yarn.lock", "lockfileVersion: 9\n");
    write(
        temp.path(),
        "package.json",
        r#"{"packageManager":"pnpm@9.0.0","scripts":{"lint":"eslint .","typecheck":"tsc --noEmit","check":"node check.js","precheck":"node pre.js"}}"#,
    );
    let baseline = super::resolve_node_native_project_check(temp.path(), None).unwrap();
    assert_eq!(baseline.recipe_root_relative, ".");
    assert_eq!(baseline.steps.len(), 1);
    assert!(baseline.steps[0].is_structured_node_check());
    assert_eq!(baseline.steps[0].args, ["--run", "check"]);
    let serialized = serde_json::to_string(&baseline.steps).unwrap();
    assert!(!serialized.contains("node check.js"));
    assert!(!serialized.contains("node pre.js"));

    write(
        temp.path(),
        "package.json",
        r#"{"packageManager":"pnpm@9.0.0","scripts":{"lint":"eslint .","typecheck":"tsc --noEmit","check":"node changed.js"}}"#,
    );
    let changed_body = super::resolve_node_native_project_check(temp.path(), None).unwrap();
    assert_eq!(baseline.steps, changed_body.steps);
    assert_eq!(baseline.invocation_digest, changed_body.invocation_digest);
    assert_ne!(baseline.manifest_digest, changed_body.manifest_digest);

    write(
        temp.path(),
        "package.json",
        r#"{"scripts":{"lint":"eslint .","typecheck":"tsc --noEmit"}}"#,
    );
    let typecheck = super::resolve_node_native_project_check(temp.path(), None).unwrap();
    assert_eq!(typecheck.steps[0].args, ["--run", "typecheck"]);
    assert_ne!(baseline.invocation_digest, typecheck.invocation_digest);
    write(
        temp.path(),
        "package.json",
        r#"{"scripts":{"lint":"eslint ."}}"#,
    );
    let lint = super::resolve_node_native_project_check(temp.path(), None).unwrap();
    assert_eq!(lint.steps[0].args, ["--run", "lint"]);

    write(
        temp.path(),
        "frontend/package.json",
        r#"{"scripts":{"check":"echo frontend"}}"#,
    );
    write(temp.path(), "frontend/src/.keep", "");
    let nested =
        super::resolve_node_native_project_check(temp.path(), Some("frontend/src")).unwrap();
    assert_eq!(nested.recipe_root_relative, "frontend");
    assert_eq!(nested.steps[0].args, ["--run", "check"]);
}

#[test]
fn native_node_project_check_rejects_malformed_and_escaped_manifests() {
    let temp = tempfile::tempdir().unwrap();
    for (manifest, expected) in [
        (r#"{"scripts":{"lint":"yes"}}"#, None),
        (
            r#"{"scripts":{"check":null,"lint":"yes"}}"#,
            Some("validation_manifest_invalid"),
        ),
        (
            r#"{"scripts":{"check":42,"lint":"yes"}}"#,
            Some("validation_manifest_invalid"),
        ),
        (
            r#"{"scripts":{"check":"","lint":"yes"}}"#,
            Some("validation_manifest_invalid"),
        ),
        (
            r#"{"scripts":{"check":"   ","lint":"yes"}}"#,
            Some("validation_manifest_invalid"),
        ),
        (r#"{"scripts":{}}"#, Some("validation_check_unavailable")),
        (
            r#"{"scripts":{"check":"ok"}"#,
            Some("validation_manifest_invalid"),
        ),
    ] {
        write(temp.path(), "package.json", manifest);
        let outcome = super::resolve_node_native_project_check(temp.path(), None);
        match expected {
            None => assert_eq!(outcome.unwrap().steps[0].args, ["--run", "lint"]),
            Some(expected_code) => assert_eq!(outcome.unwrap_err().code, expected_code),
        }
    }
    let oversized = format!(r#"{{"scripts":{{"check":"{}"}}}}"#, "x".repeat(1024 * 1024));
    write(temp.path(), "package.json", &oversized);
    assert_eq!(
        super::resolve_node_native_project_check(temp.path(), None)
            .unwrap_err()
            .code,
        "validation_manifest_invalid"
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        write(
            outside.path(),
            "package.json",
            r#"{"scripts":{"check":"echo outside"}}"#,
        );
        fs::remove_file(temp.path().join("package.json")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("package.json"),
            temp.path().join("package.json"),
        )
        .unwrap();
        assert!(super::resolve_node_native_project_check(temp.path(), None).is_err());
    }
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
        Some(RecipeId::Rust),
        &[SemanticCheck::Check],
        None,
    )
    .unwrap_err();
    assert_eq!(mismatch.code, "validation_recipe_mismatch");
    assert_eq!(
        mismatch.details.as_ref().unwrap(),
        &serde_json::json!({
            "recipe_root": ".",
            "candidate_recipes": ["node", "go"],
            "detected_markers": ["package.json", "go.mod"],
        })
    );

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
                ("test", "go", &["test", "-json", "./..."]),
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
fn scoped_rust_and_go_recipes_bind_canonical_package_argv_into_invocation_identity() {
    use sha2::{Digest, Sha256};

    let rust = tempfile::tempdir().unwrap();
    write(
        rust.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['package-a','package-b']\n",
    );
    let rust_plan = resolve_validation_recipe_with_packages(
        rust.path(),
        None,
        Some(RecipeId::Rust),
        &[SemanticCheck::Check, SemanticCheck::Test],
        None,
        Some(&["package-b".into(), "package-a".into()]),
    )
    .unwrap();
    assert_eq!(
        rust_plan.steps[0].args,
        [
            "check",
            "--all-targets",
            "-p",
            "package-a",
            "-p",
            "package-b"
        ]
    );
    assert_eq!(
        rust_plan.steps[1].args,
        ["test", "-p", "package-a", "-p", "package-b"]
    );
    assert_eq!(
        rust_plan.invocation_digest,
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&rust_plan.steps).unwrap())
        )
    );

    let go = tempfile::tempdir().unwrap();
    write(go.path(), "go.mod", "module example.test/fixture\n");
    let go_plan = resolve_validation_recipe_with_packages(
        go.path(),
        None,
        Some(RecipeId::Go),
        &[SemanticCheck::Check, SemanticCheck::Test],
        None,
        Some(&["./cmd/...".into(), "./internal".into()]),
    )
    .unwrap();
    assert_eq!(go_plan.steps[0].args, ["vet", "./cmd/...", "./internal"]);
    assert_eq!(
        go_plan.steps[1].args,
        ["test", "-json", "./cmd/...", "./internal"]
    );
    assert_eq!(
        go_plan.invocation_digest,
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&go_plan.steps).unwrap())
        )
    );
}

#[test]
fn go_test_json_argv_is_bound_into_durable_invocation_identity() {
    use sha2::{Digest, Sha256};

    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "go.mod",
        "module example.test/fixture\n\ngo 1.22\n",
    );
    let plan = resolve(
        temp.path(),
        None,
        Some(RecipeId::Go),
        &[SemanticCheck::Check, SemanticCheck::Test],
        None,
    )
    .unwrap();
    assert_eq!(plan.steps[0].args, ["vet", "./..."]);
    assert_eq!(plan.steps[1].args, ["test", "-json", "./..."]);
    assert_eq!(
        plan.durable_identity()["tool_identities"],
        serde_json::json!(["go_check", "go_test"])
    );
    assert_eq!(
        plan.invocation_digest,
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&plan.steps).unwrap())
        )
    );

    let mut legacy_steps = plan.steps.clone();
    legacy_steps[1].args = ["test", "./..."].into_iter().map(str::to_string).collect();
    let legacy_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&legacy_steps).unwrap())
    );
    assert_ne!(plan.invocation_digest, legacy_digest);
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
fn package_scope_is_rejected_for_non_portable_backends() {
    let node = tempfile::tempdir().unwrap();
    write(
        node.path(),
        "package.json",
        r#"{"packageManager":"npm@10","scripts":{"test":"vitest"}}"#,
    );
    write(node.path(), "package-lock.json", "{}");
    let error = resolve_validation_recipe_with_packages(
        node.path(),
        None,
        Some(RecipeId::Node),
        &[SemanticCheck::Test],
        None,
        Some(&["package-a".to_string()]),
    )
    .unwrap_err();
    assert_eq!(error.code, "validation_scope_unsupported");

    let python = tempfile::tempdir().unwrap();
    write(
        python.path(),
        "pyproject.toml",
        "[tool.pytest.ini_options]\naddopts = '-q'\n",
    );
    let error = resolve_validation_recipe_with_packages(
        python.path(),
        None,
        Some(RecipeId::Python),
        &[SemanticCheck::Test],
        None,
        Some(&["package-a".to_string()]),
    )
    .unwrap_err();
    assert_eq!(error.code, "validation_scope_unsupported");
}

#[test]
fn project_pytest_fences_ancestor_config_and_external_parent_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("nested")).unwrap();

    write(temp.path(), "pytest.ini", "[pytest]\naddopts=-q\n");
    let external = resolve_project_validation_recipe(
        &project,
        Some("nested"),
        Some(RecipeId::Python),
        &[SemanticCheck::Test],
        None,
        None,
        false,
        None,
    )
    .unwrap_err();
    assert_eq!(external.code, "validation_manifest_invalid");

    write(&project, "pytest.ini", "[pytest]\naddopts=-q\n");
    let before = resolve_project_validation_recipe(
        &project,
        Some("nested"),
        Some(RecipeId::Python),
        &[SemanticCheck::Test],
        None,
        None,
        false,
        None,
    )
    .unwrap();
    write(&project, "pytest.ini", "[pytest]\naddopts=-ra\n");
    let after = resolve_project_validation_recipe(
        &project,
        Some("nested"),
        Some(RecipeId::Python),
        &[SemanticCheck::Test],
        None,
        None,
        false,
        None,
    )
    .unwrap();
    assert_ne!(before.manifest_digest, after.manifest_digest);
}

#[test]
fn project_recipe_filtered_execution_is_deterministic_without_repeating_arg_builders() {
    for (marker, backend, filter) in [
        ("Cargo.toml", RecipeId::Rust, " selected "),
        ("go.mod", RecipeId::Go, "^TestA/Sub$"),
    ] {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), marker, "");
        let plain = resolve(
            root.path(),
            None,
            Some(backend),
            &[SemanticCheck::Test],
            None,
        )
        .unwrap();
        let selected = resolve(
            root.path(),
            None,
            Some(backend),
            &[SemanticCheck::Test],
            Some(filter),
        )
        .unwrap();
        let expected =
            crate::project_validation_operation(backend.as_str(), SemanticCheck::Test, None, false)
                .unwrap()
                .with_test_filter(Some(filter))
                .unwrap()
                .build_readonly_plan()
                .unwrap();
        assert_eq!(selected.steps, vec![expected.structured_step]);
        assert_eq!(selected.manifest_digest, plain.manifest_digest);
        assert_ne!(selected.invocation_digest, plain.invocation_digest);
        assert_eq!(
            resolve(
                root.path(),
                None,
                Some(backend),
                &[SemanticCheck::Test],
                Some("")
            )
            .unwrap()
            .invocation_digest,
            plain.invocation_digest
        );
    }
}

#[test]
fn rust_member_manifest_digest_tracks_workspace_root_inputs() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['member']\nresolver='2'\n",
    );
    write(temp.path(), "Cargo.lock", "version = 4\n");
    write(
        temp.path(),
        "member/Cargo.toml",
        "[package]\nname='member'\nversion='0.1.0'\nedition='2021'\n",
    );

    let resolve_member = || {
        resolve(
            temp.path(),
            Some("member"),
            Some(RecipeId::Rust),
            &[SemanticCheck::Check],
            None,
        )
        .unwrap()
    };
    let original = resolve_member();
    assert_eq!(original.recipe_root_relative, "member");
    assert_eq!(original.steps[0].args, ["check", "--all-targets"]);

    write(
        temp.path(),
        "member/src/lib.rs",
        "pub fn value() -> u8 { 1 }\n",
    );
    let source_only = resolve_member();
    assert_eq!(source_only.manifest_digest, original.manifest_digest);
    assert_eq!(source_only.invocation_digest, original.invocation_digest);

    write(temp.path(), "Cargo.lock", "version = 3\n");
    let lock_changed = resolve_member();
    assert_ne!(lock_changed.manifest_digest, original.manifest_digest);
    assert_eq!(lock_changed.invocation_digest, original.invocation_digest);

    write(temp.path(), "Cargo.lock", "version = 4\n");
    write(
        temp.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['member']\nresolver='2'\n[profile.dev]\nopt-level=1\n",
    );
    let workspace_manifest_changed = resolve_member();
    assert_ne!(
        workspace_manifest_changed.manifest_digest,
        original.manifest_digest
    );
    assert_eq!(
        workspace_manifest_changed.invocation_digest,
        original.invocation_digest
    );
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
fn marker_symlink_escape_remains_a_manifest_error_after_recipe_detection() {
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
    assert_eq!(
        resolve(root.path(), None, None, &[SemanticCheck::Check], None)
            .unwrap_err()
            .code,
        "validation_manifest_invalid"
    );
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

#[test]
fn manifestless_explicit_python_uses_stable_unittest_plan() {
    use sha2::{Digest, Sha256};
    use webcodex_core::runner_protocol::ShellJobValidationStep;

    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "calculator.py",
        "def add(a, b): return a + b\n",
    );
    let plan = resolve(
        temp.path(),
        None,
        Some(RecipeId::Python),
        &[SemanticCheck::Test],
        None,
    )
    .unwrap();
    assert_eq!(
        (plan.recipe_id, plan.recipe_root_relative.as_str()),
        ("python", ".")
    );
    assert_eq!(
        plan.steps[0].args,
        ["-B", "-m", "unittest", "discover", "-v"]
    );
    assert!(plan.steps[0].is_canonical());
    let seed = format!(
        "{:x}",
        Sha256::digest(b"webcodex.python.manifestless.recipe.v1")
    );
    assert_eq!(plan.manifest_digest, seed);
    assert_eq!(
        plan.durable_identity()["tool_identities"][0],
        "python:unittest:test"
    );
    assert_eq!(
        resolve(
            temp.path(),
            None,
            Some(RecipeId::Python),
            &[SemanticCheck::Test],
            None
        )
        .unwrap()
        .invocation_digest,
        plan.invocation_digest
    );
    assert!(!temp.path().join("pyproject.toml").exists());
    for check in [SemanticCheck::Format, SemanticCheck::Check] {
        assert_eq!(
            resolve(temp.path(), None, Some(RecipeId::Python), &[check], None)
                .unwrap_err()
                .code,
            "validation_check_unavailable"
        );
    }
    assert_eq!(
        resolve(
            temp.path(),
            None,
            Some(RecipeId::Python),
            &[SemanticCheck::Test],
            Some("T")
        )
        .unwrap_err()
        .code,
        "test_filter_unsupported"
    );
    assert_eq!(
        resolve(temp.path(), None, None, &[SemanticCheck::Test], None)
            .unwrap_err()
            .code,
        "validation_recipe_not_found"
    );
    assert!(!ShellJobValidationStep {
        name: "test".into(),
        program: "python".into(),
        args: ["-B", "-m", "unittest", "discover", "-v", "extra"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        env: Vec::new(),
    }
    .is_canonical());
    write(
        temp.path(),
        "Cargo.toml",
        "[package]\nname = \"polyglot\"\nversion = \"0.1.0\"\n",
    );
    let polyglot = resolve(
        temp.path(),
        None,
        Some(RecipeId::Python),
        &[SemanticCheck::Test],
        None,
    )
    .unwrap();
    assert_eq!(
        polyglot.steps[0].args,
        ["-B", "-m", "unittest", "discover", "-v"]
    );
    fs::create_dir(temp.path().join("pyproject.toml")).unwrap();
    assert_eq!(
        resolve(
            temp.path(),
            None,
            Some(RecipeId::Python),
            &[SemanticCheck::Test],
            None
        )
        .unwrap_err()
        .code,
        "validation_manifest_invalid"
    );
    fs::remove_dir(temp.path().join("pyproject.toml")).unwrap();
    write(
        temp.path(),
        "pyproject.toml",
        "[tool.pytest.ini_options]\nminversion = \"6.0\"\n",
    );
    let pytest = resolve(
        temp.path(),
        None,
        Some(RecipeId::Python),
        &[SemanticCheck::Test],
        None,
    )
    .unwrap();
    assert_eq!(pytest.steps[0].args, ["-m", "pytest"]);
    assert_ne!(pytest.manifest_digest, seed);
}

#[test]
fn project_ruff_requires_local_explicit_config_and_fences_exact_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let plan = |action| {
        resolve_project_validation_recipe(
            temp.path(),
            None,
            Some(RecipeId::Python),
            &[action],
            None,
            None,
            false,
            None,
        )
    };
    for action in [SemanticCheck::Check, SemanticCheck::Format] {
        assert!(plan(action).is_err(), "manifestless Ruff must fail closed");
        for manifest in [
            "",
            "[tool.black]\n",
            "[tool.ruff]\n",
            "[tool.ruff]\ntarget-version=311\n",
            "[tool.ruff]\ntarget-version='py311'\nextend='../ruff.toml'\n",
            "[tool.ruff]\ntarget-version='py311'\nextend=''\n",
            "[tool.ruff]\ntarget-version='ambient'\n",
        ] {
            write(temp.path(), "pyproject.toml", manifest);
            assert!(plan(action).is_err(), "{manifest}");
        }
        write(
            temp.path(),
            "pyproject.toml",
            "[tool.ruff]\ntarget-version='py311'\nfix=true\nfix-only=true\n",
        );
        let first = plan(action).unwrap();
        assert!(first.steps[0].is_structured_ruff());
        assert_eq!(
            first.steps[0],
            webcodex_core::runner_protocol::ShellJobValidationStep::python_ruff(action.as_str())
                .unwrap()
        );
        write(
            temp.path(),
            "pyproject.toml",
            "[tool.ruff]\ntarget-version='py312'\n",
        );
        let changed = plan(action).unwrap();
        assert_ne!(first.manifest_digest, changed.manifest_digest);
        assert_eq!(first.invocation_digest, changed.invocation_digest);
        for (packages, all, filter, policy) in [
            (Some(vec!["src".into()]), false, None, None),
            (None, true, None, None),
            (None, false, Some("name"), None),
            (
                None,
                false,
                None,
                Some(webcodex_core::project_validation::ProjectDependencyPolicy {
                    mode: webcodex_core::project_validation::ProjectDependencyMode::Locked,
                }),
            ),
        ] {
            assert!(resolve_project_validation_recipe(
                temp.path(),
                None,
                Some(RecipeId::Python),
                &[action],
                filter,
                packages.as_deref(),
                all,
                policy
            )
            .is_err());
        }
        fs::remove_file(temp.path().join("pyproject.toml")).unwrap();
    }
    write(
        temp.path(),
        "pyproject.toml",
        &format!(
            "[tool.ruff]\ntarget-version='py311'\n#{}",
            "x".repeat(1024 * 1024)
        ),
    );
    assert_eq!(
        plan(SemanticCheck::Check).unwrap_err().code,
        "validation_manifest_invalid"
    );
}

#[cfg(unix)]
#[test]
fn project_ruff_rejects_manifest_symlink_escape() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(
        outside.path(),
        "pyproject.toml",
        "[tool.ruff]\ntarget-version='py311'\n",
    );
    std::os::unix::fs::symlink(
        outside.path().join("pyproject.toml"),
        temp.path().join("pyproject.toml"),
    )
    .unwrap();
    assert!(resolve_project_validation_recipe(
        temp.path(),
        None,
        Some(RecipeId::Python),
        &[SemanticCheck::Check],
        None,
        None,
        false,
        None
    )
    .is_err());
}
