use super::*;
use objc2_foundation::{NSBundle, NSString, NSURL};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

fn create_test_application(path: &Path, name: &str, bundle_identifier: &str) {
    let executable_name = "TestExecutable";
    let contents = path.join("Contents");
    let executable_directory = contents.join("MacOS");
    fs::create_dir_all(&executable_directory).unwrap();
    let info_plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleIdentifier</key><string>{bundle_identifier}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>{executable_name}</string>
</dict>
</plist>
"#
    );
    fs::write(contents.join("Info.plist"), info_plist).unwrap();
    let executable = executable_directory.join(executable_name);
    fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(executable, permissions).unwrap();
}

fn assert_test_application_bundle(path: &Path) {
    let canonical = fs::canonicalize(path).unwrap();
    let native_path = NSString::from_str(canonical.to_str().unwrap());
    let url = NSURL::fileURLWithPath_isDirectory(&native_path, true);
    let bundle = NSBundle::bundleWithURL(&url).expect("synthetic test bundle");
    assert!(bundle.bundleIdentifier().is_some());
    assert!(bundle.executableURL().is_some());
}

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
fn computer_macos_application_discovery_is_bounded_and_exact_without_launching() {
    let applications =
        platform::list_applications(MAX_APPLICATION_SCAN).expect("native macOS app discovery");
    assert!(
        !applications.is_empty(),
        "a macOS host should expose at least one reliably launchable native application bundle"
    );
    assert!(applications.len() <= MAX_APPLICATION_SCAN);
    assert!(applications.windows(2).all(|pair| {
        application_candidate_order(&pair[0], &pair[1]) != std::cmp::Ordering::Greater
    }));
    for application in &applications {
        assert!(!application.display_name.is_empty());
        assert!(!application.native_identity.is_empty());
    }
    if let Some(application) = applications.first() {
        platform::application_identity_revalidates_for_test(&application.native_identity)
            .expect("fresh macOS application identity revalidates");
    }
}

#[test]
fn computer_macos_display_discovery_is_bounded_exact_and_noninvasive() {
    let displays = platform::list_displays(MAX_DISPLAYS + 1)
        .expect("bounded native macOS display discovery without capture");
    assert!(displays.len() <= MAX_DISPLAYS + 1);
    assert!(displays
        .iter()
        .all(|display| !display.native_identity.is_empty()));
    assert!(displays
        .iter()
        .all(|display| display.width > 0 && display.height > 0));
    if let Some(primary) = displays.iter().find(|display| display.primary) {
        println!(
            "macOS primary display source pixel geometry: {}x{}",
            primary.width, primary.height
        );
    }
    assert!(displays
        .iter()
        .enumerate()
        .all(|(index, display)| displays[..index]
            .iter()
            .all(|prior| prior.native_identity != display.native_identity)));
    for display in &displays {
        platform::macos_display_identity_revalidates_for_test(display)
            .expect("fresh exact macOS display identity and pixel geometry revalidate");
    }
}

#[test]
#[ignore = "requires live macOS event-post permission, an unrotated display, and idle mouse state"]
fn computer_macos_pointer_mapping_preflight_is_native_and_non_effecting() {
    let displays = platform::list_displays(MAX_DISPLAYS + 1)
        .expect("bounded native macOS display discovery without capture");
    let primary = displays
        .iter()
        .find(|display| display.primary)
        .expect("macOS host exposes one primary display");
    let probe = platform::macos_pointer_read_only_probe_for_test(primary)
        .expect("read-only macOS pointer mapping/preflight probe");
    let (origin_x, origin_y, bounds_width, bounds_height) = probe.bounds;
    let (target_x, target_y) = probe.mapped_edge;
    println!(
        "macOS pointer preflight source={}x{} bounds=({origin_x},{origin_y} {bounds_width}x{bounds_height}) rotation={} mapped_edge=({target_x},{target_y}) buttons_down=0x{:08x} modifier_flags=0x{:x} prohibited_modifiers_active={} event_post_permission={} constructed_events={}",
        probe.source_width,
        probe.source_height,
        probe.rotation_degrees,
        probe.buttons_down,
        probe.modifier_flags,
        probe.prohibited_modifiers_active,
        probe.event_post_permission,
        probe.constructed_event_count,
    );
    assert_eq!(
        (probe.source_width, probe.source_height),
        (primary.width, primary.height)
    );
    assert!(origin_x.is_finite() && origin_y.is_finite());
    assert!(bounds_width.is_finite() && bounds_width > 0.0);
    assert!(bounds_height.is_finite() && bounds_height > 0.0);
    assert_eq!(probe.rotation_degrees, 0.0);
    assert!(target_x >= origin_x && target_x < origin_x + bounds_width);
    assert!(target_y >= origin_y && target_y < origin_y + bounds_height);
    assert_eq!(probe.buttons_down, 0);
    assert!(probe.event_post_permission);
    assert!(!probe.prohibited_modifiers_active);
    assert_eq!(probe.constructed_event_count, 3);
}

