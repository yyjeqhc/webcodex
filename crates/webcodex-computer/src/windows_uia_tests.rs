use super::*;
use std::process::{Child, Command, Stdio};
use std::thread;
use windows::Win32::UI::Accessibility::{
    UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_DocumentControlTypeId,
    UIA_EditControlTypeId, UIA_HyperlinkControlTypeId, UIA_WindowControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

#[test]
fn windows_application_discovery_is_bounded_and_identity_revalidation_is_exact() {
    let applications =
        platform::list_applications(MAX_APPLICATION_SCAN).expect("Windows AppsFolder discovery");
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
            .expect("fresh native application identity revalidates");
        assert!(platform::application_shell_execute_contract_for_test(
            &application.native_identity
        )
        .expect("construct exact PIDL ShellExecute contract"));

        let mut changed = application.native_identity.clone();
        changed[0] ^= 0xff;
        let error = platform::application_identity_revalidates_for_test(&changed)
            .expect_err("changed identity must fail closed before launch");
        assert!(error.starts_with("stale_application:"), "{error}");
    }
}

#[test]
fn windows_pointer_dpi_context_uses_physical_monitor_space_and_restores() {
    let (before, during, xcap_bounds, after) =
        platform::test_windows_pointer_dpi_context_metrics().unwrap();
    assert_eq!(during, xcap_bounds);
    assert_eq!(after, before);
}

