use super::*;

#[test]
fn service_scope_is_only_a_new_setup_choice_not_a_control_retarget() {
    let selected = input(&["configure", "--create", "--runner", "--scope", "user"]).unwrap();
    assert_eq!(selected.scope, Some(service::ServiceScope::User));
    assert_eq!(
        input(&["configure", "--create", "--runner", "--scope", "system"])
            .unwrap()
            .scope,
        Some(service::ServiceScope::System)
    );
    for args in [
        vec!["configure", "--scope", "auto"],
        vec!["resume", "--scope", "user"],
        vec!["start", "server", "--scope", "user"],
        vec!["configure", "--scope", "user", "--scope", "system"],
    ] {
        assert!(input(&args).is_err());
    }
    assert!(!selected.code_stdin && selected.token_file.is_none());
}

fn input(args: &[&str]) -> Result<Input, String> {
    parse(
        &args
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>(),
    )
}

#[tokio::test]
async fn update_command_dispatches_to_the_terminal_adapter() {
    let args = ["update", "--help"].map(str::to_owned);
    let output = run(&args).await.unwrap();
    assert!(output.starts_with("webcodex environment update <COMMAND>"));
}

#[test]
fn business_choices_and_resume_are_unambiguous() {
    let create = input(&["configure", "--create", "--no-project"]).unwrap();
    assert!(create.create && create.no_project);
    let viewer = input(&[
        "configure",
        "--join",
        "https://server.example",
        "--no-project",
        "--token-file",
        "credential",
    ])
    .unwrap();
    assert!(viewer.no_project && viewer.token_file.is_some() && !viewer.code_stdin);
    let runner = input(&[
        "configure",
        "--join",
        "https://server.example",
        "--project",
        "project",
        "--code-stdin",
    ])
    .unwrap();
    assert!(runner.project.is_some() && runner.code_stdin && runner.token_file.is_none());
    let resumed = input(&["resume", "--code-stdin", "--new-pairing-code"]).unwrap();
    assert!(resumed.new_code && resumed.code_stdin);
    for args in [
        vec!["configure", "--create", "--join", "https://server.example"],
        vec!["configure", "--project", "a", "--no-project"],
        vec!["configure", "--join", "a", "--join", "b"],
        vec!["configure", "--token-file", "secret", "--code-stdin"],
        vec!["configure", "--development-build"],
    ] {
        assert!(input(&args).is_err());
    }
}

#[test]
fn projectless_runner_is_explicit_and_does_not_change_legacy_viewer_defaults() {
    let local = input(&["configure", "--create", "--runner"]).unwrap();
    assert!(local.runner && local.project.is_none());
    let remote = input(&[
        "configure",
        "--join",
        "https://server.example",
        "--runner",
        "--code-stdin",
    ])
    .unwrap();
    assert!(remote.runner && remote.project.is_none() && remote.code_stdin);
    let named = input(&[
        "configure",
        "--create",
        "--runner",
        "--runner-name",
        "My laptop",
    ])
    .unwrap();
    assert_eq!(named.runner_name.as_deref(), Some("My laptop"));
    let named_project = input(&[
        "configure",
        "--join",
        "https://server.example",
        "--project",
        "project",
        "--runner-name",
        "Build runner",
    ])
    .unwrap();
    assert_eq!(named_project.runner_name.as_deref(), Some("Build runner"));
    assert!(input(&["configure", "--create", "--runner-name", "orphan"]).is_err());
    assert!(input(&["status", "--runner-name", "orphan"]).is_err());
    assert!(
        !input(&["configure", "--create", "--no-project"])
            .unwrap()
            .runner
    );
    for command in ["status", "resume", "doctor", "start"] {
        assert!(input(&[command, "--runner"]).is_err());
    }
}

