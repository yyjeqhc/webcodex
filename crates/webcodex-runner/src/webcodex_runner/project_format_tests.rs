use super::config::{RunnerPolicy, ShellConfig};
use super::project_format::{
    commit_candidates, format_candidates, format_candidates_with_deadline, handle, plan, replan,
    PlannedFormat,
};
use super::shell::PreparedShellProfileCache;
use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use webcodex_core::project_format::*;

const MANIFEST: &str = "[package]\nname='demo'\nversion='0.1.0'\nedition='2021'\n";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    registry: PathBuf,
    policy: RunnerPolicy,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let registry = temp.path().join("registry");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir(&registry).unwrap();
        fs::write(root.join("Cargo.toml"), MANIFEST).unwrap();
        fs::write(root.join("src/main.rs"), "fn main( ){ }\n").unwrap();
        let fixture = Self {
            _temp: temp,
            root: root.clone(),
            registry,
            policy: RunnerPolicy {
                allowed_roots: vec![root],
                ..Default::default()
            },
        };
        fixture.register(true);
        fixture
    }

    fn register(&self, allow_patch: bool) {
        fs::write(
            self.registry.join("demo.toml"),
            format!(
                "id='demo'\npath={}\nallow_patch={allow_patch}\n",
                serde_json::to_string(self.root.to_str().unwrap()).unwrap(),
            ),
        )
        .unwrap();
    }

    fn request(&self) -> ProjectFormatRequest {
        ProjectFormatRequest {
            project_id: "demo".into(),
            cwd: None,
            adapter: ProjectFormatAdapter::Auto,
            files: vec!["src/main.rs".into()],
        }
    }

    fn plan(&self) -> PlannedFormat {
        plan(&self.policy, &self.registry, &self.request()).unwrap()
    }
}

fn rejection(result: Result<PlannedFormat, ProjectFormatPlanningResult>) -> String {
    match result {
        Err(ProjectFormatPlanningResult::Unavailable { code }) => code,
        _ => panic!("expected unavailable format plan"),
    }
}

#[test]
fn project_format_plan_keeps_source_private_and_replans_exact_bytes() {
    let fixture = Fixture::new();
    let original = fixture.plan();
    assert_eq!(original.sources, ["fn main( ){ }\n"]);
    assert_eq!(original.root, fixture.root.canonicalize().unwrap());
    assert_eq!(original.cwd, original.root);
    assert!(original.plan.is_valid());
    assert!(!serde_json::to_string(&original.plan)
        .unwrap()
        .contains("fn main"));
    assert!(replan(&fixture.policy, &fixture.registry, &original.plan).is_ok());
    fs::write(fixture.root.join("src/main.rs"), "fn main() {}\n").unwrap();
    assert!(matches!(
        replan(&fixture.policy, &fixture.registry, &original.plan),
        Err("format_plan_stale")
    ));
    let updated = fixture.plan();
    fs::write(
        fixture.root.join("Cargo.toml"),
        MANIFEST.replace("2021", "2024"),
    )
    .unwrap();
    assert!(replan(&fixture.policy, &fixture.registry, &updated.plan).is_err());
}

#[test]
fn project_format_replan_rejects_same_path_replacement_with_identical_bytes() {
    let fixture = Fixture::new();
    let original = fixture.plan();
    let retired = fixture.root.with_extension("retired");
    fs::rename(&fixture.root, &retired).unwrap();
    fs::create_dir_all(fixture.root.join("src")).unwrap();
    fs::write(fixture.root.join("Cargo.toml"), MANIFEST).unwrap();
    fs::write(fixture.root.join("src/main.rs"), &original.sources[0]).unwrap();
    let replacement = fixture.plan();
    assert_ne!(replacement.plan.root_digest, original.plan.root_digest);
    assert!(replan(&fixture.policy, &fixture.registry, &original.plan).is_err());
}

#[test]
fn project_format_requires_exact_enabled_writable_project_and_process_policy() {
    let mut fixture = Fixture::new();
    let mut unknown = fixture.request();
    unknown.project_id = "other".into();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &unknown)),
        "unknown_project"
    );
    fixture.register(false);
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
        "permission_denied"
    );
    fixture.register(true);
    fixture.policy.allow_raw_shell = false;
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
        "permission_denied"
    );
}

#[test]
fn project_format_rejects_cross_recipe_and_ambiguous_roots_without_adapter_fallback() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root.join("nested")).unwrap();
    fs::write(fixture.root.join("nested/Cargo.toml"), MANIFEST).unwrap();
    fs::write(fixture.root.join("nested/main.rs"), "fn main() {}\n").unwrap();
    let mut request = fixture.request();
    request.files.push("nested/main.rs".into());
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_scope_mismatch"
    );
    request.files.remove(0);
    request.cwd = Some("nested".into());
    assert_eq!(
        plan(&fixture.policy, &fixture.registry, &request)
            .unwrap()
            .plan
            .recipe_root,
        "nested"
    );
    fs::write(
        fixture.root.join("nested/pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    request.adapter = ProjectFormatAdapter::Rust;
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_recipe_ambiguous"
    );
}

#[test]
fn project_format_invalid_nested_markers_never_fall_back_to_parent() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.root.join("src/Cargo.toml")).unwrap();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
        "format_manifest_invalid"
    );
}

#[test]
fn project_format_rejects_file_local_rustfmt_config_and_stale_plan() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    for config in ["src/rustfmt.toml", "src/.rustfmt.toml"] {
        let path = fixture.root.join(config);
        fs::write(&path, "hard_tabs = true\n").unwrap();
        assert_eq!(
            rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
            "format_config_unsupported",
            "a selected source must not silently ignore {config}"
        );
        assert_eq!(
            replan(&fixture.policy, &fixture.registry, &planned.plan).err(),
            Some("format_plan_stale"),
            "new source-local format configuration invalidates the existing plan"
        );
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn project_format_rejects_custom_config_inherited_edition_and_large_inputs() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("rustfmt.toml"), "max_width=80\n").unwrap();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
        "format_config_unsupported"
    );
    fs::remove_file(fixture.root.join("rustfmt.toml")).unwrap();
    fs::write(
        fixture.root.join("Cargo.toml"),
        "[package]\nname='demo'\nedition.workspace=true\n",
    )
    .unwrap();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
        "format_manifest_unsupported"
    );
    fs::write(fixture.root.join("Cargo.toml"), MANIFEST).unwrap();
    fs::write(
        fixture.root.join("src/main.rs"),
        vec![b' '; PROJECT_FORMAT_FILE_MAX_BYTES + 1],
    )
    .unwrap();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
        "format_input_too_large"
    );
}