#[test]
fn windows_display_discovery_and_exact_capture_use_private_identity() {
    let displays = platform::list_displays(2).expect("Windows display discovery");
    assert!(displays.len() <= 2);
    let Some(display) = displays.first() else {
        return;
    };
    assert!(!display.native_identity.is_empty());
    assert!(display.width > 0 && display.height > 0);
    let record = DisplayRecord {
        native_identity: display.native_identity.clone(),
        width: display.width,
        height: display.height,
        primary: display.primary,
    };
    let image = platform::capture_display(&record).expect("exact Windows display capture");
    assert_eq!(image.width(), record.width);
    assert_eq!(image.height(), record.height);

    let mut changed = record.clone();
    changed.native_identity[0] ^= 0xff;
    let error = platform::capture_display(&changed)
        .expect_err("changed display identity must never capture another monitor");
    assert!(error.starts_with("stale_display:"), "{error}");
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

#[test]
fn windows_pointer_mapping_is_display_local_and_virtual_desktop_exact() {
    let left = platform::test_windows_pointer_map(-1920, 0, 1920, 1080, -1920, 0, 3840, 1080, 0, 0)
        .unwrap();
    assert_eq!((left.0, left.1, left.2, left.3), (-1920, 0, 0, 0));

    let left_bottom_right =
        platform::test_windows_pointer_map(-1920, 0, 1920, 1080, -1920, 0, 3840, 1080, 1919, 1079)
            .unwrap();
    assert_eq!((left_bottom_right.0, left_bottom_right.1), (-1, 1079));
    assert!(left_bottom_right.2 > 0 && left_bottom_right.2 < 65_535);
    assert_eq!(left_bottom_right.3, 65_535);

    let offset =
        platform::test_windows_pointer_map(2560, -900, 1600, 900, 0, -900, 4160, 2340, 100, 200)
            .unwrap();
    assert_eq!((offset.0, offset.1), (2660, -700));
    assert!(offset.2 > 0 && offset.3 > 0);

    assert!(platform::test_windows_pointer_map(
        -1920, 0, 1920, 1080, -1920, 0, 3840, 1080, 1920, 0,
    )
    .unwrap_err()
    .starts_with("invalid_request:"));
    assert!(
        platform::test_windows_pointer_map(0, 0, 70_000, 1080, 0, 0, 70_000, 1080, 69_999, 0,)
            .unwrap_err()
            .starts_with("pointer_input_failed:")
    );
    assert!(platform::test_windows_pointer_coordinate_spaces(
        (-1920, 0, 3840, 1080),
        (0, 0, 1920, 1080),
    )
    .unwrap_err()
    .contains("cannot be proven identical"));
}

#[test]
fn windows_pointer_shared_input_guards_fail_closed_without_releasing_state() {
    assert!(platform::test_windows_pointer_state_guard(PointerAction::Move, None).is_ok());
    assert!(platform::test_windows_pointer_state_guard(PointerAction::Move, Some(16)).is_ok());
    for down in [1u16, 2, 4, 5, 6] {
        assert!(
            platform::test_windows_pointer_state_guard(PointerAction::Move, Some(down))
                .unwrap_err()
                .starts_with("pointer_input_failed:")
        );
    }
    for down in [1u16, 16, 17, 18, 91, 92] {
        assert!(
            platform::test_windows_pointer_state_guard(PointerAction::Click, Some(down))
                .unwrap_err()
                .starts_with("pointer_input_failed:")
        );
    }
}

#[test]
fn windows_pointer_send_input_lifecycle_and_postconditions_are_closed() {
    assert_eq!(
        platform::test_windows_pointer_input_flags(PointerAction::Move),
        vec![49_153]
    );
    assert_eq!(
        platform::test_windows_pointer_input_flags(PointerAction::Click),
        vec![49_153, 2, 4]
    );
    assert!(platform::test_windows_pointer_move_send_input_count(1).is_ok());
    let zero = platform::test_windows_pointer_move_send_input_count(0).unwrap_err();
    assert!(zero.starts_with("not_started:"), "{zero}");
    assert!(platform::test_windows_pointer_button_send_input_count(2).is_ok());
    for inserted in [0, 1] {
        let error = platform::test_windows_pointer_button_send_input_count(inserted).unwrap_err();
        assert!(error.starts_with("outcome_unknown:"), "{error}");
    }

    assert!(platform::test_windows_pointer_postcondition(
        -100,
        50,
        -100,
        50,
        PointerAction::Move,
        false,
    )
    .is_ok());
    let moved_elsewhere =
        platform::test_windows_pointer_postcondition(-100, 50, -99, 50, PointerAction::Move, false)
            .unwrap_err();
    assert!(moved_elsewhere.starts_with("outcome_unknown:"));
    assert!(platform::test_windows_pointer_postcondition(
        10,
        20,
        10,
        20,
        PointerAction::Click,
        false,
    )
    .is_ok());
    let stuck =
        platform::test_windows_pointer_postcondition(10, 20, 10, 20, PointerAction::Click, true)
            .unwrap_err();
    assert!(stuck.starts_with("outcome_unknown:"));

    let (mismatch, sent, state_checks) = platform::test_windows_pointer_dispatch_trace(
        PointerAction::Click,
        1,
        (9, 20),
        None,
        2,
        (10, 20),
        false,
    );
    assert!(mismatch.unwrap_err().starts_with("outcome_unknown:"));
    assert_eq!(sent, vec![vec![49_153]]);
    assert_eq!(state_checks, 0);

    for button_inserted in [0, 1] {
        let (result, sent, state_checks) = platform::test_windows_pointer_dispatch_trace(
            PointerAction::Click,
            1,
            (10, 20),
            None,
            button_inserted,
            (10, 20),
            false,
        );
        assert!(result.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(sent, vec![vec![49_153], vec![2, 4]]);
        assert_eq!(state_checks, 1);
    }

    let (held, sent, state_checks) = platform::test_windows_pointer_dispatch_trace(
        PointerAction::Click,
        1,
        (10, 20),
        Some(16),
        2,
        (10, 20),
        false,
    );
    assert!(held.unwrap_err().starts_with("outcome_unknown:"));
    assert_eq!(sent, vec![vec![49_153]]);
    assert_eq!(state_checks, 1);

    let (success, sent, state_checks) = platform::test_windows_pointer_dispatch_trace(
        PointerAction::Click,
        1,
        (10, 20),
        None,
        2,
        (10, 20),
        false,
    );
    assert_eq!(success.unwrap(), true);
    assert_eq!(sent, vec![vec![49_153], vec![2, 4]]);
    assert_eq!(state_checks, 1);
}

const WINDOWS_CONTROL_FIXTURE_TITLE: &str = "WebCodex Windows UIA Control Smoke";
const WINDOWS_FOREGROUND_PROBE_TITLE: &str = "WebCodex Windows UIA Foreground Probe";

struct WindowsControlFixture {
    child: Child,
}

impl WindowsControlFixture {
    fn start() -> Self {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()
$form = New-Object System.Windows.Forms.Form
$form.Text = 'WebCodex Windows UIA Control Smoke'
$form.StartPosition = 'Manual'
$form.Location = New-Object System.Drawing.Point(120, 120)
$form.Size = New-Object System.Drawing.Size(820, 360)
$form.TopMost = $true
$input = New-Object System.Windows.Forms.TextBox
$input.Name = 'SmokeInput'
$input.AccessibleName = 'Smoke input'
$input.Location = New-Object System.Drawing.Point(24, 44)
$input.Size = New-Object System.Drawing.Size(280, 28)
$input.TabIndex = 0
$button = New-Object System.Windows.Forms.Button
$button.Name = 'SmokePress'
$button.Text = 'Smoke press'
$button.Location = New-Object System.Drawing.Point(24, 96)
$button.Size = New-Object System.Drawing.Size(140, 32)
$button.TabIndex = 1
$status = New-Object System.Windows.Forms.Label
$status.Name = 'SmokeStatus'
$status.Text = 'ready'
$status.Location = New-Object System.Drawing.Point(24, 148)
$status.Size = New-Object System.Drawing.Size(140, 24)
$password = New-Object System.Windows.Forms.TextBox
$password.Name = 'ProtectedInput'
$password.AccessibleName = 'Protected input'
$password.UseSystemPasswordChar = $true
$password.Location = New-Object System.Drawing.Point(24, 196)
$password.Size = New-Object System.Drawing.Size(280, 28)
$password.TabIndex = 2
$scrollList = New-Object System.Windows.Forms.ListView
$scrollList.Name = 'ScrollList'
$scrollList.AccessibleName = 'Scroll list'
$scrollList.View = [System.Windows.Forms.View]::List
$scrollList.Scrollable = $true
$scrollList.Location = New-Object System.Drawing.Point(500, 44)
$scrollList.Size = New-Object System.Drawing.Size(240, 92)
$scrollList.TabIndex = 3
for ($i = 0; $i -lt 40; $i++) {
$item = New-Object System.Windows.Forms.ListViewItem(('Scroll item {0:D2}' -f $i))
[void]$scrollList.Items.Add($item)
}
$script:twinPrimary = New-Object System.Windows.Forms.Button
$script:twinPrimary.Name = 'TwinAction'
$script:twinPrimary.Text = 'Twin action'
$script:twinPrimary.AccessibleName = 'Twin action'
$script:twinPrimary.Location = New-Object System.Drawing.Point(210, 96)
$script:twinPrimary.Size = New-Object System.Drawing.Size(120, 32)
$script:twinPrimary.TabIndex = 10
$script:twinPrimary.Add_Click({ param($sender, $eventArgs) $sender.Text = 'wrong target invoked' })
$script:twinSecondary = New-Object System.Windows.Forms.Button
$script:twinSecondary.Name = 'TwinAction'
$script:twinSecondary.Text = 'Twin action'
$script:twinSecondary.AccessibleName = 'Twin action'
$script:twinSecondary.Location = New-Object System.Drawing.Point(350, 96)
$script:twinSecondary.Size = New-Object System.Drawing.Size(120, 32)
$script:twinSecondary.TabIndex = 11
$script:twinSecondary.Add_Click({ param($sender, $eventArgs) $sender.Text = 'wrong target invoked' })
$replaceTwins = New-Object System.Windows.Forms.Button
$replaceTwins.Name = 'ReplaceTwins'
$replaceTwins.Text = 'Replace twins'
$replaceTwins.AccessibleName = 'Replace twins'
$replaceTwins.Location = New-Object System.Drawing.Point(210, 148)
$replaceTwins.Size = New-Object System.Drawing.Size(140, 32)
$replaceTwins.TabIndex = 12
$replaceTwins.Add_Click({
param($sender, $eventArgs)
$primaryIndex = $form.Controls.GetChildIndex($script:twinPrimary)
$secondaryIndex = $form.Controls.GetChildIndex($script:twinSecondary)
$form.Controls.Remove($script:twinPrimary)
$form.Controls.Remove($script:twinSecondary)
$script:twinPrimary.Dispose()
$script:twinSecondary.Dispose()
$script:twinPrimary = New-Object System.Windows.Forms.Button
$script:twinPrimary.Name = 'TwinAction'
$script:twinPrimary.Text = 'Twin action'
$script:twinPrimary.AccessibleName = 'Twin action'
$script:twinPrimary.Location = New-Object System.Drawing.Point(210, 96)
$script:twinPrimary.Size = New-Object System.Drawing.Size(120, 32)
$script:twinPrimary.TabIndex = 10
$script:twinPrimary.Add_Click({ param($replacementSender, $replacementEventArgs) $replacementSender.Text = 'wrong target invoked' })
$script:twinSecondary = New-Object System.Windows.Forms.Button
$script:twinSecondary.Name = 'TwinAction'
$script:twinSecondary.Text = 'Twin action'
$script:twinSecondary.AccessibleName = 'Twin action'
$script:twinSecondary.Location = New-Object System.Drawing.Point(350, 96)
$script:twinSecondary.Size = New-Object System.Drawing.Size(120, 32)
$script:twinSecondary.TabIndex = 11
$script:twinSecondary.Add_Click({ param($replacementSender, $replacementEventArgs) $replacementSender.Text = 'wrong target invoked' })
$form.Controls.Add($script:twinPrimary)
$form.Controls.Add($script:twinSecondary)
$form.Controls.SetChildIndex($script:twinPrimary, $primaryIndex)
$form.Controls.SetChildIndex($script:twinSecondary, $secondaryIndex)
})
$button.Add_Click({ param($sender, $eventArgs) $sender.Text = 'clicked' })
$form.Controls.Add($input)
$form.Controls.Add($button)
$form.Controls.Add($status)
$form.Controls.Add($password)
$form.Controls.Add($scrollList)
$form.Controls.Add($script:twinPrimary)
$form.Controls.Add($script:twinSecondary)
$form.Controls.Add($replaceTwins)
$form.Add_Shown({ $form.Activate(); $input.Focus() })
[System.Windows.Forms.Application]::Run($form)
"#;
        let child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-STA",
                "-WindowStyle",
                "Hidden",
                "-Command",
                script,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch private WinForms control fixture");
        Self { child }
    }

    fn start_foreground_probe() -> Self {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()
$form = New-Object System.Windows.Forms.Form
$form.Text = 'WebCodex Windows UIA Foreground Probe'
$form.StartPosition = 'Manual'
$form.Location = New-Object System.Drawing.Point(980, 120)
$form.Size = New-Object System.Drawing.Size(320, 180)
$form.TopMost = $true
$button = New-Object System.Windows.Forms.Button
$button.Text = 'Foreground probe'
$button.AccessibleName = 'Foreground probe'
$button.Location = New-Object System.Drawing.Point(24, 44)
$button.Size = New-Object System.Drawing.Size(180, 32)
$form.Controls.Add($button)
$form.Add_Shown({ $form.Activate(); $button.Focus() })
[System.Windows.Forms.Application]::Run($form)
"#;
        let child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-STA",
                "-WindowStyle",
                "Hidden",
                "-Command",
                script,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch private WinForms foreground probe");
        Self { child }
    }

    fn wait_for_window(&mut self, title: &str, context: &str) -> PlatformWindow {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .unwrap_or_else(|error| panic!("query {context} process: {error}"))
            {
                panic!("{context} exited before discovery: {status}");
            }
            if let Some(candidate) = platform::list_windows(4096)
                .unwrap_or_else(|error| panic!("list Windows windows for {context}: {error}"))
                .into_iter()
                .find(|candidate| candidate.title == title)
            {
                return candidate;
            }
            let now = Instant::now();
            assert!(now < deadline, "timed out discovering {context}");
            thread::sleep(
                deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(20)),
            );
        }
    }
}

