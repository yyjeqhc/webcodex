use super::*;
use serde_json::json;

#[test]
fn overlap_is_inclusive_and_invalid_ranges_fail_closed() {
    let range = |min_generation, max_generation| DesktopRuntimeContract {
        min_generation,
        max_generation,
    };
    assert!(range(1, 2).overlaps(range(2, 3)));
    assert!(!range(1, 1).overlaps(range(2, 2)));
    assert!(!range(0, 1).overlaps(range(1, 1)));
    assert!(!range(3, 1).overlaps(range(1, 3)));
    assert_eq!(range(1, 3).intersection(range(2, 4)), Some(range(2, 3)));
}

#[test]
fn build_differences_are_diagnostics_not_protocol_authority() {
    assert_eq!(
        build_alignment(
            Some("0.4.1"),
            Some("aaaaaaa"),
            Some(false),
            Some("0.5.0"),
            Some("bbbbbbb"),
            Some(false)
        ),
        BuildAlignment::DifferentVersion
    );
    assert_eq!(
        build_alignment(
            Some("0.4.1"),
            Some("aaaaaaa"),
            Some(false),
            Some("0.4.1"),
            Some("bbbbbbb"),
            Some(false)
        ),
        BuildAlignment::DifferentCommit
    );
    assert_eq!(
        build_alignment(
            Some("0.4.1"),
            Some("aaaaaaa"),
            Some(true),
            Some("0.4.1"),
            Some("aaaaaaa"),
            Some(false)
        ),
        BuildAlignment::Dirty
    );
    assert_eq!(
        build_alignment(
            Some("0.4.1"),
            Some("aaaaaaa"),
            Some(false),
            Some("0.4.1"),
            Some("aaaaaaa"),
            Some(false)
        ),
        BuildAlignment::Exact
    );
    assert!(DESKTOP_RUNTIME_CONTRACT.overlaps(DESKTOP_RUNTIME_CONTRACT));
}

#[test]
fn metadata_ignores_additive_fields_and_preserves_all_three_identities() {
    for binary in ["webcodex", "webcodex-server", "webcodex-runner"] {
        let mut raw = serde_json::to_value(crate::build_info::machine_build_info(binary)).unwrap();
        raw["future_field"] = json!({"anything": true});
        raw["desktop_runtime_contract"]["future_field"] = json!(true);
        let parsed: MachineBuildInfo = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.binary, binary);
        parsed.validate(binary).unwrap();
    }
}

#[test]
fn metadata_rejects_unverifiable_contract_and_control_text() {
    let mut info = crate::build_info::machine_build_info("webcodex");
    info.schema_version = 99;
    assert_eq!(
        info.validate("webcodex"),
        Err("build_info_schema_unsupported")
    );
    info.schema_version = BUILD_INFO_SCHEMA_VERSION;
    assert_eq!(
        info.validate("webcodex-server"),
        Err("build_info_binary_mismatch")
    );
    info.desktop_runtime_contract.min_generation = 0;
    assert_eq!(info.validate("webcodex"), Err("runtime_contract_malformed"));
    info.desktop_runtime_contract = DESKTOP_RUNTIME_CONTRACT;
    info.version = "1.0.0\nAuthorization: secret".into();
    assert_eq!(
        info.validate("webcodex"),
        Err("build_info_metadata_invalid")
    );
}

#[test]
fn missing_fields_do_not_implicitly_advertise_generation_one() {
    assert!(serde_json::from_value::<MachineBuildInfo>(
        json!({"binary":"webcodex","version":"0.4.1"})
    )
    .is_err());
    assert!(serde_json::from_value::<DesktopRuntimeContract>(json!({"max_generation":1})).is_err());
}

#[test]
fn runner_generation_not_version_defines_wire_compatibility() {
    assert_eq!(
        runner_protocol_compatibility(crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2.get()),
        ProtocolCompatibility::Compatible
    );
    assert_eq!(
        runner_protocol_compatibility(0),
        ProtocolCompatibility::Unknown
    );
    assert_eq!(
        runner_protocol_compatibility(u16::MAX),
        ProtocolCompatibility::Incompatible
    );
}
