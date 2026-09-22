use super::*;
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(args: &[&str], cwd: &Path) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(name);
    let state = temp.path().join("state");
    fs::create_dir(&root).unwrap();
    git(&["init", "-q"], &root);
    git(&["config", "core.autocrlf", "false"], &root);
    git(&["config", "core.longpaths", "true"], &root);
    fs::write(root.join("README.md"), "fixture\n").unwrap();
    git(&["add", "README.md"], &root);
    git(
        &[
            "-c",
            "user.name=WebCodex Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ],
        &root,
    );
    (temp, root, state)
}

fn options(root: PathBuf, state: PathBuf) -> ProjectCommandOptions {
    ProjectCommandOptions {
        root,
        profile: "personal".to_string(),
        state_dir: Some(state),
        json: false,
        console_assets_dir: None,
    }
}

fn write_console_assets(directory: &Path) {
    fs::create_dir_all(directory).unwrap();
    fs::write(directory.join("runtime.html"), "<html></html>\n").unwrap();
    fs::write(directory.join("app.js"), "globalThis.runtimeDev = true;\n").unwrap();
    fs::write(directory.join("styles.css"), "body { color: black; }\n").unwrap();
}

#[test]
fn console_assets_are_validated_and_passed_only_to_the_serve_child() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("assets");
    write_console_assets(&directory);
    let mut start_options = options(
        temp.path().join("project"),
        temp.path().join("project-state"),
    );
    start_options.console_assets_dir = Some(directory.clone());

    let canonical = resolve_console_assets_directory(&start_options)
        .unwrap()
        .unwrap();
    assert_eq!(canonical, fs::canonicalize(&directory).unwrap());

    let mut command = tokio::process::Command::new("webcodex");
    configure_console_assets_environment(&mut command, Some(&canonical));
    let configured = command
        .as_std()
        .get_envs()
        .find(|(key, _)| *key == crate::console_web::CONSOLE_ASSETS_DIR_ENV)
        .and_then(|(_, value)| value)
        .map(PathBuf::from);
    assert_eq!(configured, Some(canonical));

    let mut embedded_command = tokio::process::Command::new("webcodex");
    configure_console_assets_environment(&mut embedded_command, None);
    assert!(embedded_command
        .as_std()
        .get_envs()
        .any(|(key, value)| key == crate::console_web::CONSOLE_ASSETS_DIR_ENV && value.is_none()));

    fs::remove_file(directory.join("styles.css")).unwrap();
    let error = resolve_console_assets_directory(&start_options).unwrap_err();
    assert_eq!(error.code, "console_assets_invalid");
    assert!(error.message.contains("styles.css"));
}

#[test]
fn npm_wrapper_network_credentials_are_removed_from_runtime_children() {
    let mut command = tokio::process::Command::new("webcodex-runner");
    for key in NPM_WRAPPER_NETWORK_ENV_KEYS {
        command.env(key, "credential-like-value");
    }
    command.env("WEBCODEX_TEST_UNRELATED_ENV", "preserved");

    remove_npm_wrapper_network_environment(&mut command);
    let envs: Vec<_> = command.as_std().get_envs().collect();
    for key in NPM_WRAPPER_NETWORK_ENV_KEYS {
        assert!(
            envs.iter()
                .any(|(candidate, value)| { candidate.to_str() == Some(key) && value.is_none() }),
            "runtime child did not remove wrapper-only environment key {key}"
        );
    }
    assert!(envs.iter().any(|(key, value)| {
        key.to_str() == Some("WEBCODEX_TEST_UNRELATED_ENV")
            && value.and_then(|value| value.to_str()) == Some("preserved")
    }));
}