#[test]
fn project_format_bounds_total_bytes_and_rejects_invalid_utf8() {
    let fixture = Fixture::new();
    let mut request = fixture.request();
    request.files.clear();
    for index in 0..5 {
        let path = format!("src/file{index}.rs");
        fs::write(
            fixture.root.join(&path),
            vec![b' '; PROJECT_FORMAT_FILE_MAX_BYTES],
        )
        .unwrap();
        request.files.push(path);
    }
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_input_too_large"
    );
    fs::write(fixture.root.join("src/main.rs"), [0xff]).unwrap();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &fixture.request())),
        "format_invalid_utf8"
    );
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
#[ignore = "requires installed rustfmt; exercises the owned native process boundary"]
fn runner_real_process_project_format_rustfmt_stdin_leaves_sources_untouched() {
    use super::config::ShellConfig;
    use super::shell::{run_process_with_profiles_and_execution_state, PreparedShellProfileCache};
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join("src/main.rs"),
        "mod child;\nfn main( ){ }\n",
    )
    .unwrap();
    fs::write(fixture.root.join("child.rs"), "fn child( ){ }\n").unwrap();
    let planned = fixture.plan();
    let command = planned.plan.profile.process("src/main.rs").unwrap();
    let result = run_process_with_profiles_and_execution_state(
        1,
        &fixture.policy,
        &ShellConfig::default(),
        &fixture.registry,
        &PreparedShellProfileCache::default(),
        planned.cwd.to_str(),
        &command.executable,
        &command.args,
        Some(&planned.sources[0]),
        10,
        None,
    );
    assert_eq!(
        result.result.exit_code,
        Some(0),
        "{:?}",
        result.result.error
    );
    assert_eq!(
        result.result.stdout.as_deref(),
        Some("mod child;\nfn main() {}\n")
    );
    assert!(!result.stdout_truncated && !result.stderr_truncated);
    assert!(result
        .result
        .stderr
        .as_deref()
        .unwrap_or_default()
        .is_empty());
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/main.rs")).unwrap(),
        planned.sources[0]
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("child.rs")).unwrap(),
        "fn child( ){ }\n"
    );
}

#[test]
fn project_format_python_pins_local_target_and_rejects_extend() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(fixture.root.join("src/main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.adapter = ProjectFormatAdapter::Python;
    request.files = vec!["src/main.py".into()];
    for manifest in [
        "[tool.ruff]\n",
        "[tool.ruff]\ntarget-version='ambient'\n",
        "[tool.ruff]\ntarget-version='py311'\nextend='other.toml'\n",
    ] {
        fs::write(fixture.root.join("pyproject.toml"), manifest).unwrap();
        assert_eq!(
            rejection(plan(&fixture.policy, &fixture.registry, &request)),
            "format_manifest_invalid"
        );
    }
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    assert_eq!(
        planned.plan.profile,
        ProjectFormatProfile::Python {
            target_version: "py311".into()
        }
    );
    let result = handle(&fixture.policy, &fixture.registry, &request);
    let planning: ProjectFormatPlanningResult =
        serde_json::from_str(result.stdout.as_deref().unwrap()).unwrap();
    assert_eq!(
        planning,
        ProjectFormatPlanningResult::Ready { plan: planned.plan }
    );
}

#[cfg(unix)]
#[test]
fn project_format_rejects_symlink_hardlink_and_fifo_inputs() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let mut request = fixture.request();
    symlink("main.rs", fixture.root.join("src/link.rs")).unwrap();
    request.files = vec!["src/link.rs".into()];
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_path_invalid"
    );
    fs::hard_link(
        fixture.root.join("src/main.rs"),
        fixture.root.join("src/alias.rs"),
    )
    .unwrap();
    request.files = vec!["src/main.rs".into()];
    // Even a single selected path must not mutate an inode with other aliases.
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_alias_unavailable"
    );
    let fifo = fixture.root.join("src/pipe.rs");
    make_fifo(&fifo);
    request.files = vec!["src/pipe.rs".into()];
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_path_invalid"
    );
    // Deterministically replace a checked regular leaf at the protected open
    // boundary. Opening the FIFO must return an error without waiting for stdin.
    let target = fixture.root.canonicalize().unwrap().join("src/main.rs");
    let opened = super::file_access::open_regular_file_unix(&target, || {
        fs::remove_file(&target).unwrap();
        make_fifo(&target);
    });
    assert!(opened.is_err());
}

#[cfg(unix)]
fn make_fifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
}

#[test]
fn project_format_candidates_returns_formatted_output_and_unchanged_detection() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let candidates = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    )
    .expect("dry-run rustfmt candidate formatting succeeded");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], "fn main() {}\n");
    assert_ne!(candidates[0], planned.sources[0]);

    // Already formatted -> candidate equals original source
    fs::write(fixture.root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let planned_formatted = fixture.plan();
    let candidates_formatted = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned_formatted,
        &cache,
        1,
        10,
        None,
    )
    .expect("dry-run formatting formatted source succeeds");
    assert_eq!(candidates_formatted.len(), 1);
    assert_eq!(candidates_formatted[0], planned_formatted.sources[0]);

    // Original file content was untouched by candidate dry-run
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );
}

