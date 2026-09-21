use super::super::support::*;

#[test]
fn server_run_env_file_is_passed_only_through_child_environment() {
    let _guard = env_test_guard();
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp
        .path()
        .join(format!("webpi-server{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&bin, b"").unwrap();
    let env_file = tmp.path().join("foreground server.env");
    let _env = EnvGuard::new().set_os("PATH", tmp.path().as_os_str().to_os_string());

    // Inject only the already-resolved fixture binary; native acceptance tests
    // separately prove that production discovery never falls back to PATH.
    let opts = crate::parse_server_run_with_resolver(
        &args(&["--env-file", env_file.to_str().unwrap()]),
        || Some(bin.clone()),
    )
    .unwrap();
    assert_eq!(opts.bin, bin);
    assert!(opts.args.is_empty());
    assert_eq!(
        opts.env,
        vec![(
            std::ffi::OsString::from("WEBPI_ENV_FILE"),
            env_file.into_os_string()
        )]
    );
}

#[test]
fn server_run_without_env_file_preserves_server_startup_discovery() {
    let _guard = env_test_guard();
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp
        .path()
        .join(format!("webpi-server{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&bin, b"").unwrap();
    let _env = EnvGuard::new().set_os("PATH", tmp.path().as_os_str().to_os_string());

    let opts = crate::parse_server_run_with_resolver(&[], || Some(bin.clone())).unwrap();
    assert_eq!(opts.bin, bin);
    assert!(opts.args.is_empty());
    assert!(opts.env.is_empty());
}

#[test]
fn webpi_server_run_missing_installation_binary_fails_closed() {
    let err = crate::parse_server_run_with_resolver(&[], || None).unwrap_err();
    assert!(err.contains("PATH fallback is disabled"));
}

#[test]
fn server_run_rejects_invalid_env_file_shapes_before_binary_discovery() {
    let _guard = env_test_guard();
    let _env = EnvGuard::new().set_os("PATH", std::ffi::OsString::new());

    assert!(parse_server_run(&args(&["--env-file"]))
        .unwrap_err()
        .contains("--env-file"));
    assert_eq!(
        parse_server_run(&args(&["--env-file", "a", "--env-file", "b"])).unwrap_err(),
        "--env-file may be specified only once"
    );
    assert!(parse_server_run(&args(&["--bogus"]))
        .unwrap_err()
        .contains("unknown server run option"));
}