#[test]
fn runner_parent_credentials_are_removed_before_spawn() {
    let mut command = tokio::process::Command::new("webcodex-runner");
    for key in ["WEBCODEX_TOKEN", "WEBCODEX_PAT", "WEBCODEX_AGENT_TOKEN"] {
        command.env(key, "credential-like-value");
    }
    command.env("WEBCODEX_TEST_UNRELATED_ENV", "preserved");

    remove_runner_parent_credentials(&mut command);
    let envs: Vec<_> = command.as_std().get_envs().collect();
    for key in ["WEBCODEX_TOKEN", "WEBCODEX_PAT", "WEBCODEX_AGENT_TOKEN"] {
        assert!(
            envs.iter()
                .any(|(candidate, value)| { candidate.to_str() == Some(key) && value.is_none() }),
            "Runner parent credential was not removed before spawn: {key}"
        );
    }
    assert!(envs.iter().any(|(key, value)| {
        key.to_str() == Some("WEBCODEX_TEST_UNRELATED_ENV")
            && value.and_then(|value| value.to_str()) == Some("preserved")
    }));
}

fn fact<'a>(readiness: &'a ProjectReadiness, code: &str) -> &'a ReadinessFact {
    readiness
        .findings
        .iter()
        .find(|finding| finding.code == code)
        .unwrap_or_else(|| panic!("missing readiness fact {code}: {readiness:?}"))
}

fn assert_no_project_state_artifacts(root: &Path) {
    for relative in [
        "credentials",
        "agent",
        "project.toml",
        "runs",
        "results",
        ".webcodex",
    ] {
        assert!(
            !root.join(relative).exists(),
            "unsafe state resolution created {relative} inside the checkout"
        );
    }
}

#[tokio::test]
async fn state_directory_boundary_is_shared_and_has_no_failure_side_effects() {
    let (temp, root, _) = repo("state-boundary");

    let relative_inside = ProjectCommandOptions {
        state_dir: Some(PathBuf::from(".webcodex")),
        ..options(root.clone(), temp.path().join("unused"))
    };
    let relative_error =
        setup_service::resolve_state_path_from(&relative_inside, &root, "unused-project-id", &root)
            .unwrap_err();
    assert_eq!(relative_error.code, "state_directory_unsafe");
    assert_no_project_state_artifacts(&root);

    for state in [root.clone(), root.join(".webcodex")] {
        let unsafe_options = options(root.clone(), state);
        let setup_error = setup(&unsafe_options).unwrap_err();
        assert_eq!(setup_error.code, "state_directory_unsafe");
        assert!(setup_error.message.contains("outside"));

        let readiness = readiness_with_probe(&unsafe_options, RemoteProbe::Unreachable);
        assert_eq!(
            fact(&readiness, "state_directory_unsafe").status,
            ReadinessStatus::Fail
        );
        let start_error = start_runner(&unsafe_options).await.unwrap_err();
        assert_eq!(start_error.code, "state_directory_unsafe");
        assert_no_project_state_artifacts(&root);
    }
}

#[test]
fn state_directory_allows_absolute_and_relative_paths_outside_checkout() {
    let (temp, root, _) = repo("outside-state");

    let absolute = temp.path().join("absolute-state");
    setup(&options(root.clone(), absolute.clone())).unwrap();
    assert!(absolute.join("credentials/project-credential").is_file());
    assert!(absolute.join("credentials/agent-token").is_file());

    let relative_options = ProjectCommandOptions {
        state_dir: Some(PathBuf::from("../relative-state")),
        ..options(root.clone(), temp.path().join("unused"))
    };
    let resolved = setup_service::resolve_state_path_from(
        &relative_options,
        &root,
        "unused-project-id",
        &root,
    )
    .unwrap();
    assert_eq!(
        resolved,
        temp.path().canonicalize().unwrap().join("relative-state")
    );
    setup(&options(root, resolved.clone())).unwrap();
    assert!(resolved.join("project.toml").is_file());
}