#[cfg(unix)]
#[test]
fn project_format_candidate_does_not_inherit_user_rustfmt_config() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join("src/main.rs"),
        "fn main(){if true{println!(\"test\");}}\n",
    )
    .unwrap();
    let planned = fixture.plan();
    let home_config = tempfile::tempdir().unwrap();
    let rustfmt_config = home_config.path().join("rustfmt");
    fs::create_dir(&rustfmt_config).unwrap();
    fs::write(rustfmt_config.join("rustfmt.toml"), "hard_tabs = true\n").unwrap();
    let mut shell = ShellConfig::default();
    shell.env.insert(
        "XDG_CONFIG_HOME".into(),
        home_config.path().to_string_lossy().into_owned(),
    );
    let output = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &PreparedShellProfileCache::default(),
        1,
        10,
        None,
    )
    .expect("closed rustfmt profile should not inherit user configuration");
    assert_eq!(
        output,
        vec!["fn main() {\n    if true {\n        println!(\"test\");\n    }\n}\n".to_string()]
    );
}

#[cfg(unix)]
#[test]
fn project_format_candidate_preserves_home_for_rustup_style_formatter() {
    use std::os::unix::fs::PermissionsExt;
    let temp = crate::tests::executable_tempdir();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join(".rustup")).unwrap();
    fs::write(home.join(".rustup/settings.toml"), "configured\n").unwrap();
    let fake_rustfmt = temp.path().join("rustfmt");
    fs::write(
        &fake_rustfmt,
        "#!/bin/sh\nif [ ! -f \"$HOME/.rustup/settings.toml\" ]; then\n  echo 'rustup home unavailable' >&2\n  exit 42\nfi\ncat\n",
    )
    .unwrap();
    fs::set_permissions(&fake_rustfmt, fs::Permissions::from_mode(0o755)).unwrap();
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let mut shell = ShellConfig::default();
    shell.path_prepend = vec![temp.path().to_path_buf()];
    shell
        .env
        .insert("HOME".into(), home.to_string_lossy().into_owned());
    let actual = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &PreparedShellProfileCache::default(),
        1,
        10,
        None,
    );
    assert_eq!(
        actual,
        Ok(planned.sources),
        "formatter toolchain lookup must retain its configured HOME"
    );
}

#[test]
fn project_format_candidates_fails_on_nonzero_exit_syntax_error() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("src/main.rs"), "fn broken syntax {{{ \n").unwrap();
    let planned = fixture.plan();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let result = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    );
    assert_eq!(result, Err("format_process_failed"));
}

#[test]
fn project_format_candidates_fails_on_truncated_output() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let mut policy = fixture.policy.clone();
    // Set max_output_bytes very low so the formatter output triggers truncation
    policy.max_output_bytes = 4;
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let result = format_candidates(
        &policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    );
    assert_eq!(result, Err("format_output_truncated"));
}

#[test]
fn project_format_candidates_fails_on_missing_rustfmt() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let mut shell = ShellConfig::default();
    // Restrict PATH to an empty directory so rustfmt cannot be located
    shell
        .env
        .insert("PATH".into(), "/nonexistent_bin_path_12345".into());
    shell.environment_mode = super::config::ShellEnvironmentMode::Isolated;
    let cache = PreparedShellProfileCache::default();
    let result = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    );
    assert_eq!(result, Err("format_process_failed"));
}

#[test]
fn project_format_candidates_enforces_deadline_and_stop_before_dispatch() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let cancelled = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        Some(&cancel),
    );
    assert_eq!(cancelled, Err("format_cancelled"));
    let invalid_budget = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        0,
        None,
    );
    assert_eq!(invalid_budget, Err("format_timeout_invalid"));
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/main.rs")).unwrap(),
        planned.sources[0]
    );
}

fn commit(
    fixture: &Fixture,
    planned: &PlannedFormat,
    candidates: &[String],
) -> Result<ProjectFormatMutationReport, &'static str> {
    super::project_format::commit_candidates(
        &fixture.policy,
        &fixture.registry,
        planned,
        candidates,
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &std::sync::atomic::AtomicBool::new(false),
    )
}

#[test]
fn project_format_pf07_noop_performs_no_write() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let before = fs::metadata(fixture.root.join("src/main.rs"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        commit(&fixture, &planned, &planned.sources),
        Ok(ProjectFormatMutationReport::Unchanged)
    );
    assert_eq!(
        fs::metadata(fixture.root.join("src/main.rs"))
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
}

#[test]
fn project_format_pf08_guarded_write_changes_only_selected_files() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("src/other.rs"), "fn other( ){ }\n").unwrap();
    let planned = fixture.plan();
    assert_eq!(
        commit(&fixture, &planned, &["fn main() {}\n".into()]),
        Ok(ProjectFormatMutationReport::Changed)
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/other.rs")).unwrap(),
        "fn other( ){ }\n"
    );
    assert_eq!(
        commit(&fixture, &planned, &["fn main() {}\n".into()]),
        Err("format_plan_stale")
    );
}

#[test]
fn project_format_pf09_candidate_commit_detects_source_and_manifest_conflicts() {
    for manifest in [false, true] {
        let fixture = Fixture::new();
        let planned = fixture.plan();
        if manifest {
            fs::write(
                fixture.root.join("Cargo.toml"),
                MANIFEST.replace("2021", "2024"),
            )
            .unwrap();
        } else {
            fs::write(fixture.root.join("src/main.rs"), "fn concurrent() {}\n").unwrap();
        }
        assert_eq!(
            commit(&fixture, &planned, &["fn main() {}\n".into()]),
            Err("format_plan_stale")
        );
    }
}

#[test]
fn project_format_commit_rechecks_context_before_each_file() {
    for revoke_permission in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("src/other.rs"), "fn other( ){ }\n").unwrap();
        let mut request = fixture.request();
        request.files.push("src/other.rs".into());
        let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
        let result = super::project_format::commit_candidates_with_hook(
            &fixture.policy,
            &fixture.registry,
            &planned,
            &["fn main() {}\n".into(), "fn other() {}\n".into()],
            std::time::Instant::now() + std::time::Duration::from_secs(10),
            &std::sync::atomic::AtomicBool::new(false),
            |index, after| {
                if index == 0 && after {
                    if revoke_permission {
                        fixture.register(false);
                    } else {
                        fs::write(
                            fixture.root.join("Cargo.toml"),
                            MANIFEST.replace("2021", "2024"),
                        )
                        .unwrap();
                    }
                }
            },
        );
        assert_eq!(
            result,
            Err("format_plan_stale"),
            "revoked={revoke_permission}"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("src/main.rs")).unwrap(),
            "fn main() {}\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("src/other.rs")).unwrap(),
            planned.sources[1]
        );
    }
}

