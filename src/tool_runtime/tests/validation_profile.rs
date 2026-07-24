use super::super::validation_parser::{
    parse_cargo_check_diagnostics, parse_cargo_test_diagnostics, PARSER_KIND, PARSER_VERSION,
};
use super::super::validation_profile::{validation_adapter_for_tool, ValidationCommandOptions};
use super::super::{is_known_tool_name, registered_tool_specs};

#[test]
fn rust_profile_selects_cargo_fmt_adapter_and_preserves_command() {
    let adapter = validation_adapter_for_tool("cargo_fmt").expect("cargo_fmt adapter");
    assert_eq!(adapter.tool_identity(), "cargo_fmt");
    assert_eq!(adapter.validation_kind(), "format");
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                check: true,
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        "cargo fmt -- --check"
    );
}

#[test]
fn rust_profile_selects_cargo_check_adapter_and_preserves_command() {
    let adapter = validation_adapter_for_tool("cargo_check").expect("cargo_check adapter");
    assert_eq!(adapter.tool_identity(), "cargo_check");
    assert_eq!(adapter.validation_kind(), "check");
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions::default())
            .unwrap(),
        "cargo check --all-targets"
    );
    assert!(adapter
        .build_command(ValidationCommandOptions {
            features: Some("feat\0x".to_string()),
            ..ValidationCommandOptions::default()
        })
        .is_err());
}

#[test]
fn rust_profile_selects_cargo_test_adapter_and_preserves_command() {
    let adapter = validation_adapter_for_tool("cargo_test").expect("cargo_test adapter");
    assert_eq!(adapter.tool_identity(), "cargo_test");
    assert_eq!(adapter.validation_kind(), "test");
    assert!(adapter.reports_test_run_metadata());
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                filter: Some("tool_runtime".to_string()),
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        "cargo test 'tool_runtime'"
    );
}

#[test]
fn rust_adapter_parser_entries_preserve_parser_v3_results() {
    assert_eq!(PARSER_VERSION, 3);
    let stderr = "error[E0308]: mismatched types\n --> src/lib.rs:12:5\n";
    let check = validation_adapter_for_tool("cargo_check").unwrap();
    assert_eq!(
        check.parse("", stderr, false),
        parse_cargo_check_diagnostics("", stderr, false)
    );
    assert_eq!(check.parse("", stderr, false).parser, PARSER_KIND);

    let stdout = "running 1 test\ntest demo ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n";
    let test = validation_adapter_for_tool("cargo_test").unwrap();
    assert_eq!(
        test.parse(stdout, "", false),
        parse_cargo_test_diagnostics(stdout, "", false)
    );
    assert_eq!(test.parse(stdout, "", false).parser, PARSER_KIND);
}

#[test]
fn validation_profiles_reuse_existing_runtime_tool_schemas() {
    let specs = registered_tool_specs();
    for tool_name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        assert!(is_known_tool_name(tool_name));
        assert_eq!(
            specs.iter().filter(|spec| spec.name == tool_name).count(),
            1,
            "{tool_name} must retain exactly one existing runtime schema"
        );
    }
    assert!(validation_adapter_for_tool("validation_profile").is_none());
    assert!(!is_known_tool_name("validation_profile"));
    assert!(!is_known_tool_name("validation_adapter"));
}
