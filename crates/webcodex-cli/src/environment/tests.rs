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
fn legacy_server_network_inputs_are_explicit_and_scoped() {
    let legacy = input(&[
        "migrate-legacy-server",
        "--user",
        "alice",
        "--listen",
        "0.0.0.0:8080",
        "--server-url",
        "http://127.0.0.1:8080",
        "--token-file",
        "private-token",
    ])
    .unwrap();
    assert_eq!(legacy.listen.as_deref(), Some("0.0.0.0:8080"));
    assert_eq!(legacy.username.as_deref(), Some("alice"));
    assert!(input(&["configure", "--create", "--listen", "0.0.0.0:8080"]).is_err());
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