impl Drop for WindowsControlFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn computer_windows_uia_control_types_use_existing_semantic_roles() {
    assert_eq!(
        platform::uia_control_role(UIA_WindowControlTypeId),
        "AXWindow"
    );
    assert_eq!(
        platform::uia_control_role(UIA_ButtonControlTypeId),
        "AXButton"
    );
    assert_eq!(
        platform::uia_control_role(UIA_EditControlTypeId),
        "AXTextField"
    );
    assert_eq!(
        platform::uia_control_role(UIA_DocumentControlTypeId),
        "AXTextArea"
    );
    assert_eq!(
        platform::uia_control_role(UIA_HyperlinkControlTypeId),
        "AXLink"
    );
    assert_eq!(
        platform::uia_control_role(UIA_CheckBoxControlTypeId),
        "AXCheckBox"
    );
    assert!(platform::uia_semantic_focus_role("AXTextField"));
    assert!(!platform::uia_semantic_focus_role("AXTextArea"));
    assert!(platform::uia_semantic_text_input_role("AXTextField"));
    assert!(!platform::uia_semantic_text_input_role("AXTextArea"));
    assert!(!platform::uia_semantic_focus_role("AXButton"));
    assert!(!platform::uia_semantic_focus_role("AXWindow"));
}

#[test]
fn computer_windows_window_activation_attempt_failure_is_unknown() {
    let error = platform::windows_window_activation_attempt_error("IUIAutomationElement::SetFocus");
    assert!(error.starts_with("outcome_unknown:"), "{error}");
}

