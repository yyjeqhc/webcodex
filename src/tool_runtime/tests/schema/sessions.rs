use super::*;

#[test]
fn session_tool_specs_describe_ledger_vs_current_binding() {
    let specs = registered_tool_specs();

    let desc = |name: &str| spec_named(&specs, name).description.to_lowercase();

    let start_desc = desc("start_session");
    for phrase in [
        "explicit wc_sess_* session_id",
        "session ledger",
        "does not by itself bind future calls as current",
    ] {
        assert!(
            start_desc.contains(phrase),
            "start_session description should mention {phrase}: {start_desc}"
        );
    }

    let summary_desc = desc("session_summary");
    for phrase in [
        "session ledger",
        "explicit session_id",
        "does not rely on current-session binding",
    ] {
        assert!(
            summary_desc.contains(phrase),
            "session_summary description should mention {phrase}: {summary_desc}"
        );
    }

    let handoff_desc = desc("session_handoff_summary");
    for phrase in [
        "session ledger",
        "explicit session_id",
        "ledger-derived validation",
        "bounded tails",
        "safe result metadata",
        "validation.parser.available",
        "does not depend on current-session binding",
    ] {
        assert!(
            handoff_desc.contains(phrase),
            "session_handoff_summary description should mention {phrase}: {handoff_desc}"
        );
    }

    for name in [
        "bind_current_session",
        "current_session",
        "unbind_current_session",
    ] {
        let current_desc = desc(name);
        for phrase in ["process-local in-memory", "not the durable session ledger"] {
            assert!(
                current_desc.contains(phrase),
                "{name} description should mention {phrase}: {current_desc}"
            );
        }
    }

    for name in ["bind_current_session", "current_session"] {
        let current_desc = desc(name);
        assert!(
            current_desc.contains("may be lost on restart"),
            "{name} description should mention restart loss: {current_desc}"
        );
    }
}
