use super::super::support::*;

#[test]
fn server_tunnel_parser_is_machine_owned_and_openai_only() {
    let parsed = parse_server_tunnel(&args(&[
        "--provider",
        "openai",
        "--env-file",
        "local.env",
        "--json",
        "--stop-on-stdin-eof",
    ]))
    .unwrap();
    assert_eq!(parsed.env_file, PathBuf::from("local.env"));
    assert!(parsed.stop_on_stdin_eof);

    let persistent = parse_server_tunnel(&args(&[
        "--provider",
        "openai",
        "--env-file",
        "local.env",
        "--json",
    ]))
    .unwrap();
    assert!(!persistent.stop_on_stdin_eof);

    assert!(parse_server_tunnel(&args(&[
        "--provider",
        "cloudflare",
        "--env-file",
        "local.env",
        "--json",
        "--stop-on-stdin-eof",
    ]))
    .unwrap_err()
    .contains("openai"));
    assert!(
        parse_server_tunnel(&args(&["--provider", "openai", "--env-file", "local.env",])).is_err()
    );
}

#[test]
fn regular_tunnel_bootstrap_token_follows_server_env_precedence() {
    let _guard = env_test_guard();
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    std::fs::write(&env_file, "WEBCODEX_TOKEN=file-bootstrap\n").unwrap();

    let _env = EnvGuard::new().remove("WEBCODEX_TOKEN");
    assert_eq!(
        crate::webcodex_cli::server::derive_regular_tunnel_bootstrap_token(&env_file).unwrap(),
        "file-bootstrap"
    );
    drop(_env);

    let _env = EnvGuard::new().set("WEBCODEX_TOKEN", "process-bootstrap");
    assert_eq!(
        crate::webcodex_cli::server::derive_regular_tunnel_bootstrap_token(&env_file).unwrap(),
        "process-bootstrap"
    );
}

#[test]
fn regular_tunnel_server_url_is_derived_from_loopback_env_only() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    std::fs::write(&env_file, "WEBCODEX_ADDR=0.0.0.0:18080\n").unwrap();
    assert_eq!(
        crate::webcodex_cli::server::derive_regular_tunnel_server_url(&env_file).unwrap(),
        "http://127.0.0.1:18080"
    );

    std::fs::write(&env_file, "WEBCODEX_ADDR=192.0.2.10:18080\n").unwrap();
    assert!(crate::webcodex_cli::server::derive_regular_tunnel_server_url(&env_file).is_err());
}

#[test]
fn cloudflare_server_tunnel_uses_only_a_private_runtime_binding() {
    for provider in ["cloudflare_named", "cloudflare_quick"] {
        let parsed = parse_server_tunnel(&args(&[
            "--provider",
            provider,
            "--runtime-binding",
            "private/runtime.json",
            "--json",
        ]))
        .unwrap();
        assert_eq!(
            parsed.runtime_binding,
            Some(PathBuf::from("private/runtime.json"))
        );
        assert!(parsed.env_file.as_os_str().is_empty());
        assert!(parse_server_tunnel(&args(&[
            "--provider",
            provider,
            "--env-file",
            "server.env",
            "--json"
        ]))
        .is_err());
        assert!(parse_server_tunnel(&args(&[
            "--provider",
            provider,
            "--runtime-binding",
            "runtime.json",
            "--env-file",
            "server.env",
            "--json"
        ]))
        .is_err());
    }
    assert!(parse_server_tunnel(&args(&[
        "--provider",
        "openai",
        "--runtime-binding",
        "runtime.json",
        "--json"
    ]))
    .is_err());
    let error = parse_server_tunnel(&args(&[
        "--provider",
        "cloudflare_named",
        "--token",
        "private-token-fixture",
        "--json",
    ]))
    .unwrap_err();
    assert!(!error.contains("private-token-fixture"));
    assert!(parse_server_tunnel(&args(&[
        "--provider",
        "openai",
        "--provider",
        "cloudflare_quick",
        "--runtime-binding",
        "runtime.json",
        "--json"
    ]))
    .is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn machine_tunnel_rejects_provider_mismatch_before_contacting_server() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let directory = tmp.path().join("quick");
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.join("runtime.json");
    std::fs::write(&path,serde_json::json!({"profile_id":"quick","provider":{"kind":"cloudflare_quick"},"host_mode":"standalone","autostart":true,"revision":1,"runtime_revision":1,"owner_username":"owner","runner_client_id":"client-one","ingress_port":34567,"token_ref":null,"local_server_url":"http://127.0.0.1:1","bootstrap_token":"private-bootstrap"}).to_string()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let parsed = parse_server_tunnel(&args(&[
        "--provider",
        "cloudflare_named",
        "--runtime-binding",
        path.to_str().unwrap(),
        "--json",
    ]))
    .unwrap();
    let error =
        crate::webcodex_cli::server::run_server_tunnel_with_stop(parsed, std::future::pending())
            .await
            .unwrap_err();
    assert!(error.contains("does not match"));
    assert!(!error.contains("private-bootstrap"));
}