#[test]
fn computer_windows_control_attempt_failure_is_unknown() {
    let error = platform::windows_control_attempt_error("IUIAutomationInvokePattern::Invoke");
    assert!(error.starts_with("outcome_unknown:"), "{error}");
}

#[test]
fn computer_windows_scroll_attempt_failure_is_unknown() {
    let error =
        platform::windows_scroll_attempt_error("IUIAutomationScrollItemPattern::ScrollIntoView");
    assert!(error.starts_with("outcome_unknown:"), "{error}");
}

#[test]
fn computer_windows_key_input_native_plan_is_closed_and_releases_modifiers_in_reverse() {
    for (key, expected_vk) in [
        ("enter", 13u16),
        ("escape", 27),
        ("tab", 9),
        ("arrow_up", 38),
        ("arrow_down", 40),
        ("arrow_left", 37),
        ("arrow_right", 39),
        ("page_up", 33),
        ("page_down", 34),
        ("home", 36),
        ("end", 35),
    ] {
        assert_eq!(
            platform::test_windows_key_input_plan(key, &[]).unwrap(),
            vec![
                (
                    expected_vk,
                    false,
                    matches!(
                        key,
                        "arrow_up"
                            | "arrow_down"
                            | "arrow_left"
                            | "arrow_right"
                            | "page_up"
                            | "page_down"
                            | "home"
                            | "end"
                    )
                ),
                (
                    expected_vk,
                    true,
                    matches!(
                        key,
                        "arrow_up"
                            | "arrow_down"
                            | "arrow_left"
                            | "arrow_right"
                            | "page_up"
                            | "page_down"
                            | "home"
                            | "end"
                    )
                ),
            ],
            "{key}"
        );
    }
    assert!(platform::test_windows_key_input_plan("a", &[]).is_err());

    let option = platform::test_windows_key_input_plan("arrow_left", &["option".to_string()])
        .expect("Windows option modifier must map to Alt for exact-surface chords");
    assert_eq!(
        option,
        vec![
            (18, false, false),
            (37, false, true),
            (37, true, true),
            (18, true, false),
        ]
    );
    for (key, modifiers) in [
        ("tab", vec!["option".to_string()]),
        ("escape", vec!["option".to_string()]),
        ("escape", vec!["control".to_string()]),
        ("escape", vec!["shift".to_string(), "control".to_string()]),
    ] {
        let error = platform::test_windows_key_input_plan(key, &modifiers)
            .expect_err("Windows system-level chord must fail before native input");
        assert!(error.starts_with("key_input_failed:"), "{error}");
    }

    let sequence = platform::test_windows_key_input_plan(
        "arrow_right",
        &[
            "shift".to_string(),
            "control".to_string(),
            "option".to_string(),
        ],
    )
    .expect("prepare complete Windows modifier/key sequence");
    assert_eq!(
        sequence,
        vec![
            (16, false, false),
            (17, false, false),
            (18, false, false),
            (39, false, true),
            (39, true, true),
            (18, true, false),
            (17, true, false),
            (16, true, false),
        ]
    );

    let command = platform::test_windows_key_input_plan("tab", &["command".to_string()])
        .expect_err("Windows command modifier must fail before native input");
    assert!(command.starts_with("key_input_failed:"), "{command}");
}

#[test]
fn computer_windows_key_input_zero_is_deterministic_and_partial_injection_is_unknown() {
    assert!(platform::test_windows_send_input_count(2, 2).is_ok());
    let blocked = platform::test_windows_send_input_count(0, 2)
        .expect_err("zero inserted events must be a definite no-effect failure");
    assert!(blocked.starts_with("key_input_failed:"), "{blocked}");
    let partial = platform::test_windows_send_input_count(1, 2)
        .expect_err("partial SendInput must remain uncertain");
    assert!(partial.starts_with("outcome_unknown:"), "{partial}");
    let error = platform::windows_key_input_attempt_error("Windows input deadline");
    assert!(error.starts_with("outcome_unknown:"), "{error}");
}

#[test]
fn computer_windows_key_input_rejects_interfering_keyboard_state() {
    assert!(platform::test_windows_keyboard_state_guard("tab", None).is_ok());
    for down_virtual_key in [0xA0u16, 0xA2, 0xA4, 0x5B, 0x09] {
        let error = platform::test_windows_keyboard_state_guard("tab", Some(down_virtual_key))
            .expect_err("held modifier, Windows key, or target key must fail before SendInput");
        assert!(error.starts_with("key_input_failed:"), "{error}");
    }
}

#[test]
fn computer_windows_text_input_attempt_failure_is_unknown() {
    let error = platform::windows_text_input_attempt_error("IUIAutomationValuePattern::SetValue");
    assert!(error.starts_with("outcome_unknown:"), "{error}");
}