#[test]
fn default_state_base_preserves_home_and_has_windows_localappdata_fallback() {
    use std::ffi::OsStr;

    assert_eq!(
        setup_service::default_state_base_from(
            Some(OsStr::new("/state")),
            Some(OsStr::new("/home/user")),
            Some(OsStr::new("/local")),
        )
        .unwrap(),
        PathBuf::from("/state/webcodex/projects")
    );
    assert_eq!(
        setup_service::default_state_base_from(
            None,
            Some(OsStr::new("/home/user")),
            Some(OsStr::new("/local")),
        )
        .unwrap(),
        PathBuf::from("/home/user/.local/state/webcodex/projects")
    );
    assert_eq!(
        setup_service::default_state_base_from(None, None, Some(OsStr::new("/local"))).unwrap(),
        PathBuf::from("/local/WebCodex/state/projects")
    );
    assert!(setup_service::default_state_base_from(None, None, None).is_err());
}

#[cfg(unix)]
#[test]
fn state_directory_rejects_symlink_that_resolves_into_checkout() {
    use std::os::unix::fs::symlink;

    let (temp, root, _) = repo("symlink-state");
    let link = temp.path().join("state-link");
    symlink(&root, &link).unwrap();
    let unsafe_state = link.join(".webcodex");
    let error = setup(&options(root.clone(), unsafe_state)).unwrap_err();

    assert_eq!(error.code, "state_directory_unsafe");
    assert_no_project_state_artifacts(&root);
}