#[test]
fn project_format_commit_rechecks_context_before_known_receipt() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let result = super::project_format::commit_candidates_with_hook(
        &fixture.policy,
        &fixture.registry,
        &planned,
        &["fn main() {}\n".into()],
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &std::sync::atomic::AtomicBool::new(false),
        |_, after| {
            if after {
                fixture.register(false);
            }
        },
    );
    assert_eq!(result, Err("format_plan_stale"));
}

#[test]
fn project_format_commit_rechecks_python_manifest_between_writes() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    let manifest = "[tool.ruff]\ntarget-version='py311'\n";
    fs::write(fixture.root.join("pyproject.toml"), manifest).unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    fs::write(fixture.root.join("other.py"), "y=2\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into(), "other.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let result = super::project_format::commit_candidates_with_hook(
        &fixture.policy,
        &fixture.registry,
        &planned,
        &["x = 1\n".into(), "y = 2\n".into()],
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &std::sync::atomic::AtomicBool::new(false),
        |index, after| {
            if index == 0 && after {
                fs::write(
                    fixture.root.join("pyproject.toml"),
                    manifest.replace("py311", "py312"),
                )
                .unwrap();
            }
        },
    );
    assert_eq!(result, Err("format_plan_stale"));
    assert_eq!(
        fs::read_to_string(fixture.root.join("other.py")).unwrap(),
        "y=2\n"
    );
}

#[test]
fn project_format_commit_accepts_its_own_previous_file_writes() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("src/other.rs"), "fn other( ){ }\n").unwrap();
    let mut request = fixture.request();
    request.files.push("src/other.rs".into());
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let candidates = ["fn main() {}\n".into(), "fn other() {}\n".into()];
    assert_eq!(
        commit(&fixture, &planned, &candidates),
        Ok(ProjectFormatMutationReport::Changed)
    );
    for (file, candidate) in planned.plan.files.iter().zip(&candidates) {
        assert_eq!(
            fs::read_to_string(fixture.root.join(&file.path)).unwrap(),
            *candidate
        );
    }
}

#[test]
fn project_format_pf10_partial_write_and_failed_observation_stay_unknown() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for scenario in 0..3 {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("src/other.rs"), "fn other( ){ }\n").unwrap();
        let mut request = fixture.request();
        request.files.push("src/other.rs".into());
        let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
        let stop = AtomicBool::new(false);
        let result = super::project_format::commit_candidates_with_hook(
            &fixture.policy,
            &fixture.registry,
            &planned,
            &["fn main() {}\n".into(), "fn other() {}\n".into()],
            std::time::Instant::now() + std::time::Duration::from_secs(10),
            &stop,
            |index, after| {
                if index == 0 && after {
                    match scenario {
                        0 => {
                            fs::write(fixture.root.join("src/other.rs"), "concurrent modification")
                                .unwrap();
                        }
                        1 => {
                            fs::write(fixture.root.join("src/main.rs"), "postwrite conflict")
                                .unwrap();
                        }
                        _ => stop.store(true, Ordering::SeqCst),
                    }
                }
            },
        );
        assert_eq!(
            result,
            Err(match scenario {
                0 => "format_write_conflict",
                1 => "format_observation_failed",
                _ => "format_cancelled",
            })
        );
        assert_ne!(
            fs::read_to_string(fixture.root.join("src/main.rs")).unwrap(),
            planned.sources[0]
        );
    }
}

#[test]
fn project_format_pf09_replaced_object_with_same_contents_never_receives_write() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let result = super::project_format::commit_candidates_with_hook(
        &fixture.policy,
        &fixture.registry,
        &planned,
        &["fn main() {}\n".into()],
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &std::sync::atomic::AtomicBool::new(false),
        |_, after| {
            if !after {
                fs::rename(
                    fixture.root.join("src/main.rs"),
                    fixture.root.join("src/original.rs"),
                )
                .unwrap();
                fs::write(fixture.root.join("src/main.rs"), &planned.sources[0]).unwrap();
            }
        },
    );
    assert_eq!(result, Err("format_write_conflict"));
    assert_eq!(
        fs::read_to_string(fixture.root.join("src/main.rs")).unwrap(),
        planned.sources[0]
    );
}

#[test]
fn project_format_pf06_python_execution_remains_unavailable() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let mut shell = ShellConfig::default();
    // Controlled isolated PATH ensures missing interpreter fail-closed behavior
    // without relying on the ambient environment lacking Python.
    shell
        .env
        .insert("PATH".into(), "/nonexistent/isolated/bin".into());
    assert_eq!(
        format_candidates(
            &fixture.policy,
            &shell,
            &fixture.registry,
            &planned,
            &PreparedShellProfileCache::default(),
            1,
            10,
            None
        ),
        Err("format_tool_unavailable")
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("main.py")).unwrap(),
        "x=1\n"
    );
}

#[test]
fn project_format_handle_returns_ready_for_valid_rust_request() {
    let fixture = Fixture::new();
    let request = fixture.request();
    let result = handle(&fixture.policy, &fixture.registry, &request);
    assert_eq!(result.exit_code, Some(0));
    let planning: ProjectFormatPlanningResult =
        serde_json::from_str(result.stdout.as_deref().unwrap()).unwrap();
    let ProjectFormatPlanningResult::Ready { plan } = planning else {
        panic!("expected Ready format plan");
    };
    assert!(plan.is_valid());
    assert_eq!(plan.request, request);
}