#[test]
fn computer_macos_application_scan_is_bounded_symlink_safe_and_treats_apps_as_leaves() {
    let temp = tempfile::tempdir().unwrap();
    let applications_root = temp.path().join("Applications");
    fs::create_dir_all(applications_root.join("Utilities")).unwrap();

    let outer = applications_root.join("Outer.app");
    create_test_application(&outer, "Outer", "dev.webcodex.outer");
    assert_test_application_bundle(&outer);
    create_test_application(
        &outer.join("Contents/Nested.app"),
        "Nested",
        "dev.webcodex.nested",
    );
    create_test_application(
        &applications_root.join("Utilities/Utility.app"),
        "Utility",
        "dev.webcodex.utility",
    );

    let outside = temp.path().join("Outside/Outside.app");
    create_test_application(&outside, "Outside", "dev.webcodex.outside");
    symlink(&outside, applications_root.join("Escape.app")).unwrap();

    let applications = platform::macos_applications_in_roots_for_test(
        std::slice::from_ref(&applications_root),
        MAX_APPLICATION_SCAN,
    );
    assert_eq!(
        applications
            .iter()
            .map(|application| application.display_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Outer", "Utility"]
    );
}

#[test]
fn computer_macos_application_replacement_at_same_path_is_stale() {
    let temp = tempfile::tempdir().unwrap();
    let applications_root = temp.path().join("Applications");
    fs::create_dir_all(&applications_root).unwrap();
    let application_path = applications_root.join("Replaceable.app");
    create_test_application(&application_path, "Replaceable", "dev.webcodex.replaceable");
    let first = platform::macos_applications_in_roots_for_test(
        std::slice::from_ref(&applications_root),
        MAX_APPLICATION_SCAN,
    );
    let original_identity = first[0].native_identity.clone();

    fs::rename(&application_path, applications_root.join("Retired.app")).unwrap();
    create_test_application(&application_path, "Replaceable", "dev.webcodex.replaceable");
    let second = platform::macos_applications_in_roots_for_test(
        std::slice::from_ref(&applications_root),
        MAX_APPLICATION_SCAN,
    );
    let replacement = second
        .iter()
        .find(|application| application.display_name == "Replaceable")
        .unwrap();
    assert_ne!(replacement.native_identity, original_identity);
    let error = platform::macos_application_identity_revalidates_in_roots_for_test(
        &original_identity,
        std::slice::from_ref(&applications_root),
    )
    .expect_err("same-path replacement must retire the discovered identity");
    assert!(error.starts_with("stale_application:"), "{error}");
}

#[test]
fn computer_macos_application_replacement_after_launch_preparation_never_dispatches() {
    let temp = tempfile::tempdir().unwrap();
    let applications_root = temp.path().join("Applications");
    fs::create_dir_all(&applications_root).unwrap();
    let application_path = applications_root.join("Replaceable.app");
    create_test_application(&application_path, "Replaceable", "dev.webcodex.replaceable");
    let discovered = platform::macos_applications_in_roots_for_test(
        std::slice::from_ref(&applications_root),
        MAX_APPLICATION_SCAN,
    );
    let original_identity = discovered[0].native_identity.clone();

    let (result, dispatch_attempts) = platform::macos_application_launch_preparation_race_for_test(
        &original_identity,
        std::slice::from_ref(&applications_root),
        || {
            fs::rename(&application_path, applications_root.join("Retired.app")).unwrap();
            create_test_application(&application_path, "Replaceable", "dev.webcodex.replaceable");
        },
    );

    let error = result.expect_err("replacement after preparation must fail before dispatch");
    assert!(error.starts_with("stale_application:"), "{error}");
    assert!(!error.starts_with("outcome_unknown:"), "{error}");
    assert_eq!(dispatch_attempts, 0, "stale target must never dispatch");
}

#[test]
fn computer_macos_application_launch_configuration_is_nonactivating_and_closed() {
    assert!(platform::macos_application_launch_configuration_for_test());
    assert_eq!(
        platform::macos_application_launch_completion_for_test(true, false),
        "success"
    );
    for (has_application, has_error) in [(false, true), (false, false), (true, true)] {
        assert_eq!(
            platform::macos_application_launch_completion_for_test(has_application, has_error),
            "outcome_unknown"
        );
    }
    assert_eq!(
        platform::macos_application_launch_lost_completion_for_test(),
        (true, true)
    );
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