#[test]
#[ignore = "requires an interactive Windows desktop with two observable UIA-backed windows; leaves the activated test window foreground"]
fn computer_windows_window_activation_live_smoke() {
    let candidates = platform::list_windows(MAX_WINDOWS).expect("list live Windows windows");
    let foreground = unsafe { GetForegroundWindow() };
    let original_native_id = candidates
        .iter()
        .find(|candidate| platform::win_hwnd(candidate.native_id).ok() == Some(foreground))
        .map(|candidate| candidate.native_id)
        .expect("current foreground window must have an exact xcap surface");
    let mut failures = Vec::new();

    for candidate in candidates {
        if candidate.native_id == original_native_id {
            continue;
        }
        let target_hwnd = match platform::win_hwnd(candidate.native_id) {
            Ok(hwnd) if hwnd != foreground => hwnd,
            _ => continue,
        };
        let target_record = surface_record(candidate);
        match platform::accessibility_tree("surface_windows_activation_probe", &target_record, 1, 8)
        {
            Ok(tree)
                if tree.output["nodes"]
                    .as_array()
                    .and_then(|nodes| nodes.first())
                    .and_then(|node| node["role"].as_str())
                    == Some("AXWindow") => {}
            Ok(_) => continue,
            Err(error)
                if error.starts_with("stale_surface:")
                    || error.starts_with("accessibility_failed:") =>
            {
                if failures.len() < 8 {
                    failures.push(error);
                }
                continue;
            }
            Err(error) => panic!("unexpected Windows activation preflight error: {error}"),
        }

        let output = platform::activate_window("surface_windows_activation_live", &target_record)
            .expect("activate one exact Windows UIA-backed surface");
        assert_eq!(output["platform"], "windows");
        assert_eq!(output["surface_id"], "surface_windows_activation_live");
        assert_eq!(output["success"], true);
        assert_eq!(unsafe { GetForegroundWindow() }, target_hwnd);
        return;
    }

    panic!("no alternate exact UIA-backed Windows surface was available; failures={failures:?}");
}