#[test]
fn project_format_fence_validates_authority_and_stale_detection() {
    use webcodex_core::runner_operation::{RunnerJobFormatOperation, RunnerJobOperation};
    use webcodex_core::runner_protocol::ShellJobContext;

    let fixture = Fixture::new();
    let planned = fixture.plan();
    let context = ShellJobContext {
        runtime_project_id: Some("agent:runner:demo".into()),
        workflow_session_id: None,
        ssh_resource: None,
        project_cwd: Some(".".into()),
        cwd: Some(planned.cwd.to_string_lossy().into()),
        purpose: Some("format".into()),
        shell: Some("direct_argv".into()),
        command_preview: "project_format selected files".into(),
        validation_steps: vec![],
        validation: None,
        structured_execution: Some(structured_metadata()),
    };
    let operation = RunnerJobOperation::StartFormat(RunnerJobFormatOperation {
        job_id: "job-1".into(),
        cwd: context.cwd.clone(),
        plan: planned.plan.clone(),
        timeout_secs: 60,
        context: context.clone(),
    });
    assert!(super::project_format::fence(&fixture.policy, &fixture.registry, &operation).is_ok());

    // Mismatched project ID
    let mut mismatched_proj = operation.clone();
    if let RunnerJobOperation::StartFormat(ref mut op) = mismatched_proj {
        op.context.runtime_project_id = Some("agent:runner:other".into());
    }
    assert_eq!(
        super::project_format::fence(&fixture.policy, &fixture.registry, &mismatched_proj),
        Err("format_scope_mismatch".into())
    );

    // Mismatched cwd
    let mut mismatched_cwd = operation.clone();
    if let RunnerJobOperation::StartFormat(ref mut op) = mismatched_cwd {
        op.cwd = Some("/wrong/path".into());
    }
    assert_eq!(
        super::project_format::fence(&fixture.policy, &fixture.registry, &mismatched_cwd),
        Err("format_plan_stale".into())
    );

    // Stale plan on disk
    fs::write(fixture.root.join("src/main.rs"), "fn modified() {}\n").unwrap();
    assert_eq!(
        super::project_format::fence(&fixture.policy, &fixture.registry, &operation),
        Err("format_plan_stale".into())
    );
}

#[test]
fn project_format_candidates_fails_on_missing_python() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let mut shell = ShellConfig::default();
    shell
        .env
        .insert("PATH".into(), "/nonexistent_bin_path_12345".into());
    shell.environment_mode = super::config::ShellEnvironmentMode::Isolated;
    let cache = PreparedShellProfileCache::default();
    assert_eq!(
        format_candidates(
            &fixture.policy,
            &shell,
            &fixture.registry,
            &planned,
            &cache,
            1,
            10,
            None,
        ),
        Err("format_tool_unavailable")
    );
}

#[test]
fn project_format_candidates_fails_on_missing_ruff_module() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let cache = PreparedShellProfileCache::default();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let temp = crate::tests::executable_tempdir();
        // Exit 42 models python present without the ruff module installed
        let script = "#!/bin/sh\nexit 42\n";
        let mock_py = temp.path().join("python3");
        fs::write(&mock_py, script).unwrap();
        fs::set_permissions(&mock_py, fs::Permissions::from_mode(0o755)).unwrap();

        let mut shell = ShellConfig::default();
        shell.path_prepend = vec![temp.path().to_path_buf()];
        assert_eq!(
            format_candidates(
                &fixture.policy,
                &shell,
                &fixture.registry,
                &planned,
                &cache,
                1,
                10,
                None,
            ),
            Err("format_tool_unavailable")
        );
    }
    #[cfg(not(unix))]
    {
        let mut shell = ShellConfig::default();
        shell
            .env
            .insert("PATH".into(), "/nonexistent/isolated/bin".into());
        assert_eq!(
            format_candidates(
                &fixture.policy,
                &shell,
                &fixture.registry,
                &planned,
                &cache,
                1,
                10,
                None,
            ),
            Err("format_tool_unavailable")
        );
    }
}

#[cfg(unix)]
#[test]
#[ignore = "real shell-profile subprocess timing; run the explicit project format deadline evidence lane"]
fn project_format_rust_profile_preparation_obeys_total_job_deadline() {
    use super::config::ShellProfileConfig;
    use std::time::{Duration, Instant};
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let mut shell = ShellConfig::default();
    shell.default_profile = Some("slow".into());
    shell.profiles.insert(
        "slow".into(),
        ShellProfileConfig {
            init_script: Some("sleep 5".into()),
            ..Default::default()
        },
    );
    let start = Instant::now();
    let result = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &PreparedShellProfileCache::default(),
        1,
        1,
        None,
    );
    assert_eq!(result, Err("format_timeout"));
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "rust profile preparation exceeded the total Job deadline"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "real shell-profile subprocess timing; run the explicit project format deadline evidence lane"]
fn project_format_python_profile_preparation_obeys_total_job_deadline() {
    use super::config::ShellProfileConfig;
    use std::time::{Duration, Instant};
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();

    let mut shell = ShellConfig::default();
    shell.default_profile = Some("slow".into());
    shell.profiles.insert(
        "slow".into(),
        ShellProfileConfig {
            init_script: Some("sleep 5".into()),
            ..Default::default()
        },
    );
    let start = Instant::now();
    let result = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &PreparedShellProfileCache::default(),
        1,
        1,
        None,
    );
    assert_eq!(result, Err("format_timeout"));
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "shell profile preparation exceeded the 1-second Job deadline"
    );
}

#[cfg(unix)]
#[test]
fn project_format_python_profile_error_text_is_not_job_timeout() {
    use super::config::ShellProfileConfig;
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let mut shell = ShellConfig::default();
    shell.default_profile = Some("invalid".into());
    shell.profiles.insert(
        "invalid".into(),
        ShellProfileConfig {
            init_script: Some("printf 'timed out from profile' >&2; exit 42".into()),
            ..Default::default()
        },
    );
    assert_eq!(
        format_candidates(
            &fixture.policy,
            &shell,
            &fixture.registry,
            &planned,
            &PreparedShellProfileCache::default(),
            1,
            10,
            None,
        ),
        Err("format_process_failed"),
        "a profile's stderr text cannot impersonate the total Job timeout"
    );
}

