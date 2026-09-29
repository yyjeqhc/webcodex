use super::{run_process_validation_identity, run_script_validation_identity};

#[test]
fn generic_validation_identity_uses_canonical_execution_purpose_classification() {
    let args = vec!["--check".to_string()];
    for purpose in ["validation", "test", "build", "format", "release"] {
        assert!(
            run_process_validation_identity(
                "custom-validator",
                &args,
                None,
                Some("."),
                Some(purpose),
            )
            .is_some(),
            "run_process {purpose}"
        );
        assert!(
            run_script_validation_identity(
                "sh",
                "custom-validator --check",
                &[],
                None,
                Some("."),
                Some(purpose),
            )
            .is_some(),
            "run_script {purpose}"
        );
    }

    for purpose in ["diagnostic", "operation", "other"] {
        assert!(
            run_process_validation_identity(
                "custom-validator",
                &args,
                None,
                Some("."),
                Some(purpose),
            )
            .is_none(),
            "run_process {purpose}"
        );
        assert!(
            run_script_validation_identity(
                "sh",
                "custom-validator --check",
                &[],
                None,
                Some("."),
                Some(purpose),
            )
            .is_none(),
            "run_script {purpose}"
        );
    }
    assert!(
        run_process_validation_identity("custom-validator", &args, None, Some("."), None).is_none()
    );
}

#[test]
fn native_multi_package_cargo_check_matches_structured_identity() {
    let args = vec![
        "check".to_string(),
        "--all-targets".to_string(),
        "-p".to_string(),
        "package-b".to_string(),
        "-p".to_string(),
        "package-a".to_string(),
    ];
    let native =
        run_process_validation_identity("cargo", &args, None, Some("."), Some("validation"))
            .expect("canonical Cargo validation identity");
    let structured = webcodex_core::validation_identity::structured_validation_target_identity(
        webcodex_core::validation_identity::ToolValidationIdentityKind::CargoCheck,
        &serde_json::json!({
            "cwd": ".",
            "all_targets": true,
            "packages": ["package-a", "package-b"]
        }),
    )
    .unwrap();

    assert_eq!(native.validation_tool, Some("cargo_check"));
    assert_eq!(native.identity, structured);
}