#[test]
#[ignore = "requires an interactive Windows desktop; creates and closes a private WinForms control fixture"]
fn computer_windows_control_fixture_live_smoke() {
    let mut fixture = WindowsControlFixture::start();
    let candidate = fixture.wait_for_window(
        WINDOWS_CONTROL_FIXTURE_TITLE,
        "private WinForms control fixture",
    );
    let record = surface_record(candidate);
    let activation = platform::activate_window("surface_windows_control_fixture_activate", &record)
        .expect("activate private WinForms control fixture");
    assert_eq!(activation["success"], true);

    let candidate = platform::list_windows(4096)
        .expect("re-list Windows windows after fixture activation")
        .into_iter()
        .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE)
        .expect("re-observe private WinForms control fixture");
    let record = surface_record(candidate);
    let surface_id = "surface_windows_control_fixture";
    let tree = platform::accessibility_tree(surface_id, &record, 4, 64)
        .expect("read private WinForms control fixture UIA tree");
    let (edit_id, edit) = tree
        .elements
        .iter()
        .find(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == "AXTextField"
                    && fingerprint.has_positive_evidence()
                    && !fingerprint.protected
            })
        })
        .expect("fixture exposes a positively correlated UIA edit");
    let (button_id, button) = tree
        .elements
        .iter()
        .find(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == "AXButton"
                    && fingerprint.has_positive_evidence()
                    && !fingerprint.protected
            })
        })
        .expect("fixture exposes a positively correlated UIA button");

    let edit_state = platform::element_state(surface_id, edit_id, 1, &record, edit)
        .expect("read private edit state");
    assert_eq!(edit_state["can_focus"], true);
    assert_eq!(edit_state["value_empty"], true);
    let focused = platform::control(surface_id, edit_id, &record, edit, ComputerAction::Focus)
        .expect("focus private UIA edit");
    assert_eq!(focused["platform"], "windows");
    assert_eq!(focused["action"], "focus");
    assert_eq!(focused["success"], true);
    let focused_state = platform::element_state(surface_id, edit_id, 1, &record, edit)
        .expect("re-read private edit state");
    assert_eq!(focused_state["focused"], true);
    assert_eq!(focused_state["value_empty"], true);
    assert_eq!(focused_state["can_input_text"], true);

    let password = tree
        .elements
        .iter()
        .find(|(_, element)| {
            element
                .target_fingerprint()
                .is_some_and(|fingerprint| fingerprint.protected)
        })
        .expect("fixture exposes a protected UIA password field");
    let protected_scroll =
        platform::scroll_to_element(surface_id, &password.0, &record, &password.1)
            .expect_err("protected UIA target must fail before scrolling");
    assert!(
        protected_scroll.starts_with("permission_denied:"),
        "{protected_scroll}"
    );

    let text = "webcodex computer smoke";
    let input = platform::input_text(surface_id, edit_id, &record, edit, text)
        .expect("write bounded text through private UIA ValuePattern");
    assert_eq!(input["platform"], "windows");
    assert_eq!(input["surface_id"], surface_id);
    assert_eq!(input["element_id"], edit_id.as_str());
    assert_eq!(input["text_bytes"], text.len());
    assert_eq!(input["success"], true);

    let after_input = platform::element_state(surface_id, edit_id, 1, &record, edit)
        .expect("re-read private edit state after bounded text input");
    assert_eq!(after_input["focused"], true);
    assert_eq!(after_input["value_empty"], false);
    assert_eq!(after_input["can_input_text"], false);
    let second = platform::input_text(surface_id, edit_id, &record, edit, "again")
        .expect_err("bounded Windows text input must not overwrite a non-empty field");
    assert!(second.starts_with("input_failed:"), "{second}");

    let button_state = platform::element_state(surface_id, button_id, 1, &record, button)
        .expect("read private button state");
    assert_eq!(button_state["can_press"], true);
    let pressed = platform::control(
        surface_id,
        button_id,
        &record,
        button,
        ComputerAction::Press,
    )
    .expect("invoke private UIA button");
    assert_eq!(pressed["platform"], "windows");
    assert_eq!(pressed["action"], "press");
    assert_eq!(pressed["success"], true);
    let click_deadline = Instant::now() + Duration::from_secs(1);
    let mut clicked = false;
    while Instant::now() < click_deadline {
        let after_press = platform::accessibility_tree(
            "surface_windows_control_fixture_after_press",
            &record,
            4,
            64,
        )
        .expect("re-observe private WinForms fixture after InvokePattern");
        clicked = after_press.output["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["title"] == "clicked"));
        if clicked {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        clicked,
        "UIA InvokePattern did not update the private fixture"
    );
}

#[test]
#[ignore = "requires an interactive Windows desktop; creates and closes a private scrollable WinForms fixture"]
fn computer_windows_scroll_to_element_fixture_live_smoke() {
    let mut fixture = WindowsControlFixture::start();
    let candidate = fixture.wait_for_window(
        WINDOWS_CONTROL_FIXTURE_TITLE,
        "private WinForms scroll fixture",
    );
    let record = surface_record(candidate);
    platform::activate_window("surface_windows_scroll_fixture_activate", &record)
        .expect("activate private WinForms scroll fixture");

    let candidate = platform::list_windows(4096)
        .expect("re-list Windows windows after scroll fixture activation")
        .into_iter()
        .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE)
        .expect("re-observe private WinForms scroll fixture");
    let record = surface_record(candidate);
    let surface_id = "surface_windows_scroll_fixture";
    let tree = platform::accessibility_tree(surface_id, &record, 5, 128)
        .expect("read private WinForms scroll fixture UIA tree");
    let (target_id, target) = tree
        .elements
        .iter()
        .find(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == "AXRow"
                    && fingerprint.title.as_deref() == Some("Scroll item 39")
                    && !fingerprint.protected
            })
        })
        .expect("fixture exposes the last scroll-list item");
    assert!(
        platform::test_uia_is_offscreen(&record, target).expect("read initial UIA offscreen state"),
        "last fixture item must begin off-screen"
    );

    let button = tree
        .elements
        .iter()
        .find(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == "AXButton"
                    && fingerprint.title.as_deref() == Some("Smoke press")
            })
        })
        .expect("fixture exposes a non-scrollable button");
    let unsupported = platform::scroll_to_element(surface_id, &button.0, &record, &button.1)
        .expect_err("button without ScrollItemPattern must fail before effect");
    assert!(unsupported.starts_with("scroll_failed:"), "{unsupported}");

    let output = platform::scroll_to_element(surface_id, target_id, &record, target)
        .expect("scroll exact off-screen UIA list item into view");
    assert_eq!(output["platform"], "windows");
    assert_eq!(output["surface_id"], surface_id);
    assert_eq!(output["element_id"], target_id.as_str());
    assert_eq!(output["success"], true);

    let refreshed = platform::accessibility_tree(surface_id, &record, 5, 128)
        .expect("re-observe private scroll fixture after ScrollIntoView");
    let (_, refreshed_target) = refreshed
        .elements
        .iter()
        .find(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == "AXRow"
                    && fingerprint.title.as_deref() == Some("Scroll item 39")
            })
        })
        .expect("fresh observation preserves the scrolled fixture item");
    assert!(
        !platform::test_uia_is_offscreen(&record, refreshed_target)
            .expect("read reconciled UIA offscreen state"),
        "ScrollIntoView must reconcile the exact item as visible"
    );
}