#[cfg(unix)]
#[test]
fn project_format_candidates_python_ruff_probe_timeout_fails_closed() {
    use std::os::unix::fs::PermissionsExt;
    let temp = crate::tests::executable_tempdir();
    // Script sleeps longer than total timeout budget
    let script = r#"#!/bin/sh
sleep 2
exit 0
"#;
    let mock_py = temp.path().join("python3");
    fs::write(&mock_py, script).unwrap();
    fs::set_permissions(&mock_py, fs::Permissions::from_mode(0o755)).unwrap();

    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();

    let mut shell = ShellConfig::default();
    shell.path_prepend = vec![temp.path().to_path_buf()];
    let cache = PreparedShellProfileCache::default();

    // 1 second timeout budget: probe must count against budget and yield format_timeout
    let result = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        1,
        None,
    );
    assert_eq!(result, Err("format_timeout"));
}

#[test]
fn project_format_candidates_real_ruff_opt_in_when_installed() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x = 1+2\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let probe_ok = std::process::Command::new("python3")
        .args([
            "-I",
            "-B",
            "-c",
            "import sys,importlib.util;sys.exit(0 if sys.version_info.major == 3 and importlib.util.find_spec('ruff') else 42)",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !probe_ok {
        return;
    }
    let candidates = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    )
    .unwrap();
    assert_eq!(candidates, vec!["x = 1 + 2\n".to_string()]);
}

#[cfg(unix)]
#[test]
fn project_format_candidates_python_ruff_mock_fixture_formats_and_detects_changes() {
    use std::os::unix::fs::PermissionsExt;
    let temp = crate::tests::executable_tempdir();
    let script = r#"#!/bin/sh
if [ -n "$RUFF_OUTPUT_FILE" ]; then
    echo "RUFF_OUTPUT_FILE was not removed" >&2
    exit 55
fi
if [ "$PYTHONDONTWRITEBYTECODE" != "1" ]; then
    echo "PYTHONDONTWRITEBYTECODE not set" >&2
    exit 56
fi
case "$*" in
    *"find_spec('ruff')"*)
        exit 0
        ;;
    *"-m ruff format"*)
        cat > /dev/null
        printf 'formatted = True\n'
        exit 0
        ;;
    *)
        exit 42
        ;;
esac
"#;
    let mock_py = temp.path().join("python3");
    fs::write(&mock_py, script).unwrap();
    fs::set_permissions(&mock_py, fs::Permissions::from_mode(0o755)).unwrap();

    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "formatted=False\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();

    let mut shell = ShellConfig::default();
    shell.path_prepend = vec![temp.path().to_path_buf()];
    let cache = PreparedShellProfileCache::default();

    let candidates = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    )
    .unwrap();
    assert_eq!(candidates, vec!["formatted = True\n".to_string()]);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let stop = std::sync::atomic::AtomicBool::new(false);
    let report = commit_candidates(
        &fixture.policy,
        &fixture.registry,
        &planned,
        &candidates,
        deadline,
        &stop,
    )
    .unwrap();
    assert_eq!(report, ProjectFormatMutationReport::Changed);
    assert_eq!(
        fs::read_to_string(fixture.root.join("main.py")).unwrap(),
        "formatted = True\n"
    );
}

#[cfg(unix)]
#[test]
fn project_format_candidates_python_ruff_syntax_error_fails() {
    use std::os::unix::fs::PermissionsExt;
    let temp = crate::tests::executable_tempdir();
    let script = r#"#!/bin/sh
case "$*" in
    *"find_spec('ruff')"*)
        exit 0
        ;;
    *"-m ruff format"*)
        cat > /dev/null
        echo "syntax error: unclosed parenthesis" >&2
        exit 1
        ;;
    *)
        exit 42
        ;;
esac
"#;
    let mock_py = temp.path().join("python3");
    fs::write(&mock_py, script).unwrap();
    fs::set_permissions(&mock_py, fs::Permissions::from_mode(0o755)).unwrap();

    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "def foo(\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();

    let mut shell = ShellConfig::default();
    shell.path_prepend = vec![temp.path().to_path_buf()];
    let cache = PreparedShellProfileCache::default();

    let result = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    );
    assert_eq!(result, Err("format_process_failed"));
}

#[cfg(unix)]
#[test]
fn project_format_candidates_fails_on_unexpected_stderr() {
    use std::os::unix::fs::PermissionsExt;
    let temp = crate::tests::executable_tempdir();
    let script = r#"#!/bin/sh
case "$*" in
    *"find_spec('ruff')"*)
        exit 0
        ;;
    *"-m ruff format"*)
        cat > /dev/null
        printf 'warning: something unexpected\n' >&2
        printf 'x = 1\n'
        exit 0
        ;;
    *)
        exit 42
        ;;
esac
"#;
    let mock_py = temp.path().join("python3");
    fs::write(&mock_py, script).unwrap();
    fs::set_permissions(&mock_py, fs::Permissions::from_mode(0o755)).unwrap();

    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "x=1\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();

    let mut shell = ShellConfig::default();
    shell.path_prepend = vec![temp.path().to_path_buf()];
    let cache = PreparedShellProfileCache::default();

    let result = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    );
    assert_eq!(result, Err("format_stderr_unexpected"));
}

#[cfg(unix)]
#[test]
fn project_format_candidates_nonzero_exit_with_truncated_stderr_fails_closed() {
    use std::os::unix::fs::PermissionsExt;
    let temp = crate::tests::executable_tempdir();
    let script = r#"#!/bin/sh
case "$*" in
    *"find_spec('ruff')"*)
        exit 0
        ;;
    *"-m ruff format"*)
        cat > /dev/null
        printf 'very long error message exceeding max output bytes\n' >&2
        exit 1
        ;;
    *)
        exit 42
        ;;
