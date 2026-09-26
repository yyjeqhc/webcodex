use super::*;

fn input(args: &[&str]) -> Result<Input, String> {
    parse(
        &args
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>(),
    )
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