#[test]
#[ignore = "requires an interactive Windows desktop; creates and closes only private WinForms key-input fixtures"]
fn computer_windows_key_input_fixture_live_smoke() {
    let mut fixture = WindowsControlFixture::start();
    let candidate = fixture.wait_for_window(
        WINDOWS_CONTROL_FIXTURE_TITLE,
        "private WinForms key-input fixture",
    );
    let record = surface_record(candidate);
    let hwnd = platform::win_hwnd(record.native_id).expect("resolve key fixture HWND");
    let foreground_deadline = Instant::now() + Duration::from_secs(2);
    while unsafe { GetForegroundWindow() } != hwnd {
        assert!(
            Instant::now() < foreground_deadline,
            "private key fixture must self-activate before key-input validation"
        );
        thread::sleep(Duration::from_millis(10));
    }
    platform::activate_window("surface_windows_key_fixture_activate", &record)
        .expect("reconcile already-active private WinForms key-input fixture");

    let candidate = platform::list_windows(4096)
        .expect("re-list Windows windows after key-input fixture activation")
        .into_iter()
        .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE)
        .expect("re-observe private WinForms key-input fixture");
    let record = surface_record(candidate);
    let surface_id = "surface_windows_key_fixture";
    let tree = platform::accessibility_tree(surface_id, &record, 4, 96)
        .expect("read private WinForms key-input fixture UIA tree");
    let (edit_id, edit) = tree
        .elements
        .iter()
        .find(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.identifier.as_deref() == Some("SmokeInput") && !fingerprint.protected
            })
        })
        .expect("fixture exposes exact SmokeInput element");
    platform::control(surface_id, edit_id, &record, edit, ComputerAction::Focus)
        .expect("focus exact private SmokeInput before key input");

    let command = platform::key_input(surface_id, &record, "tab", &["command".to_string()])
        .expect_err("Windows command modifier must fail before SendInput");
    assert!(command.starts_with("key_input_failed:"), "{command}");
    let after_command = platform::accessibility_tree(surface_id, &record, 4, 96)
        .expect("re-observe fixture after rejected command modifier");
    assert!(
        after_command.output["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes
                .iter()
                .any(|node| { node["focused"] == true && node["title"] == "Smoke input" })),
        "rejected command modifier must not move focus"
    );

    let tab = platform::key_input(surface_id, &record, "tab", &[])
        .expect("send one closed Tab to the exact foreground fixture");
    assert_eq!(tab["platform"], "windows");
    assert_eq!(tab["surface_id"], surface_id);
    assert_eq!(tab["key"], "tab");
    assert_eq!(tab["modifiers"], json!([]));
    assert_eq!(tab["success"], true);

    let button_focus_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let refreshed = platform::accessibility_tree(surface_id, &record, 4, 96)
            .expect("re-observe fixture after Tab");
        if refreshed.output["nodes"].as_array().is_some_and(|nodes| {
            nodes
                .iter()
                .any(|node| node["focused"] == true && node["title"] == "Smoke press")
        }) {
            break;
        }
        assert!(
            Instant::now() < button_focus_deadline,
            "Tab must move focus to the fixture-only button"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let shift_tab = platform::key_input(surface_id, &record, "tab", &["shift".to_string()])
        .expect("send Shift+Tab with bounded modifier lifetime");
    assert_eq!(shift_tab["modifiers"], json!(["shift"]));
    let input_focus_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let refreshed = platform::accessibility_tree(surface_id, &record, 4, 96)
            .expect("re-observe fixture after Shift+Tab");
        if refreshed.output["nodes"].as_array().is_some_and(|nodes| {
            nodes
                .iter()
                .any(|node| node["focused"] == true && node["title"] == "Smoke input")
        }) {
            break;
        }
        assert!(
            Instant::now() < input_focus_deadline,
            "Shift+Tab must return focus to the fixture input"
        );
        thread::sleep(Duration::from_millis(10));
    }

    platform::key_input(surface_id, &record, "tab", &[])
        .expect("return focus to the fixture button");
    platform::key_input(surface_id, &record, "enter", &[])
        .expect("send Enter to the fixture-only focused button");
    let click_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let refreshed = platform::accessibility_tree(surface_id, &record, 4, 96)
            .expect("re-observe fixture after Enter");
        if refreshed.output["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["title"] == "clicked"))
        {
            break;
        }
        assert!(
            Instant::now() < click_deadline,
            "Enter must invoke only the fixture-local focused button"
        );
        thread::sleep(Duration::from_millis(10));
    }

    platform::key_input(surface_id, &record, "tab", &[])
        .expect("move fixture focus from button to password field");
    let protected_focus_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let refreshed = platform::accessibility_tree(surface_id, &record, 4, 96)
            .expect("re-observe fixture after moving to password field");
        let nodes = refreshed.output["nodes"]
            .as_array()
            .expect("fixture observation nodes are an array");
        let protected_focused =
            refreshed
                .elements
                .iter()
                .zip(nodes.iter())
                .any(|((_, element), node)| {
                    element
                        .target_fingerprint()
                        .is_some_and(|fingerprint| fingerprint.protected)
                        && node["focused"] == true
                });
        if protected_focused {
            break;
        }
        assert!(
            Instant::now() < protected_focus_deadline,
            "Tab must reach the fixture password field"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let protected = platform::key_input(surface_id, &record, "escape", &[])
        .expect_err("protected focused content must reject key input before SendInput");
    assert!(protected.starts_with("permission_denied:"), "{protected}");

    let mut foreground_probe = WindowsControlFixture::start_foreground_probe();
    let probe_candidate = foreground_probe
        .wait_for_window(WINDOWS_FOREGROUND_PROBE_TITLE, "private foreground probe");
    let probe_record = surface_record(probe_candidate);
    let probe_hwnd =
        platform::win_hwnd(probe_record.native_id).expect("resolve foreground probe HWND");
    let probe_foreground_deadline = Instant::now() + Duration::from_secs(2);
    while unsafe { GetForegroundWindow() } != probe_hwnd {
        assert!(
            Instant::now() < probe_foreground_deadline,
            "private foreground probe must self-activate before background rejection"
        );
        thread::sleep(Duration::from_millis(10));
    }
    platform::activate_window("surface_windows_key_foreground_probe", &probe_record)
        .expect("reconcile already-active private foreground probe");

    let outside = platform::test_windows_focused_element_belongs_to_surface(&record)
        .expect_err("focus in another private root must fail exact-root ancestry");
    assert!(outside.starts_with("key_input_failed:"), "{outside}");
    let background = platform::key_input(surface_id, &record, "escape", &[])
        .expect_err("background exact surface must fail before SendInput");
    assert!(background.starts_with("key_input_failed:"), "{background}");
}