esac
"#;
    let mock_py = temp.path().join("python3");
    fs::write(&mock_py, script).unwrap();
    fs::set_permissions(&mock_py, fs::Permissions::from_mode(0o755)).unwrap();

    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    fs::write(fixture.root.join("main.py"), "def foo(\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.py".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();

    let mut shell = ShellConfig::default();
    shell.path_prepend = vec![temp.path().to_path_buf()];
    let mut policy = fixture.policy.clone();
    policy.max_output_bytes = 10;
    let cache = PreparedShellProfileCache::default();

    let result = format_candidates(
        &policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    );
    assert_eq!(result, Err("format_output_truncated"));
}

#[test]
fn project_format_candidates_succeeds_with_one_second_timeout_budget() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let candidates = format_candidates(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        1,
        None,
    )
    .expect("valid 1s timeout budget must not fail to floor or spurious validation error");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], "fn main() {}\n");
}

#[test]
fn project_format_candidates_with_deadline_fails_closed_when_deadline_exhausted() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let past_deadline = std::time::Instant::now() - std::time::Duration::from_millis(50);
    let result = format_candidates_with_deadline(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        past_deadline,
        None,
    );
    assert_eq!(result, Err("format_timeout"));
}

#[test]
fn project_format_candidates_with_deadline_respects_cancellation() {
    let fixture = Fixture::new();
    let planned = fixture.plan();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let stop = std::sync::atomic::AtomicBool::new(true);
    let future_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let result = format_candidates_with_deadline(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        future_deadline,
        Some(&stop),
    );
    assert_eq!(result, Err("format_cancelled"));
}

#[test]
fn project_format_candidates_with_deadline_fails_closed_on_deadline_across_multiple_files() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("src/lib.rs"), "pub fn lib( ){ }\n").unwrap();
    let mut request = fixture.request();
    request.files = vec!["src/main.rs".into(), "src/lib.rs".into()];
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let shell = ShellConfig::default();
    let cache = PreparedShellProfileCache::default();
    let expired_deadline = std::time::Instant::now() - std::time::Duration::from_millis(1);
    let result = format_candidates_with_deadline(
        &fixture.policy,
        &shell,
        &fixture.registry,
        &planned,
        &cache,
        1,
        expired_deadline,
        None,
    );
    assert_eq!(result, Err("format_timeout"));
}

fn go_fixture() -> (Fixture, ProjectFormatRequest) {
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.join("Cargo.toml")).unwrap();
    fs::write(
        fixture.root.join("go.mod"),
        "module example.invalid/format\n\ngo 1.23\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("main.go"),
        "package main\nfunc main( ){ }\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("other.go"),
        "package main\nvar answer=42\n",
    )
    .unwrap();
    let mut request = fixture.request();
    request.files = vec!["main.go".into()];
    (fixture, request)
}

#[test]
fn project_format_go_plans_explicit_and_auto_without_toml_or_module_loading() {
    let (fixture, mut request) = go_fixture();
    let automatic = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    assert_eq!(automatic.plan.profile, ProjectFormatProfile::Go {});
    request.adapter = ProjectFormatAdapter::Go;
    let explicit = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    assert_eq!(explicit.plan.profile, automatic.plan.profile);
    assert_eq!(explicit.sources, automatic.sources);
    // go.mod is an identity-bound recipe marker. gofmt never loads its grammar,
    // dependencies, toolchain directive, go.work, or project-authored scripts.
    fs::write(
        fixture.root.join("go.mod"),
        "opaque Go recipe marker, not TOML\n",
    )
    .unwrap();
    assert!(plan(&fixture.policy, &fixture.registry, &request).is_ok());
    assert_eq!(
        replan(&fixture.policy, &fixture.registry, &explicit.plan).err(),
        Some("format_plan_stale")
    );
    let encoded = serde_json::to_string(&explicit.plan).unwrap();
    assert!(!encoded.contains("func main"));
}

#[test]
fn project_format_go_rejects_nested_modules_ambiguity_and_permission_changes() {
    let (fixture, mut request) = go_fixture();
    let original = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    fixture.register(false);
    assert_eq!(
        replan(&fixture.policy, &fixture.registry, &original.plan).err(),
        Some("format_plan_stale")
    );
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "permission_denied"
    );
    fixture.register(true);
    fs::create_dir(fixture.root.join("nested")).unwrap();
    fs::write(
        fixture.root.join("nested/go.mod"),
        "module example.invalid/nested\n",
    )
    .unwrap();
    fs::write(fixture.root.join("nested/main.go"), "package nested\n").unwrap();
    request.files = vec!["nested/main.go".into()];
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_scope_mismatch"
    );
    request.cwd = Some("nested".into());
    assert!(plan(&fixture.policy, &fixture.registry, &request).is_ok());
    fs::write(
        fixture.root.join("nested/pyproject.toml"),
        "[project]\nname='conflict'\n",
    )
    .unwrap();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_recipe_ambiguous"
    );
}

#[test]
fn project_format_go_rechecks_manifest_identity_and_stops_partial_commit() {
    let (fixture, mut request) = go_fixture();
    request.files.push("other.go".into());
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let result = super::project_format::commit_candidates_with_hook(
        &fixture.policy,
        &fixture.registry,
        &planned,
        &[
            "package main\n\nfunc main() {}\n".into(),
            "package main\n\nvar answer = 42\n".into(),
        ],
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        &std::sync::atomic::AtomicBool::new(false),
        |index, after| {
            if index == 0 && after {
                let marker = fixture.root.join("go.mod");
                let bytes = fs::read(&marker).unwrap();
                fs::rename(&marker, fixture.root.join("retired.mod")).unwrap();
                fs::write(&marker, bytes).unwrap();
            }
        },
    );
    assert_eq!(result, Err("format_plan_stale"));
    assert_eq!(
        fs::read_to_string(fixture.root.join("main.go")).unwrap(),
        "package main\n\nfunc main() {}\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("other.go")).unwrap(),
        "package main\nvar answer=42\n"
    );
}