#[test]
fn fresh_setup_is_minimal_idempotent_and_does_not_expose_internal_ids() {
    let (_temp, root, state) = repo("demo");
    let options = options(root, state.clone());

    let first = setup(&options).unwrap();
    assert_eq!(first.status, "configured");
    assert_eq!(
        first.changed,
        ["Connection", "Runner", "Project registration"]
    );
    let agent = fs::read_to_string(state.join("agent/runner.toml")).unwrap();
    let registration = fs::read_to_string(
        fs::read_dir(state.join("agent/project-registry"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    let agent_toml: toml::Value = toml::from_str(&agent).unwrap();
    let registration_toml: toml::Value = toml::from_str(&registration).unwrap();
    let project_credential =
        read_private_value(&state.join("credentials/project-credential")).unwrap();
    let agent_token = read_private_value(&state.join("credentials/agent-token")).unwrap();
    assert_ne!(project_credential, agent_token);
    assert!(agent_token.starts_with("wc_agent_"));
    assert_eq!(agent_toml["token"].as_str(), Some(agent_token.as_str()));
    let canonical_root = options.root.canonicalize().unwrap();
    let canonical_state = state.canonicalize().unwrap();
    assert_eq!(
        registration_toml["path"].as_str(),
        Some(canonical_root.to_string_lossy().as_ref())
    );
    assert_eq!(
        agent_toml["project_registry_dir"].as_str(),
        Some(
            canonical_state
                .join("agent/project-registry")
                .to_string_lossy()
                .as_ref()
        )
    );
    let before = (agent, registration);

    let second = setup(&options).unwrap();
    assert_eq!(second.status, "already_configured");
    assert!(second.changed.is_empty());
    assert_eq!(
        fs::read_to_string(state.join("agent/runner.toml")).unwrap(),
        before.0
    );
    let project_file = fs::read_dir(state.join("agent/project-registry"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(fs::read_to_string(project_file).unwrap(), before.1);

    let output = render_setup_text(&second);
    for forbidden in [
        "client_id",
        "runtime project",
        "executor_ref",
        "workflow session",
        "agent:local",
        "wc_proj_",
        "token",
        "credentials/",
    ] {
        assert!(
            !output.to_ascii_lowercase().contains(forbidden),
            "default setup output leaked {forbidden}: {output}"
        );
    }
    assert!(output.contains("Next:\n  webcodex doctor"));
}

#[test]
fn setup_preserves_a_single_legacy_project_registry_layout() {
    let (_temp, root, state) = repo("legacy-registry");
    let legacy = state.join("agent/projects.d");
    fs::create_dir_all(&legacy).unwrap();
    let options = options(root, state.clone());

    setup(&options).unwrap();

    assert!(legacy.is_dir());
    assert!(!state.join("agent/project-registry").exists());
    let runner = fs::read_to_string(state.join("agent/runner.toml")).unwrap();
    let runner_toml: toml::Value = toml::from_str(&runner).unwrap();
    assert_eq!(
        runner_toml["project_registry_dir"].as_str(),
        Some(legacy.canonicalize().unwrap().to_string_lossy().as_ref())
    );
    assert_eq!(fs::read_dir(legacy).unwrap().count(), 1);
}

#[test]
fn setup_fails_closed_when_both_project_registry_layouts_exist() {
    let (_temp, root, state) = repo("ambiguous-registry");
    fs::create_dir_all(state.join("agent/project-registry")).unwrap();
    fs::create_dir_all(state.join("agent/projects.d")).unwrap();

    let error = setup(&options(root, state)).unwrap_err();
    assert_eq!(error.code, "project_registration_invalid");
    assert!(
        error
            .message
            .contains("both Runner project registry directories exist"),
        "{}",
        error.message
    );
}

#[test]
fn setup_repairs_only_missing_components_and_preserves_existing_config() {
    let (_temp, root, state) = repo("repair");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let agent_path = state.join("agent/runner.toml");
    let original_agent = fs::read_to_string(&agent_path).unwrap();
    let project_path = fs::read_dir(state.join("agent/project-registry"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(&project_path).unwrap();

    let report = setup(&options).unwrap();
    assert_eq!(report.changed, ["Project registration"]);
    assert_eq!(fs::read_to_string(agent_path).unwrap(), original_agent);
    assert!(project_path.is_file());
}

#[test]
fn setup_accepts_legacy_agent_toml_without_rewriting_it() {
    let (_temp, root, state) = repo("legacy-runner-config");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let runner_config = state.join("agent/runner.toml");
    let legacy_config = state.join("agent/agent.toml");
    let original = fs::read(&runner_config).unwrap();
    fs::rename(&runner_config, &legacy_config).unwrap();

    let report = setup(&options).unwrap();
    assert_eq!(report.status, "already_configured");
    assert!(report.changed.is_empty());
    assert_eq!(fs::read(&legacy_config).unwrap(), original);
    assert!(!runner_config.exists());
}

#[test]
fn setup_rejects_dual_runner_config_names_without_guessing() {
    let (_temp, root, state) = repo("dual-runner-config");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let runner_config = state.join("agent/runner.toml");
    let legacy_config = state.join("agent/agent.toml");
    fs::copy(&runner_config, &legacy_config).unwrap();

    let error = setup(&options).unwrap_err();
    assert_eq!(error.code, "project_registration_invalid");
    assert!(error.message.contains("runner.toml"));
    assert!(error.message.contains("agent.toml"));
    assert!(error.message.contains("legacy"));
    assert!(error.message.contains("remove or archive"));
}

#[test]
fn setup_accepts_legacy_projects_dir_without_rewriting_config() {
    let (_temp, root, state) = repo("legacy-projects-dir");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let runner_config = state.join("agent/runner.toml");
    let canonical = fs::read_to_string(&runner_config).unwrap();
    let legacy = canonical.replace("project_registry_dir", "projects_dir");
    assert_ne!(
        legacy, canonical,
        "fixture must contain project_registry_dir"
    );
    fs::write(&runner_config, &legacy).unwrap();

    let report = setup(&options).unwrap();
    assert_eq!(report.status, "already_configured");
    assert!(report.changed.is_empty());
    assert_eq!(fs::read_to_string(&runner_config).unwrap(), legacy);
}

#[test]
fn setup_accepts_legacy_filename_and_registry_field_together_without_rewriting() {
    let (_temp, root, state) = repo("legacy-runner-state");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let runner_config = state.join("agent/runner.toml");
    let legacy_config = state.join("agent/agent.toml");
    let canonical = fs::read_to_string(&runner_config).unwrap();
    let legacy = canonical.replace("project_registry_dir", "projects_dir");
    fs::write(&runner_config, &legacy).unwrap();
    fs::rename(&runner_config, &legacy_config).unwrap();

    let report = setup(&options).unwrap();
    assert_eq!(report.status, "already_configured");
    assert!(report.changed.is_empty());
    assert_eq!(fs::read_to_string(&legacy_config).unwrap(), legacy);
    assert!(!runner_config.exists());
}

#[test]
fn setup_rejects_both_runner_registry_fields_without_rewriting_config() {
    let (_temp, root, state) = repo("dual-runner-registry-fields");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let runner_config = state.join("agent/runner.toml");
    let canonical = fs::read_to_string(&runner_config).unwrap();
    let registry_line = canonical
        .lines()
        .find(|line| line.starts_with("project_registry_dir = "))
        .expect("generated Runner config must contain project_registry_dir");
    let legacy_line = registry_line.replacen("project_registry_dir", "projects_dir", 1);
    let dual = canonical.replacen(registry_line, &format!("{registry_line}\n{legacy_line}"), 1);
    fs::write(&runner_config, &dual).unwrap();

    let error = setup(&options).unwrap_err();
    assert_eq!(error.code, "project_registration_invalid");
    assert!(error.message.contains("project_registry_dir"));
    assert!(error.message.contains("projects_dir"));
    assert_eq!(fs::read_to_string(&runner_config).unwrap(), dual);
}

#[test]
fn setup_conflict_and_project_root_collision_fail_closed() {
    let (temp, root, state) = repo("first");
    let first = options(root, state.clone());
    setup(&first).unwrap();
    let agent_path = state.join("agent/runner.toml");
    let before = fs::read_to_string(&agent_path).unwrap();
    let mut conflicting = before.replace("server_url = ", "server_url = \"http://invalid\" # ");
    if conflicting == before {
        conflicting.push_str("\nserver_url = \"http://invalid\"\n");
    }
    fs::write(&agent_path, conflicting).unwrap();

    let error = setup(&first).unwrap_err();
    assert_eq!(error.code, "project_registration_invalid");
    assert!(error.message.contains("server_url"));

    let second_root = temp.path().join("second");
    fs::create_dir(&second_root).unwrap();
    git(&["init", "-q"], &second_root);
    let collision = options(second_root, state);
    let error = setup(&collision).unwrap_err();
    assert_eq!(error.code, "project_registration_invalid");
    assert!(error.message.contains("project root"));
}

#[test]
fn setup_client_ids_include_project_grant_identity() {
    let (temp, root, _) = repo("grant-scoped-client");
    let first = options(root.clone(), temp.path().join("state-a"));
    let second = options(root, temp.path().join("state-b"));
    let (first_config, first_paths) = ProjectConfig::resolve(&first).unwrap();
    let (second_config, second_paths) = ProjectConfig::resolve(&second).unwrap();

    assert_ne!(
        first_config.project_grant_id(&first_paths),
        second_config.project_grant_id(&second_paths)
    );
    assert_ne!(
        first_config.executor_client_id,
        second_config.executor_client_id
    );
}

#[test]
fn setup_selects_stable_fallback_port_and_persists_it() {
    let (_temp, root, state) = repo("port-collision");
    let options = options(root, state.clone());
    let (expected, _) = ProjectConfig::resolve(&options).unwrap();
    let occupied = std::net::TcpListener::bind(("127.0.0.1", expected.port)).unwrap();

    setup(&options).unwrap();
    let persisted: ProjectConfig =
        toml::from_str(&fs::read_to_string(state.join("project.toml")).unwrap()).unwrap();
    assert_ne!(persisted.port, expected.port);
    assert_eq!(
        persisted.port,
        20_000 + ((u32::from(expected.port - 20_000) + 7_919) % 20_000) as u16
    );
    drop(occupied);

    let second = setup(&options).unwrap();
    assert_eq!(second.status, "already_configured");
    let again: ProjectConfig =
        toml::from_str(&fs::read_to_string(state.join("project.toml")).unwrap()).unwrap();
    assert_eq!(again.port, persisted.port);
}

#[test]
fn setup_does_not_replace_persisted_port_when_temporarily_occupied() {
    let (_temp, root, state) = repo("stable-port");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let config_path = state.join("project.toml");
    let before = fs::read(&config_path).unwrap();
    let config: ProjectConfig = toml::from_str(std::str::from_utf8(&before).unwrap()).unwrap();
    let occupied = std::net::TcpListener::bind(("127.0.0.1", config.port)).unwrap();

    let report = setup(&options).unwrap();

    assert_eq!(report.status, "already_configured");
    assert_eq!(fs::read(config_path).unwrap(), before);
    drop(occupied);
}

#[test]
fn doctor_and_status_share_canonical_readiness_facts_and_stay_read_only() {
    let (_temp, root, state) = repo("readiness");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let runner_config = state.join("agent/runner.toml");
    let before = fs::read(&runner_config).unwrap();

    let cases = [
        (RemoteProbe::Ready, true, "ready"),
        (RemoteProbe::Unreachable, false, "server_unreachable"),
        (
            RemoteProbe::CredentialRejected,
            false,
            "project_credential_rejected",
        ),
        (RemoteProbe::RunnerOffline, false, "agent_offline"),
        (
            RemoteProbe::ProjectMissing,
            false,
            "project_registration_invalid",
        ),
    ];
    for (probe, expected_ready, expected_code) in cases {
        let readiness = readiness_with_probe(&options, probe);
        assert_eq!(readiness.ready, expected_ready);
        assert!(readiness
            .findings
            .iter()
            .any(|finding| finding.code == expected_code));
        let status = render_status_text(&readiness);
        let doctor = render_doctor_text(&readiness);
        assert_eq!(
            status.contains("Coding access: ready"),
            expected_ready,
            "{status}"
        );
        assert!(doctor.contains(&fact(&readiness, expected_code).summary));
        for output in [status, doctor] {
            assert!(!output.contains("agent:"));
            assert!(!output.contains("client_id"));
            assert!(!output.contains("wc_proj_"));
        }
    }
    assert_eq!(fs::read(runner_config).unwrap(), before);
}
#[test]
fn doctor_reports_not_setup_and_invalid_workspace_with_stable_actions() {
    let (_temp, root, state) = repo("invalid");
    let options = options(root.clone(), state);
    let missing = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(missing.connection, "not configured");
    let finding = fact(&missing, "project_not_configured");
    assert_eq!(finding.status, ReadinessStatus::Fail);
    assert_eq!(finding.next_action.as_deref(), Some("webcodex setup"));

    setup(&options).unwrap();
    fs::remove_dir_all(root).unwrap();
    let invalid = readiness_with_probe(&options, RemoteProbe::Ready);
    assert_eq!(
        fact(&invalid, "workspace_unavailable").status,
        ReadinessStatus::Fail
    );
}

#[test]
fn doctor_reports_malformed_registration() {
    let (_temp, root, state) = repo("malformed-registration");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let project_path = fs::read_dir(state.join("agent/project-registry"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&project_path, "this is not = valid TOML [").unwrap();
    let before = fs::read(&project_path).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_registration_invalid").status,
        ReadinessStatus::Fail
    );
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "project_not_configured"));
    assert_eq!(fs::read(project_path).unwrap(), before);
}

#[test]
fn doctor_reports_conflicting_registration() {
    let (temp, root, state) = repo("conflicting-registration");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let project_path = fs::read_dir(state.join("agent/project-registry"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let before = fs::read_to_string(&project_path).unwrap();
    let different_root = temp.path().join("different-project");
    fs::create_dir(&different_root).unwrap();
    let different_root = different_root.canonicalize().unwrap();
    let mut registration: toml::Value = toml::from_str(&before).unwrap();
    registration["path"] = toml::Value::String(different_root.to_string_lossy().into_owned());
    let conflicting = toml::to_string(&registration).unwrap();
    assert_ne!(conflicting, before);
    fs::write(&project_path, &conflicting).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_registration_invalid").status,
        ReadinessStatus::Fail
    );
    assert_eq!(fs::read_to_string(project_path).unwrap(), conflicting);
}

#[test]
fn doctor_reports_missing_private_credential() {
    let (_temp, root, state) = repo("missing-credential");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    fs::remove_file(state.join("credentials/project-credential")).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_credential_invalid").status,
        ReadinessStatus::Fail
    );
}

#[test]
fn doctor_reports_unreadable_private_credential() {
    let (_temp, root, state) = repo("unreadable-credential");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let credential = state.join("credentials/project-credential");
    fs::remove_file(&credential).unwrap();
    fs::create_dir(&credential).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(
        fact(&readiness, "project_credential_invalid").status,
        ReadinessStatus::Fail
    );
}

#[test]
fn status_does_not_turn_invalid_config_into_not_configured() {
    let (_temp, root, state) = repo("malformed-config");
    let options = options(root, state.clone());
    setup(&options).unwrap();
    let config_path = state.join("project.toml");
    fs::write(&config_path, "version = [broken").unwrap();
    let before = fs::read(&config_path).unwrap();

    let readiness = readiness_with_probe(&options, RemoteProbe::Unreachable);
    assert_eq!(readiness.connection, "invalid");
    assert!(readiness
        .findings
        .iter()
        .any(|finding| finding.code == "project_registration_invalid"));
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "project_not_configured"));
    assert_eq!(fs::read(config_path).unwrap(), before);
}

#[tokio::test]
async fn doctor_reports_rejected_project_credential_without_agent_offline() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (_temp, root, state) = repo("rejected-credential");
    let options = options(root, state);
    setup(&options).unwrap();
    let (config, _) = ProjectConfig::resolve(&options).unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", config.port))
        .await
        .unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        stream
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: 24\r\nconnection: close\r\n\r\n{\"error\":\"Unauthorized\"}",
            )
            .await
            .unwrap();
    });

    let readiness = collect_readiness(&options).await;
    server.await.unwrap();
    assert_eq!(readiness.connection, "connected");
    assert_eq!(
        fact(&readiness, "project_credential_rejected").status,
        ReadinessStatus::Fail
    );
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "server_unreachable"));
    assert!(!readiness
        .findings
        .iter()
        .any(|finding| finding.code == "agent_offline"));
}