#[test]
#[ignore = "requires an interactive Windows desktop; creates and replaces indistinguishable private WinForms controls"]
fn computer_windows_uia_stale_identity_rejects_indistinguishable_replacement_live() {
    let mut fixture = WindowsControlFixture::start();
    let candidate = fixture.wait_for_window(
        WINDOWS_CONTROL_FIXTURE_TITLE,
        "private WinForms identity fixture",
    );
    let record = surface_record(candidate);
    platform::activate_window("surface_windows_identity_fixture_activate", &record)
        .expect("activate private WinForms identity fixture");

    let candidate = platform::list_windows(4096)
        .expect("re-list Windows windows after identity fixture activation")
        .into_iter()
        .find(|candidate| candidate.title == WINDOWS_CONTROL_FIXTURE_TITLE)
        .expect("re-observe private WinForms identity fixture");
    let record = surface_record(candidate);
    let surface_id = "surface_windows_identity_fixture";
    let tree = platform::accessibility_tree(surface_id, &record, 4, 96)
        .expect("read private WinForms identity fixture UIA tree");
    let twins = tree
        .elements
        .iter()
        .filter(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == "AXButton"
                    && fingerprint.identifier.as_deref() == Some("TwinAction")
                    && fingerprint.title.as_deref() == Some("Twin action")
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        twins.len(),
        2,
        "fixture must expose two indistinguishable twins"
    );
    let (old_twin_id, old_twin) = twins[0];
    let old_twin_id = old_twin_id.clone();
    let old_twin = old_twin.clone();
    let old_target = old_twin
        .target_fingerprint()
        .expect("old twin has complete lineage")
        .clone();
    let (replace_id, replace) = tree
        .elements
        .iter()
        .find(|(_, element)| {
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == "AXButton"
                    && fingerprint.identifier.as_deref() == Some("ReplaceTwins")
            })
        })
        .expect("fixture exposes the replace-twins trigger");
    platform::control(
        surface_id,
        replace_id,
        &record,
        replace,
        ComputerAction::Press,
    )
    .expect("replace both indistinguishable twins through the private fixture");

    let replacement_deadline = Instant::now() + Duration::from_secs(1);
    let mut replacement_observed = false;
    while Instant::now() < replacement_deadline {
        let refreshed = platform::accessibility_tree(surface_id, &record, 4, 96)
            .expect("re-observe private fixture after twin replacement");
        replacement_observed = refreshed.elements.iter().any(|(_, element)| {
            if element.path != old_twin.path {
                return false;
            }
            element.target_fingerprint().is_some_and(|fingerprint| {
                fingerprint.role == old_target.role
                    && fingerprint.subrole == old_target.subrole
                    && fingerprint.identifier == old_target.identifier
                    && fingerprint.title == old_target.title
                    && fingerprint.description == old_target.description
                    && fingerprint.placeholder == old_target.placeholder
                    && fingerprint.protected == old_target.protected
                    && fingerprint.native_runtime_id != old_target.native_runtime_id
            })
        });
        if replacement_observed {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        replacement_observed,
        "replacement must preserve the old semantic path while changing native UIA identity"
    );

    let stale_state = platform::element_state(surface_id, &old_twin_id, 1, &record, &old_twin)
        .expect_err("old element state handle must not retarget the replacement twin");
    assert!(stale_state.starts_with("stale_element:"), "{stale_state}");
    let stale_control = platform::control(
        surface_id,
        &old_twin_id,
        &record,
        &old_twin,
        ComputerAction::Press,
    )
    .expect_err("old effect handle must fail before invoking the replacement twin");
    assert!(
        stale_control.starts_with("stale_element:"),
        "{stale_control}"
    );
    let stale_scroll = platform::scroll_to_element(surface_id, &old_twin_id, &record, &old_twin)
        .expect_err("old scroll handle must not retarget the replacement twin");
    assert!(stale_scroll.starts_with("stale_element:"), "{stale_scroll}");
    let after = platform::accessibility_tree(surface_id, &record, 4, 96)
        .expect("re-observe fixture after rejected stale control");
    assert!(
        after.output["nodes"].as_array().is_some_and(|nodes| nodes
            .iter()
            .all(|node| node["title"] != "wrong target invoked")),
        "stale control must not invoke either indistinguishable replacement"
    );
}

#[test]
#[ignore = "requires an interactive Windows desktop with at least one UIA-accessible window"]
fn computer_windows_uia_live_smoke() {
    let status = platform::accessibility_status().expect("initialize Windows UI Automation");
    assert_eq!(status["platform"], "windows");
    assert_eq!(status["trusted"], true);

    let candidates = platform::list_windows(MAX_WINDOWS).expect("list live Windows windows");
    let mut failures = Vec::new();
    for candidate in candidates {
        let record = surface_record(candidate);
        match platform::accessibility_tree("surface_windows_live", &record, 3, 64) {
            Ok(tree) => {
                assert_eq!(tree.output["platform"], "windows");
                assert_eq!(tree.output["surface_id"], "surface_windows_live");
                assert!(tree.output["node_count"].as_u64().unwrap_or(0) > 0);
                assert!(!tree.elements.is_empty());
                let Some((element_id, element)) = tree.elements.iter().find(|(_, element)| {
                    element
                        .target_fingerprint()
                        .is_some_and(ElementFingerprint::has_positive_evidence)
                }) else {
                    if failures.len() < 8 {
                        failures.push(
                            "accessibility_failed: live UIA tree had no positively correlated element"
                                .to_string(),
                        );
                    }
                    continue;
                };
                match platform::element_state(
                    "surface_windows_live",
                    element_id,
                    1,
                    &record,
                    element,
                ) {
                    Ok(state) => {
                        assert_eq!(state["platform"], "windows");
                        assert_eq!(state["surface_id"], "surface_windows_live");
                        assert_eq!(state["element_id"], element_id.as_str());
                        assert!(state["can_press"].is_boolean());
                        assert!(state["can_focus"].is_boolean());
                        assert!(state["can_input_text"].is_boolean());
                        return;
                    }
                    Err(error) => {
                        if failures.len() < 8 && !failures.contains(&error) {
                            failures.push(error);
                        }
                        continue;
                    }
                }
            }
            Err(error) => {
                if failures.len() < 8 && !failures.contains(&error) {
                    failures.push(error);
                }
                continue;
            }
        }
    }
    panic!(
        "no bounded observable window exposed a Windows UIA Control View root; errors={failures:?}"
    );
}