#[test]
fn project_format_go_missing_tool_and_pre_dispatch_stop_leave_sources_untouched() {
    let (fixture, request) = go_fixture();
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let mut shell = ShellConfig::default();
    shell.environment_mode = super::config::ShellEnvironmentMode::Isolated;
    shell.env.insert(
        "PATH".into(),
        fixture
            .root
            .join("empty-bin")
            .to_string_lossy()
            .into_owned(),
    );
    let cache = PreparedShellProfileCache::default();
    assert_eq!(
        format_candidates(
            &fixture.policy,
            &shell,
            &fixture.registry,
            &planned,
            &cache,
            1,
            10,
            None
        ),
        Err("format_process_failed")
    );
    assert_eq!(
        format_candidates(
            &fixture.policy,
            &shell,
            &fixture.registry,
            &planned,
            &cache,
            1,
            10,
            Some(&std::sync::atomic::AtomicBool::new(true))
        ),
        Err("format_cancelled")
    );
    assert_eq!(
        format_candidates_with_deadline(
            &fixture.policy,
            &shell,
            &fixture.registry,
            &planned,
            &cache,
            1,
            std::time::Instant::now(),
            None
        ),
        Err("format_timeout")
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("main.go")).unwrap(),
        planned.sources[0]
    );
}

#[test]
fn project_format_go_manifest_and_source_bounds_are_not_bypassed() {
    let (fixture, request) = go_fixture();
    for (file, bytes, expected) in [
        ("go.mod", vec![0xff], "format_invalid_utf8"),
        (
            "go.mod",
            vec![b'x'; PROJECT_FORMAT_MANIFEST_MAX_BYTES + 1],
            "format_input_too_large",
        ),
    ] {
        fs::write(fixture.root.join(file), bytes).unwrap();
        assert_eq!(
            rejection(plan(&fixture.policy, &fixture.registry, &request)),
            expected
        );
    }
    fs::write(
        fixture.root.join("go.mod"),
        "module example.invalid/format\n",
    )
    .unwrap();
    fs::write(
        fixture.root.join("main.go"),
        vec![b'x'; PROJECT_FORMAT_FILE_MAX_BYTES + 1],
    )
    .unwrap();
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_input_too_large"
    );
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
#[ignore = "requires installed gofmt; exercises candidate output and guarded writes"]
fn runner_real_process_project_format_gofmt_changes_only_selected_and_noop_does_not_write() {
    let (fixture, request) = go_fixture();
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let cache = PreparedShellProfileCache::default();
    let candidates = format_candidates(
        &fixture.policy,
        &ShellConfig::default(),
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    )
    .unwrap();
    assert_eq!(candidates, ["package main\n\nfunc main() {}\n"]);
    assert_eq!(
        fs::read_to_string(fixture.root.join("main.go")).unwrap(),
        planned.sources[0]
    );
    assert_eq!(
        commit(&fixture, &planned, &candidates),
        Ok(ProjectFormatMutationReport::Changed)
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("other.go")).unwrap(),
        "package main\nvar answer=42\n"
    );
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let metadata = fs::metadata(fixture.root.join("main.go")).unwrap();
    let candidates = format_candidates(
        &fixture.policy,
        &ShellConfig::default(),
        &fixture.registry,
        &planned,
        &cache,
        1,
        10,
        None,
    )
    .unwrap();
    assert_eq!(
        commit(&fixture, &planned, &candidates),
        Ok(ProjectFormatMutationReport::Unchanged)
    );
    assert_eq!(
        metadata.modified().unwrap(),
        fs::metadata(fixture.root.join("main.go"))
            .unwrap()
            .modified()
            .unwrap()
    );
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
#[ignore = "requires installed gofmt; exercises syntax failure and output limits"]
fn runner_real_process_project_format_gofmt_errors_never_commit_candidates() {
    let (fixture, mut request) = go_fixture();
    request.files.push("other.go".into());
    fs::write(
        fixture.root.join("other.go"),
        "package main\nfunc broken( {\n",
    )
    .unwrap();
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    let cache = PreparedShellProfileCache::default();
    assert_eq!(
        format_candidates(
            &fixture.policy,
            &ShellConfig::default(),
            &fixture.registry,
            &planned,
            &cache,
            1,
            10,
            None
        ),
        Err("format_process_failed")
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("main.go")).unwrap(),
        planned.sources[0]
    );
    let mut policy = fixture.policy.clone();
    policy.max_output_bytes = 4;
    request.files.pop();
    let planned = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    assert_eq!(
        format_candidates(
            &policy,
            &ShellConfig::default(),
            &fixture.registry,
            &planned,
            &cache,
            1,
            10,
            None
        ),
        Err("format_output_truncated")
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("main.go")).unwrap(),
        planned.sources[0]
    );
}

#[cfg(unix)]
#[test]
fn project_format_go_rejects_source_aliases_and_invalid_module_markers() {
    use std::os::unix::fs::symlink;
    let (fixture, mut request) = go_fixture();
    let original = plan(&fixture.policy, &fixture.registry, &request).unwrap();
    fs::write(fixture.root.join("main.go"), "package changed\n").unwrap();
    assert_eq!(
        replan(&fixture.policy, &fixture.registry, &original.plan).err(),
        Some("format_plan_stale")
    );
    symlink(fixture.root.join("main.go"), fixture.root.join("alias.go")).unwrap();
    request.files = vec!["alias.go".into()];
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_path_invalid"
    );
    fs::hard_link(fixture.root.join("main.go"), fixture.root.join("hard.go")).unwrap();
    request.files = vec!["hard.go".into()];
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_alias_unavailable"
    );
    fs::remove_file(fixture.root.join("hard.go")).unwrap();
    fs::rename(fixture.root.join("go.mod"), fixture.root.join("saved.mod")).unwrap();
    symlink(fixture.root.join("saved.mod"), fixture.root.join("go.mod")).unwrap();
    request.files = vec!["main.go".into()];
    assert_eq!(
        rejection(plan(&fixture.policy, &fixture.registry, &request)),
        "format_manifest_invalid"
    );
}