#[test]
fn gitignore_hygiene_fact_grades_setup_before_the_first_task() {
    let (_temp, root, _state) = repo("hygiene");
    // The fixture repo has a clean status but no .gitignore: warn about the
    // gap before build artifacts poison workspace provenance.
    let missing = gitignore_hygiene_fact(&root);
    assert_eq!(missing.status, ReadinessStatus::Warn);
    assert_eq!(missing.code, "gitignore_missing");

    // Untracked build artifacts outrank the missing file: they are the exact
    // provenance poison, and the summary names them.
    fs::create_dir(root.join("target")).unwrap();
    fs::write(root.join("target/junk.o"), "x").unwrap();
    let poisoned = gitignore_hygiene_fact(&root);
    assert_eq!(poisoned.status, ReadinessStatus::Warn);
    assert_eq!(poisoned.code, "untracked_build_artifacts");
    assert!(poisoned.summary.contains("target/"), "{}", poisoned.summary);

    // A .gitignore covering the artifacts settles both complaints.
    fs::write(root.join(".gitignore"), "target/\n").unwrap();
    git(&["add", ".gitignore"], &root);
    let clean = gitignore_hygiene_fact(&root);
    assert_eq!(clean.status, ReadinessStatus::Pass);
    assert_eq!(clean.code, "gitignore_present");
}