#[tokio::test]
async fn json_argument_errors_are_structured_and_do_not_echo_inputs() {
    let args = vec!["configure", "--api-key=private-fixture-value", "--json"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let error = run(&args).await.unwrap_err();
    let value: serde_json::Value = serde_json::from_str(&error).unwrap();
    assert_eq!(value["ok"], false);
    assert!(!error.contains("private-fixture-value"));
}

#[test]
fn tunnel_and_upgrade_inputs_never_take_literal_credentials() {
    let tunnel = input(&[
        "configure-tunnel",
        "work",
        "--credentials-file",
        "private.json",
    ])
    .unwrap();
    assert_eq!(tunnel.operand.as_deref(), Some("work"));
    let embedded = input(&[
        "configure-tunnel",
        "work",
        "--host",
        "embedded",
        "--credentials-file",
        "private.json",
    ])
    .unwrap();
    assert_eq!(embedded.tunnel_host, Some(TunnelHostMode::Embedded));
    assert_eq!(
        configure_tunnel_host_mode(None, Some(TunnelHostMode::Embedded)),
        TunnelHostMode::Embedded,
        "reconfiguring an existing profile must preserve its owner when --host is omitted"
    );
    assert_eq!(
        configure_tunnel_host_mode(None, None),
        TunnelHostMode::Standalone,
        "new profiles keep the historical standalone default"
    );
    assert!(input(&["status", "--host", "embedded"]).is_err());
    assert!(input(&["configure-tunnel", "--host", "other"]).is_err());
    assert!(input(&["configure-tunnel", "--api-key", "secret"]).is_err());
    assert!(input(&["status", "--credentials-file", "private.json"]).is_err());
    let upgrade = input(&[
        "upgrade-preflight",
        "--candidate-dir",
        "candidate",
        "--development-build",
    ])
    .unwrap();
    assert!(upgrade.development_build);
}

#[test]
fn explicit_runtime_directory_never_persists_the_temporary_invoking_cli() {
    let root = std::env::temp_dir().canonicalize().unwrap();
    let binaries = discover_binaries(Some(&root)).unwrap();
    for (path, name) in [
        (&binaries.cli, "webcodex"),
        (&binaries.server, "webcodex-server"),
        (&binaries.runner, "webcodex-runner"),
    ] {
        assert_eq!(
            *path,
            root.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        );
    }
}

#[cfg(unix)]
#[test]
fn tunnel_credentials_require_owner_private_file_and_redact_parse_failure() {
    use std::os::unix::fs::PermissionsExt;
    let root =
        std::env::temp_dir().join(format!("webcodex-cli-credentials-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.join("credential.json");
    std::fs::write(
        &path,
        r#"{"tunnel_id":"fixture","api_key":"private-fixture-value"}"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let credential = read_tunnel_credentials(&path).unwrap();
    assert_eq!(credential.api_key.expose(), "private-fixture-value");
    assert!(!format!("{credential:?}").contains("private-fixture-value"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_tunnel_credentials(&path).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(
        &path,
        r#"{"tunnel_id":"fixture","api_key":"private-fixture-value","unknown":0}"#,
    )
    .unwrap();
    assert!(!read_tunnel_credentials(&path)
        .unwrap_err()
        .contains("private-fixture-value"));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn installer_finalization_cannot_override_authorized_store() {
    let arguments = [
        "installer-finish",
        "--environment-dir",
        "/different-user/environment",
        "--json",
    ]
    .map(str::to_owned);
    let error = run(&arguments).await.unwrap_err();
    assert!(error.contains("fixed by the owner authorization"));
    assert!(!error.contains("/different-user"));
    let child = [
        "__installer-child",
        "4",
        "finish",
        "/different-user/environment",
    ]
    .map(str::to_owned);
    assert_eq!(
        run(&child).await.unwrap_err(),
        "Invalid internal installer request"
    );
}

#[test]
fn guarded_installer_options_never_fall_back_to_unbound_commands() {
    let target = [
        "--environment-dir",
        "/selected",
        "--upgrade-target-file",
        "/private/request.json",
    ];
    let mut prepare = vec!["upgrade-prepare", "--candidate-dir", "/candidate"];
    prepare.extend(target);
    prepare.push("--operation-id-output");
    assert!(input(&prepare).is_ok());
    for bad in ["--json", "--development-build"] {
        let mut arguments = prepare.clone();
        arguments.push(bad);
        assert!(input(&arguments).is_err());
    }
    for command in ["upgrade-finish", "upgrade-rollback", "installer-verify"] {
        let mut arguments = vec![command];
        arguments.extend(target);
        assert!(input(&arguments).is_err());
        arguments.extend(["--operation-id", "e6a04341-9bf3-4021-9c19-7df4742b6799"]);
        assert!(input(&arguments).is_ok());
        arguments.push("--operation-id-output");
        assert!(input(&arguments).is_err());
    }
    for command in ["configure", "package-upgrade-prepare", "upgrade-preflight"] {
        let mut arguments = vec![command];
        arguments.extend(target);
        assert!(input(&arguments).is_err());
    }
    assert!(input(&[
        "upgrade-finish",
        "--operation-id",
        "e6a04341-9bf3-4021-9c19-7df4742b6799"
    ])
    .is_err());
    assert!(input(&[
        "upgrade-prepare",
        "--upgrade-target-file",
        "/private/request.json",
        "--operation-id-output"
    ])
    .is_err());
}

#[tokio::test]
async fn guarded_handoff_rejects_missing_owner_and_unacknowledged_followups_without_creation() {
    use webcodex_environment::unified_update::PrivateUpdateCache;
    let directory = tempfile::tempdir().unwrap();
    let nonce = "a".repeat(32);
    let cache =
        PrivateUpdateCache::open(directory.path().join(format!("handoff-{nonce}"))).unwrap();
    let envelope = serde_json::json!({"schema_version":1,"launch_nonce":nonce,"target":{
        "environment_id":"selected","manifest_sha256":"b".repeat(64),"operation_id":null}});
    cache
        .write("request.json", &serde_json::to_vec(&envelope).unwrap())
        .unwrap();
    let root = directory.path().join("absent-environment");
    let request = cache.file("request.json").unwrap();
    let mut prepare = vec![
        "upgrade-prepare".to_owned(),
        "--environment-dir".into(),
        root.to_string_lossy().into(),
        "--upgrade-target-file".into(),
        request.to_string_lossy().into(),
        "--candidate-dir".into(),
        directory
            .path()
            .join("absent-candidate")
            .to_string_lossy()
            .into(),
        "--operation-id-output".into(),
    ];
    assert_eq!(
        run(&prepare).await.unwrap_err(),
        "Selected Environment is no longer available"
    );
    assert!(!root.exists());
    prepare[0] = "upgrade-finish".into();
    prepare.truncate(5);
    prepare.extend([
        "--operation-id".into(),
        "e6a04341-9bf3-4021-9c19-7df4742b6799".into(),
    ]);
    assert_eq!(
        run(&prepare).await.unwrap_err(),
        "Guarded installer follow-up does not match the acknowledged operation"
    );
    assert!(!root.exists());
}

#[tokio::test]
async fn same_package_verification_rejects_an_invalid_installer_target_before_candidate_access() {
    let temp = tempfile::tempdir().unwrap();
    let candidate = temp.path().join("absent-candidate");
    let runtime = temp.path().join("absent-runtime");
    let args = vec![
        "installer-verify-same".to_owned(),
        "--candidate-dir".into(),
        candidate.display().to_string(),
        "--expected-runtime-dir".into(),
        runtime.display().to_string(),
        "--installer-target".into(),
        "invalid-target".into(),
    ];
    assert_eq!(
        run(&args).await.unwrap_err(),
        "Invalid installer package target"
    );
    assert!(!candidate.exists());
    assert!(!runtime.exists());
}

#[tokio::test]
async fn path_commands_are_read_only_and_manifest_is_explicitly_metadata_only() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("absent-environment");
    for command in ["paths", "backup-manifest"] {
        let args = vec![
            command.to_string(),
            "--environment-dir".into(),
            directory.display().to_string(),
            "--json".into(),
        ];
        let result = run(&args).await.unwrap();
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        if command == "paths" {
            assert_eq!(json["roots"][0]["status"], "missing");
        } else {
            assert_eq!(json["kind"], "manifest_only");
            assert_eq!(json["cannot_restore"], true);
        }
        assert!(!directory.exists());
    }
    let args = vec![
        "paths".into(),
        "--environment-dir".into(),
        directory.display().to_string(),
        "--token-file".into(),
        "private-unreadable-input".into(),
    ];
    assert!(run(&args).await.is_err());
    assert!(!directory.exists());
}

#[test]
fn runtime_runner_join_uses_existing_setup_without_any_server_steps() {
    let options = input(&[
        "configure",
        "--join",
        "https://main.example/",
        "--runner",
        "--no-project",
        "--code-stdin",
    ])
    .unwrap();
    let request = configure_request(
        &options,
        service::ServiceScope::User,
        None,
        LocalAccount {
            name: "owner".into(),
            identity: "1000".into(),
            home: "/home/owner".into(),
        },
        RuntimeBinaries {
            cli: "/usr/lib/webcodex/webcodex-runtime/webcodex".into(),
            server: "/usr/lib/webcodex/webcodex-runtime/webcodex-server".into(),
            runner: "/usr/lib/webcodex/webcodex-runtime/webcodex-runner".into(),
        },
    )
    .unwrap();
    assert_eq!(request.mode, EnvironmentMode::Join);
    assert_eq!(request.server_url, "https://main.example");
    assert!(request.local_runner());
    assert!(!request.local_server());
    assert!(request.project.is_none());
    assert!(request.steps().contains(&SetupStep::RunnerEnrollment));
    assert!(request.steps().contains(&SetupStep::RunnerServiceStart));
    for excluded in [
        SetupStep::ServerConfiguration,
        SetupStep::ServerServiceInstall,
        SetupStep::ServerServiceStart,
        SetupStep::ProjectRegistration,
    ] {
        assert!(!request.steps().contains(&excluded));
    }
    let encoded = serde_json::to_string(&request).unwrap();
    assert!(!encoded.contains("pairing_code"));
    assert!(!encoded.contains("api_key"));
}

#[test]
fn runner_join_does_not_accept_literal_pairing_or_tunnel_credentials() {
    for args in [
        vec![
            "configure",
            "--join",
            "https://main.example",
            "--runner",
            "--pairing-code",
            "private-canary",
        ],
        vec![
            "configure",
            "--join",
            "https://main.example",
            "--runner",
            "--api-key",
            "private-canary",
        ],
        vec![
            "configure",
            "--join",
            "https://main.example",
            "--runner",
            "--tunnel-id",
            "private-canary",
        ],
    ] {
        assert!(input(&args).is_err());
    }
}

#[tokio::test]
async fn invalid_package_target_is_rejected_before_authorization_and_does_not_echo_it() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("absent");
    let args = vec![
        "installer-authorize".into(),
        "--candidate-dir".into(),
        root.to_string_lossy().into_owned(),
        "--upgrade-receipt".into(),
        root.join("receipt.json").to_string_lossy().into_owned(),
        "--installer-target".into(),
        "secret-canary".into(),
        "--json".into(),
    ];
    let output = run(&args).await.unwrap_err();
    assert!(!output.contains("secret-canary"));
    assert!(!root.exists());
}

// Run the public adapter in an isolated child so cache/config overrides never
// mutate process-global state or inherit the invoking terminal.
fn public_environment_fixture(test_name: &str) -> Option<PathBuf> {
    const FIXTURE_ROOT: &str = "WEBCODEX_PUBLIC_ENVIRONMENT_TEST_ROOT";
    if let Some(root) = std::env::var_os(FIXTURE_ROOT) {
        return Some(root.into());
    }
    let directory = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", test_name, "--nocapture"])
        .stdin(std::process::Stdio::null())
        .env(FIXTURE_ROOT, directory.path())
        .env(
            "WEBCODEX_DESKTOP_DATA_DIR",
            directory.path().join("absent-update-cache"),
        );
    for (name, leaf) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        child.env(name, directory.path().join(leaf));
    }
    let output = child.output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
    assert!(
        output.status.success(),
        "isolated public entry fixture failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    None
}

#[tokio::test]
async fn public_environment_entry_help_has_newlines_and_delegates_update_help() {
    let Some(root) = public_environment_fixture(
        "environment::tests::public_environment_entry_help_has_newlines_and_delegates_update_help",
    ) else {
        return;
    };
    let help = run(&["--help".into()]).await.unwrap();
    assert!(help.starts_with("webcodex environment <COMMAND>\n\n"));
    assert!(!help.contains("\\n"));
    let update_help = run(&["update".into(), "--help".into()]).await.unwrap();
    assert!(update_help.starts_with("webcodex environment update <COMMAND>\n\n"));
    assert!(update_help.contains("apply --version VERSION --yes"));
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}

#[tokio::test]
async fn public_environment_entry_update_status_is_bounded_and_read_only_when_absent() {
    let Some(root) = public_environment_fixture(
        "environment::tests::public_environment_entry_update_status_is_bounded_and_read_only_when_absent",
    ) else {
        return;
    };
    let environment = root.join("absent-environment");
    let output = run(&[
        "update".into(),
        "status".into(),
        "--json".into(),
        "--environment-dir".into(),
        environment.to_string_lossy().into_owned(),
    ])
    .await
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["ok"], true);
    assert!(value["environment_id"].is_null());
    assert_eq!(value["headless_apply_supported"], false);
    assert!(output.len() <= unified_update::MAX_UPDATE_VIEW_BYTES);
    assert!(!output.contains(root.to_str().unwrap()));
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}

#[tokio::test]
async fn public_environment_entry_update_effects_reject_non_tty_before_store_access() {
    let Some(root) = public_environment_fixture(
        "environment::tests::public_environment_entry_update_effects_reject_non_tty_before_store_access",
    ) else {
        return;
    };
    for (command, option, target) in [
        ("apply", "--version", "1.2.3"),
        (
            "resume",
            "--operation-id",
            "11111111-1111-4111-8111-111111111111",
        ),
        (
            "rollback",
            "--operation-id",
            "11111111-1111-4111-8111-111111111111",
        ),
    ] {
        let output = run(&[
            "update".into(),
            command.into(),
            option.into(),
            target.into(),
            "--yes".into(),
            "--json".into(),
            "--environment-dir".into(),
            root.join("absent-environment")
                .to_string_lossy()
                .into_owned(),
        ])
        .await
        .unwrap_err();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], false);
        assert_eq!(
            value["error_kind"],
            if cfg!(target_os = "linux") {
                "interactive_terminal_required"
            } else {
                "headless_platform_not_supported"
            }
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    }
}

#[test]
fn runner_name_uses_shared_limits_and_requires_an_explicit_runner_setup() {
    let name = "机".repeat(200);
    assert_eq!(
        input(&["configure", "--create", "--runner", "--runner-name", &name])
            .unwrap()
            .runner_name,
        Some(name.clone())
    );
    assert!(input(&[
        "configure",
        "--join",
        "https://main.example",
        "--project",
        "project",
        "--runner-name",
        "Work machine"
    ])
    .is_ok());
    for args in [
        vec![
            "configure",
            "--create",
            "--no-project",
            "--runner-name",
            "Work machine",
        ],
        vec!["resume", "--runner-name", "Work machine"],
        vec!["status", "--runner-name", "Work machine"],
        vec!["configure", "--runner", "--runner-name"],
        vec![
            "configure",
            "--runner",
            "--runner-name",
            "a",
            "--runner-name",
            "b",
        ],
        vec![
            "configure",
            "--runner",
            "--runner-name",
            "private-name\0value",
        ],
    ] {
        let error = input(&args).err().expect("must reject invalid name input");
        assert!(!error.contains("private-name"));
    }
    assert!(input(&[
        "configure",
        "--runner",
        "--runner-name",
        &format!("{name}机")
    ])
    .is_err());
}

#[test]
fn named_projectless_join_forwards_label_without_creating_a_server_or_changing_identity() {
    let options = input(&[
        "configure",
        "--join",
        "https://main.example/",
        "--runner",
        "--no-project",
        "--runner-name",
        "SSH worker",
        "--code-stdin",
    ])
    .unwrap();
    let request = configure_request(
        &options,
        service::ServiceScope::User,
        None,
        LocalAccount {
            name: "owner".into(),
            identity: "1000".into(),
            home: "/home/owner".into(),
        },
        RuntimeBinaries {
            cli: "/runtime/webcodex".into(),
            server: "/runtime/webcodex-server".into(),
            runner: "/runtime/webcodex-runner".into(),
        },
    )
    .unwrap();
    assert_eq!(request.runner_display_name.as_deref(), Some("SSH worker"));
    assert!(request.local_runner());
    assert!(!request.local_server());
    assert!(request.project.is_none());
    assert!(!request.steps().contains(&SetupStep::ServerConfiguration));
    assert!(!request.steps().contains(&SetupStep::ProjectRegistration));
    let unnamed = configure_request(
        &Input {
            runner_name: None,
            ..options
        },
        request.service_scope,
        None,
        request.account,
        request.binaries,
    )
    .unwrap();
    assert!(unnamed.runner_display_name.is_none());
    assert_eq!(unnamed.mode, EnvironmentMode::Join);
}

#[test]
fn tunnel_diagnosis_requires_an_explicit_environment_and_profile() {
    assert!(input(&["tunnel-diagnose"]).is_err());
    assert!(input(&["tunnel-diagnose", "primary"]).is_err());
    assert!(input(&[
        "tunnel-diagnose",
        "primary",
        "--environment-dir",
        "/fixture",
        "--json"
    ])
    .is_ok());
}

#[test]
fn recovery_is_diagnostic_by_default_and_apply_requires_exact_authority() {
    let plan = input(&["recover-tunnel", "primary", "--environment-dir", "/fixture"]).unwrap();
    assert!(!plan.apply_recovery && !plan.accept_uncertain_effects);
    assert!(input(&[
        "recover-tunnel",
        "primary",
        "--environment-dir",
        "/fixture",
        "--apply"
    ])
    .is_err());
    assert!(input(&["status", "--apply"]).is_err());
    assert!(input(&[
        "recover-tunnel",
        "primary",
        "--environment-dir",
        "/fixture",
        "--apply",
        "--accept-uncertain-effects",
        "--expected-environment-id",
        "environment",
        "--expected-revision",
        "1",
        "--expected-run-id",
        "12345678-1234-1234-1234-123456789abc"
    ])
    .is_ok());
}
