use super::*;

fn env_fixture(text: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "webcodex-safe-diagnostics-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("webcodex.env");
    std::fs::write(&file, text).unwrap();
    file
}

#[test]
fn trace_mode_updates_are_atomic_allowlisted_and_keep_unknown_entries() {
    let path=env_fixture("# operator settings\r\nWEBCODEX_TOKEN=private-canary\r\nCUSTOM_SETTING='literal value'\r\nWEBCODEX_MCP_COMPACT_SCHEMAS=true\r\n");
    let initial = inspect_trace(&path, true).unwrap();
    assert_eq!(initial.mode, TraceMode::Off);
    let next = update_trace(&path, TraceMode::Metadata, &initial.revision, false).unwrap();
    assert_eq!(next.mode, TraceMode::Metadata);
    assert!(next.restart_required);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("WEBCODEX_TOKEN=private-canary\r\n"));
    assert!(text.contains("CUSTOM_SETTING='literal value'\r\n"));
    assert!(text.contains("WEBCODEX_MCP_COMPACT_SCHEMAS=true\r\n"));
    assert_eq!(text.matches("WEBCODEX_TOOL_REQUEST_TRACE=").count(), 1);
    let full = update_trace(&path, TraceMode::Full, &next.revision, false).unwrap_err();
    assert_eq!(full.code, "full_trace_confirmation_required");
    let full = update_trace(&path, TraceMode::Full, &next.revision, true).unwrap();
    assert_eq!(full.mode, TraceMode::Full);
    let off = update_trace(&path, TraceMode::Off, &full.revision, false).unwrap();
    assert_eq!(off.mode, TraceMode::Off);
    assert!(!serde_json::to_string(&off)
        .unwrap()
        .contains("private-canary"));
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn duplicate_trace_entries_are_repaired_not_appended_and_stale_revision_rejects() {
    let path=env_fixture("WEBCODEX_TOOL_REQUEST_TRACE=off\nexport WEBCODEX_TOOL_REQUEST_TRACE = 'metadata' # duplicate\nUNRELATED=keep\n");
    let initial = inspect_trace(&path, false).unwrap();
    assert_eq!(
        initial.error_code.as_deref(),
        Some("server_environment_duplicate_key")
    );
    let next = update_trace(&path, TraceMode::Metadata, &initial.revision, false).unwrap();
    assert!(next.error_code.is_none());
    assert_eq!(
        std::fs::read_to_string(&path)
            .unwrap()
            .matches(TRACE_KEY)
            .count(),
        1
    );
    assert_eq!(
        update_trace(&path, TraceMode::Off, &initial.revision, false)
            .unwrap_err()
            .code,
        "server_environment_changed"
    );
    assert!(env_value(&path, "WEBCODEX_TOKEN").is_err());
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn trace_editor_rejects_symlink_config_and_writes_private_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let path = env_fixture("WEBCODEX_TOKEN=canary\n");
    let link = path.with_file_name("linked.env");
    symlink(&path, &link).unwrap();
    assert!(inspect_trace(&link, false).is_err());
    let before = inspect_trace(&path, false).unwrap();
    update_trace(&path, TraceMode::Metadata, &before.revision, false).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
        0
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn console_url_never_carries_a_credential_or_arbitrary_destination() {
    assert_eq!(
        console_url("http://127.0.0.1:1234/other?bearer=private#fragment").unwrap(),
        "http://127.0.0.1:1234/runtime"
    );
    assert!(console_url("https://example.com").is_err());
    assert!(console_url("http://secret@localhost:1234").is_err());
    assert!(console_url("file:///tmp/secret").is_err());
}

#[test]
fn continuation_distinguishes_handoff_unknown_streaming_and_next_observation() {
    let mut detail = json!({"active_count":0,"activity_truncated":false,"activity":[{"tool_name":"work_on_project","status":"succeeded","meaningful":true,"started_at_ms":100,"ended_at_ms":200}]});
    let first = continuation(&detail, 1000).unwrap();
    assert_eq!(first["execution"], "completed");
    assert_eq!(first["response_handoff"], "not_confirmed");
    detail["activity"][0]["request_observed_at_ms"] = json!(100);
    detail["activity"][0]["response_handed_at_ms"] = json!(210);
    let handed = continuation(&detail, 1000).unwrap();
    assert_eq!(handed["response_handoff"], "handler_returned");
    assert_eq!(handed["elapsed_ms"], 790);
    assert_eq!(handed["next_meaningful_call"], "not_observed");
    detail["activity"][0]["next_call_gap_ms"] = json!(400);
    assert_eq!(
        continuation(&detail, 1000).unwrap()["next_meaningful_call"],
        "not_observed"
    );
    assert_eq!(
        continuation(&detail, 1000).unwrap()["previous_response_gap_ms"],
        400
    );
    detail["activity"][0]["response_streaming"] = json!(true);
    assert_eq!(
        continuation(&detail, 1000).unwrap()["response_handoff"],
        "stream_started"
    );
    detail["activity"][0]["response_handed_at_ms"] = json!(99);
    assert_eq!(
        continuation(&detail, 1000).unwrap()["response_handoff"],
        "not_confirmed"
    );
}

#[test]
fn report_and_bundle_include_only_explicit_safe_projections() {
    let snapshot = crate::models::DesktopStateSnapshot::default();
    let runtime = RuntimeSettings {
        source: Default::default(),
        selection_revision: 0,
        desktop_contract: webcodex_core::desktop_runtime_contract::DESKTOP_RUNTIME_CONTRACT,
        selected: None,
        candidate: None,
        previous_source: None,
        last_switch: None,
        unavailable_code: None,
        active_jobs: None,
        can_switch: false,
        switch_unavailable_reason: None,
    };
    let trace = TraceSettings {
        mode: TraceMode::Off,
        effective_mode: None,
        revision: "fence".into(),
        available: true,
        restart_required: false,
        can_restart: false,
        error_code: None,
    };
    let runner = json!({"token":"private-token-canary","Authorization":"Bearer private-header-canary","env":{"password":"private-env-canary"},"capabilities":{"computer_observe":true,"secret":"private-caps-canary"},"protocol_compatibility":"compatible","arbitrary_stdout":"private-stdout-canary"});
    let activity = crate::activity::ActivityLog::default();
    activity.push(
        crate::activity::ActivityEventKind::ProcessStarted,
        "private-source-canary",
        crate::activity::ActivityLevel::Info,
        "private-message-canary",
    );
    let value = report(
        &snapshot,
        &runtime,
        Some(&runner),
        None,
        &trace,
        &activity.snapshot(),
        json!({"supported":false}),
    );
    let bytes = support_zip(&value).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    for private in [
        "private-token-canary",
        "private-header-canary",
        "private-env-canary",
        "private-caps-canary",
        "private-stdout-canary",
        "private-source-canary",
        "private-message-canary",
    ] {
        assert!(!text.contains(private), "leaked {private}");
    }
    assert!(text.contains("diagnostic-report.json"));
    assert!(text.contains("activity-safe.json"));
    assert_eq!(&bytes[..4], &[0x50, 0x4b, 0x03, 0x04]);
    assert_eq!(
        &bytes[bytes.len() - 22..bytes.len() - 18],
        &[0x50, 0x4b, 0x05, 0x06]
    );
    assert_eq!(
        value["computer_use"]["runner_advertised_capabilities"]["computer_observe"],
        true
    );
}

#[test]
fn support_export_never_overwrites_existing_files() {
    let path = env_fixture("original").with_file_name("report.zip");
    export_support(&path, &json!({"schema_version":1,"activity":[]})).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(export_support(&path, &json!({"new":true})).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
