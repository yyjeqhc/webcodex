use super::*;

fn surface_record(candidate: PlatformWindow) -> SurfaceRecord {
    SurfaceRecord {
        native_id: candidate.native_id,
        pid: candidate.pid,
        identity_hash: candidate.identity_hash,
        application: bounded_text(&candidate.application),
        title: bounded_text(&candidate.title),
        width: candidate.width,
        height: candidate.height,
    }
}

fn live_accessibility_smoke(application_matches: impl Fn(&str) -> bool) -> bool {
    let candidates = platform::list_windows(MAX_WINDOWS).expect("list live macOS windows");
    let Some(candidate) = candidates
        .into_iter()
        .find(|candidate| application_matches(&candidate.application))
    else {
        return false;
    };
    let record = surface_record(candidate);
    let tree = platform::accessibility_tree("surface_live", &record, 3, 64)
        .expect("read bounded live accessibility tree");
    let output = tree.output;
    assert_eq!(output["platform"], "macos");
    assert!(output["node_count"].as_u64().unwrap_or(0) > 0);
    for node in output["nodes"].as_array().expect("nodes array") {
        assert!(node["role"].as_str().is_some_and(|role| !role.is_empty()));
    }
    true
}

fn live_focus_control_smoke(application_matches: impl Fn(&str) -> bool) -> bool {
    let candidates = platform::list_windows(MAX_WINDOWS).expect("list live macOS windows");
    let Some(candidate) = candidates
        .into_iter()
        .find(|candidate| application_matches(&candidate.application))
    else {
        return false;
    };
    let record = surface_record(candidate);
    let surface_id = "surface_control_live";
    let tree = platform::accessibility_tree(surface_id, &record, 6, 128)
        .expect("read bounded accessibility tree for live focus control");
    let candidate_roles = [
        "AXTextField",
        "AXTextArea",
        "AXComboBox",
        "AXButton",
        "AXCheckBox",
        "AXRadioButton",
        "AXLink",
    ];
    for (element_id, element) in tree
        .elements
        .into_iter()
        .filter(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.has_positive_evidence()
                    && !fingerprint.protected
                    && candidate_roles.contains(&fingerprint.role.as_str())
            })
        })
        .take(16)
    {
        match platform::control(
            surface_id,
            &element_id,
            &record,
            &element,
            ComputerAction::Focus,
        ) {
            Ok(output) => {
                assert_eq!(output["action"], "focus");
                assert_eq!(output["success"], true);
                return true;
            }
            Err(error) if error.starts_with("control_failed:") => continue,
            Err(error) => {
                panic!("live focus control failed with uncertain/error state: {error}")
            }
        }
    }
    false
}

#[test]
#[ignore = "requires live macOS Accessibility permission and desktop"]
fn computer_macos_accessibility_permission_live_smoke() {
    let status = platform::accessibility_status().expect("read accessibility status");
    assert_eq!(status["trusted"], true);
}

#[test]
#[ignore = "requires live Microsoft Edge window and macOS Accessibility permission"]
fn computer_macos_accessibility_edge_live_smoke() {
    assert!(
        live_accessibility_smoke(|application| {
            application.to_ascii_lowercase().contains("microsoft edge")
                || application.to_ascii_lowercase() == "edge"
        }),
        "Microsoft Edge window must be open for this live smoke"
    );
}

#[test]
#[ignore = "requires live Microsoft Edge window and macOS Accessibility permission"]
fn computer_macos_control_focus_edge_live_smoke() {
    assert!(
        live_focus_control_smoke(|application| {
            application.to_ascii_lowercase().contains("microsoft edge")
                || application.to_ascii_lowercase() == "edge"
        }),
        "Microsoft Edge must expose a bounded focusable AX element for this live smoke"
    );
}

#[test]
#[ignore = "requires live WeChat window and macOS Accessibility permission"]
fn computer_macos_accessibility_wechat_live_smoke() {
    assert!(
        live_accessibility_smoke(|application| {
            application == "微信" || application.to_ascii_lowercase().contains("wechat")
        }),
        "WeChat window must be open for this live smoke"
    );
}
