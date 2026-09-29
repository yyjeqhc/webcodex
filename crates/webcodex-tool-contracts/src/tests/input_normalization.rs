use super::*;

#[test]
fn input_normalization_vocabulary_is_unique_closed_and_wire_stable() {
    let codes = ToolInputNormalizationCode::all();
    let spellings = codes.iter().map(|code| code.as_str()).collect::<Vec<_>>();
    // Frozen wire contract, intentionally independent of the implementation.
    assert_eq!(spellings, [
        "argv_to_args",
        "run_process_sh_c_to_run_shell",
        "run_process_bash_c_to_run_shell",
        "run_process_bash_lc_to_login_run_shell",
    ]);
    assert_eq!(spellings.iter().collect::<BTreeSet<_>>().len(), codes.len());
    for code in codes {
        assert_eq!(ToolInputNormalizationCode::from_wire(code.as_str()), Some(*code));
        assert_eq!(serde_json::to_value(code).unwrap(), code.as_str());
        assert!(code.model_hint().chars().count() <= 80);
    }
    for unknown in ["", "ARGV_TO_ARGS", " argv_to_args", "argv_to_args ", "private/parser/error"] {
        assert_eq!(ToolInputNormalizationCode::from_wire(unknown), None);
    }
}

#[test]
fn input_normalization_output_enums_derive_from_canonical_vocabulary() {
    for name in ["run_process", "run_detached_process"] {
        let schema = output_schema_for_tool(name);
        assert_eq!(
            schema.pointer("/properties/output/properties/input_normalization/properties/code/enum").unwrap(),
            &serde_json::to_value(ToolInputNormalizationCode::all()).unwrap(),
            "{name}"
        );
    }
}
