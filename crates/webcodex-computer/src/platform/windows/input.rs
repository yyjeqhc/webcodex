use super::*;

#[cfg(windows)]
pub(crate) struct PointerCoordinateContext {
    previous: DPI_AWARENESS_CONTEXT,
}

#[cfg(windows)]
impl Drop for PointerCoordinateContext {
    fn drop(&mut self) {
        let restored = unsafe { SetThreadDpiAwarenessContext(self.previous) };
        debug_assert!(!restored.0.is_null());
    }
}

#[cfg(windows)]
pub(crate) fn enter_pointer_coordinate_context() -> Result<PointerCoordinateContext, String> {
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        return Err(
            "pointer_input_failed: Windows per-monitor DPI coordinate context is unavailable"
                .to_string(),
        );
    }
    Ok(PointerCoordinateContext { previous })
}

#[cfg(windows)]
fn normalize_pointer_axis(offset: u32, extent: u32) -> Result<i32, String> {
    if extent == 0 || offset >= extent {
        return Err(
            "pointer_input_failed: pointer coordinate is outside virtual desktop bounds"
                .to_string(),
        );
    }
    if extent == 1 {
        return Ok(0);
    }
    // A 16-bit normalized axis cannot uniquely address more than 65,536 pixels.
    if extent > 65_536 {
        return Err("pointer_input_failed: virtual desktop axis exceeds exact absolute-input addressability".to_string());
    }
    let denominator = u64::from(extent - 1);
    let normalized = (u64::from(offset) * 65_535 + denominator / 2) / denominator;
    i32::try_from(normalized)
        .map_err(|_| "pointer_input_failed: normalized pointer coordinate is invalid".to_string())
}

#[cfg(windows)]
fn map_windows_pointer_coordinate(
    monitor_x: i32,
    monitor_y: i32,
    source_width: u32,
    source_height: u32,
    virtual_left: i32,
    virtual_top: i32,
    virtual_width: u32,
    virtual_height: u32,
    x: u32,
    y: u32,
) -> Result<PointerPlan, String> {
    if x >= source_width || y >= source_height {
        return Err(
            "invalid_request: pointer coordinate is outside snapshot source geometry".to_string(),
        );
    }
    let global_x = i64::from(monitor_x)
        .checked_add(i64::from(x))
        .ok_or_else(|| "pointer_input_failed: global x coordinate overflowed".to_string())?;
    let global_y = i64::from(monitor_y)
        .checked_add(i64::from(y))
        .ok_or_else(|| "pointer_input_failed: global y coordinate overflowed".to_string())?;
    let offset_x = global_x - i64::from(virtual_left);
    let offset_y = global_y - i64::from(virtual_top);
    if offset_x < 0
        || offset_y < 0
        || offset_x >= i64::from(virtual_width)
        || offset_y >= i64::from(virtual_height)
    {
        return Err(
            "pointer_input_failed: exact display lies outside Windows virtual desktop bounds"
                .to_string(),
        );
    }
    Ok(PointerPlan {
        global_x: i32::try_from(global_x)
            .map_err(|_| "pointer_input_failed: global x coordinate is invalid".to_string())?,
        global_y: i32::try_from(global_y)
            .map_err(|_| "pointer_input_failed: global y coordinate is invalid".to_string())?,
        normalized_x: normalize_pointer_axis(u32::try_from(offset_x).unwrap(), virtual_width)?,
        normalized_y: normalize_pointer_axis(u32::try_from(offset_y).unwrap(), virtual_height)?,
    })
}

#[cfg(windows)]
fn validate_windows_pointer_state_with(
    action: PointerAction,
    mut is_down: impl FnMut(VIRTUAL_KEY) -> bool,
) -> Result<(), String> {
    for key in [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2] {
        if is_down(key) {
            return Err(
                "pointer_input_failed: shared desktop mouse button is already down".to_string(),
            );
        }
    }
    if action == PointerAction::Click {
        for key in [
            VK_SHIFT,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_CONTROL,
            VK_LCONTROL,
            VK_RCONTROL,
            VK_MENU,
            VK_LMENU,
            VK_RMENU,
            VK_LWIN,
            VK_RWIN,
        ] {
            if is_down(key) {
                return Err(
                    "pointer_input_failed: modifier or Windows key is already down".to_string(),
                );
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_pointer_state(action: PointerAction) -> Result<(), String> {
    validate_windows_pointer_state_with(action, |key| unsafe {
        GetAsyncKeyState(i32::from(key.0)) < 0
    })
}

#[cfg(windows)]
fn validate_windows_pointer_coordinate_spaces(
    virtual_metrics: (i32, i32, u32, u32),
    xcap_bounds: (i32, i32, u32, u32),
) -> Result<(), String> {
    if virtual_metrics == xcap_bounds {
        Ok(())
    } else {
        Err("pointer_input_failed: Windows DPI/topology coordinate spaces cannot be proven identical".to_string())
    }
}

#[cfg(windows)]
pub(crate) fn prepare_pointer(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
) -> Result<PointerPlan, String> {
    let monitor = find_exact_display(display)?;
    let (monitor_x, monitor_y, monitor_width, monitor_height) = windows_monitor_rect(&monitor)?;
    if monitor_width != display.width || monitor_height != display.height {
        return Err(
            "stale_display: native display source geometry changed before pointer input"
                .to_string(),
        );
    }
    let virtual_metrics = windows_virtual_desktop_metrics()?;
    let xcap_bounds = windows_xcap_virtual_bounds()?;
    validate_windows_pointer_coordinate_spaces(virtual_metrics, xcap_bounds)?;
    let plan = map_windows_pointer_coordinate(
        monitor_x,
        monitor_y,
        display.width,
        display.height,
        virtual_metrics.0,
        virtual_metrics.1,
        virtual_metrics.2,
        virtual_metrics.3,
        x,
        y,
    )?;
    let fresh = find_exact_display(display)?;
    let fresh_rect = windows_monitor_rect(&fresh)?;
    if fresh_rect != (monitor_x, monitor_y, monitor_width, monitor_height) {
        return Err(
            "stale_display: native display placement changed during pointer preflight".to_string(),
        );
    }
    validate_windows_pointer_state(action)?;
    Ok(plan)
}

#[cfg(windows)]
fn windows_mouse_input(plan: PointerPlan, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: plan.normalized_x,
                dy: plan.normalized_y,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn windows_pointer_move_inputs(plan: PointerPlan) -> [INPUT; 1] {
    let move_flags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
    [windows_mouse_input(plan, move_flags)]
}

#[cfg(windows)]
fn windows_pointer_click_button_inputs(plan: PointerPlan) -> [INPUT; 2] {
    [
        windows_mouse_input(plan, MOUSEEVENTF_LEFTDOWN),
        windows_mouse_input(plan, MOUSEEVENTF_LEFTUP),
    ]
}

#[cfg(windows)]
fn validate_windows_pointer_move_send_input_count(inserted: u32) -> Result<(), String> {
    if inserted == 1 {
        Ok(())
    } else if inserted == 0 {
        Err("not_started: Windows pointer move SendInput inserted no events".to_string())
    } else {
        Err(format!(
            "outcome_unknown: Windows pointer move SendInput reported {inserted} inserted events for one prepared move"
        ))
    }
}

#[cfg(windows)]
fn validate_windows_pointer_button_send_input_count(inserted: u32) -> Result<(), String> {
    if inserted == 2 {
        Ok(())
    } else {
        Err(format!(
            "outcome_unknown: Windows pointer click button SendInput inserted {inserted} of 2 events after the exact move"
        ))
    }
}

#[cfg(windows)]
fn validate_windows_pointer_postcondition(
    plan: PointerPlan,
    action: PointerAction,
    cursor_x: i32,
    cursor_y: i32,
    left_button_down: bool,
) -> Result<(), String> {
    if cursor_x != plan.global_x || cursor_y != plan.global_y {
        return Err("outcome_unknown: Windows pointer position postcondition could not prove the exact target".to_string());
    }
    if action == PointerAction::Click && left_button_down {
        return Err(
            "outcome_unknown: Windows left mouse button remained down after click sequence"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn dispatch_windows_pointer_with(
    plan: PointerPlan,
    action: PointerAction,
    mut send_input: impl FnMut(&[INPUT]) -> u32,
    mut cursor_position: impl FnMut() -> Result<(i32, i32), String>,
    mut validate_click_state: impl FnMut() -> Result<(), String>,
    mut left_button_down: impl FnMut() -> bool,
) -> Result<bool, String> {
    let move_inputs = windows_pointer_move_inputs(plan);
    validate_windows_pointer_move_send_input_count(send_input(&move_inputs))?;
    let (cursor_x, cursor_y) = cursor_position()?;
    validate_windows_pointer_postcondition(plan, PointerAction::Move, cursor_x, cursor_y, false)?;
    if action == PointerAction::Move {
        return Ok(true);
    }

    if validate_click_state().is_err() {
        return Err(
            "outcome_unknown: shared desktop input state changed after the exact pointer move; click button events were not attempted"
                .to_string(),
        );
    }
    let button_inputs = windows_pointer_click_button_inputs(plan);
    validate_windows_pointer_button_send_input_count(send_input(&button_inputs))?;
    let (cursor_x, cursor_y) = cursor_position()?;
    validate_windows_pointer_postcondition(
        plan,
        PointerAction::Click,
        cursor_x,
        cursor_y,
        left_button_down(),
    )?;
    Ok(true)
}

#[cfg(windows)]
pub(crate) fn dispatch_pointer(plan: PointerPlan, action: PointerAction) -> Result<bool, String> {
    let input_size = std::mem::size_of::<INPUT>() as i32;
    dispatch_windows_pointer_with(
        plan,
        action,
        |inputs| unsafe { SendInput(inputs, input_size) },
        || {
            let mut point = POINT::default();
            unsafe { GetCursorPos(&mut point) }.map_err(|_| {
                "outcome_unknown: Windows cursor position postcondition is unavailable".to_string()
            })?;
            Ok((point.x, point.y))
        },
        || validate_windows_pointer_state(PointerAction::Click),
        || unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) < 0 },
    )
}

#[cfg(windows)]
fn windows_key_mapping(key: &str) -> Result<(VIRTUAL_KEY, bool), String> {
    match key {
        "enter" => Ok((VK_RETURN, false)),
        "escape" => Ok((VK_ESCAPE, false)),
        "tab" => Ok((VK_TAB, false)),
        "arrow_up" => Ok((VK_UP, true)),
        "arrow_down" => Ok((VK_DOWN, true)),
        "arrow_left" => Ok((VK_LEFT, true)),
        "arrow_right" => Ok((VK_RIGHT, true)),
        "page_up" => Ok((VK_PRIOR, true)),
        "page_down" => Ok((VK_NEXT, true)),
        "home" => Ok((VK_HOME, true)),
        "end" => Ok((VK_END, true)),
        _ => Err("invalid_request: computer key is outside the closed vocabulary".to_string()),
    }
}

#[cfg(windows)]
fn windows_modifier_key(modifier: &str) -> Result<VIRTUAL_KEY, String> {
    match modifier {
        "shift" => Ok(VK_SHIFT),
        "control" => Ok(VK_CONTROL),
        "option" => Ok(VK_MENU),
        "command" => Err(
            "key_input_failed: command modifier has no safe Windows mapping in this closed input slice"
                .to_string(),
        ),
        _ => Err("invalid_request: computer key input modifier is outside the closed vocabulary".to_string()),
    }
}

#[cfg(windows)]
fn validate_windows_key_input_chord(key: &str, modifiers: &[String]) -> Result<(), String> {
    let has_option = modifiers.iter().any(|modifier| modifier == "option");
    let has_control = modifiers.iter().any(|modifier| modifier == "control");
    let escapes_exact_surface =
        (has_option && matches!(key, "tab" | "escape")) || (has_control && key == "escape");
    if escapes_exact_surface {
        return Err(
            "key_input_failed: Windows system-level key chord is outside the exact-surface input contract"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_keyboard_state_with<F>(key: &str, mut is_down: F) -> Result<(), String>
where
    F: FnMut(VIRTUAL_KEY) -> bool,
{
    let (key_code, _) = windows_key_mapping(key)?;
    for candidate in [
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
        key_code,
    ] {
        if is_down(candidate) {
            return Err(
                "key_input_failed: Windows keyboard state is not neutral; release held Shift/Control/Alt/Windows/target keys and re-observe before retrying"
                    .to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_keyboard_state(key: &str) -> Result<(), String> {
    validate_windows_keyboard_state_with(key, |candidate| unsafe {
        GetAsyncKeyState(i32::from(candidate.0)) < 0
    })
}

#[cfg(windows)]
fn windows_keyboard_input(key: VIRTUAL_KEY, key_up: bool, extended: bool) -> INPUT {
    let flags = KEYBD_EVENT_FLAGS(
        if extended { KEYEVENTF_EXTENDEDKEY.0 } else { 0 }
            | if key_up { KEYEVENTF_KEYUP.0 } else { 0 },
    );
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn windows_key_input_plan(key: &str, modifiers: &[String]) -> Result<Vec<INPUT>, String> {
    validate_key_input(key, modifiers)?;
    let (key_code, key_extended) = windows_key_mapping(key)?;
    let mut modifier_keys = Vec::with_capacity(modifiers.len());
    for modifier in modifiers {
        modifier_keys.push(windows_modifier_key(modifier)?);
    }
    validate_windows_key_input_chord(key, modifiers)?;

    let mut inputs = Vec::with_capacity(modifier_keys.len() * 2 + 2);
    for modifier in &modifier_keys {
        inputs.push(windows_keyboard_input(*modifier, false, false));
    }
    inputs.push(windows_keyboard_input(key_code, false, key_extended));
    inputs.push(windows_keyboard_input(key_code, true, key_extended));
    for modifier in modifier_keys.iter().rev() {
        inputs.push(windows_keyboard_input(*modifier, true, false));
    }
    Ok(inputs)
}

#[cfg(windows)]
pub(crate) fn windows_key_input_attempt_error(operation: &str) -> String {
    format!("outcome_unknown: {operation} returned after Windows native key input was attempted")
}

#[cfg(windows)]
fn validate_windows_send_input_count(inserted: u32, expected: u32) -> Result<(), String> {
    if inserted == expected {
        Ok(())
    } else if inserted == 0 {
        Err(format!(
            "key_input_failed: SendInput inserted 0 of {expected} prepared keyboard events; no keyboard event was inserted"
        ))
    } else {
        Err(windows_key_input_attempt_error(&format!(
            "SendInput inserted {inserted} of {expected} prepared keyboard events"
        )))
    }
}

#[cfg(windows)]
pub(crate) fn windows_text_input_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} returned after the exact Windows UI Automation text write was attempted"
    )
}

#[cfg(windows)]
pub(crate) fn key_input(
    surface_id: &str,
    surface: &SurfaceRecord,
    key: &str,
    modifiers: &[String],
) -> Result<Value, String> {
    validate_key_input(key, modifiers)?;
    let context = UiaContext::new()?;

    // Prove exact foreground/focus ownership before preparing the native input.
    let root = exact_uia_window(&context, surface)?;
    validate_windows_key_input_target(&context, surface, &root)?;

    // Prepare the complete bounded input sequence before the first native effect.
    // `command` deliberately fails here instead of being mapped to the Windows key.
    let inputs = windows_key_input_plan(key, modifiers)?;
    let expected_count = u32::try_from(inputs.len())
        .map_err(|_| "key_input_failed: Windows key input sequence is too large".to_string())?;
    let input_size = i32::try_from(std::mem::size_of::<INPUT>())
        .map_err(|_| "key_input_failed: Windows INPUT size is invalid".to_string())?;

    // Revalidate the exact surface/root and focus as close to SendInput as practical.
    let root = exact_uia_window(&context, surface)?;
    validate_windows_key_input_target(&context, surface, &root)?;
    context.deadline.ensure_remaining()?;
    // SendInput shares the interactive desktop's keyboard state. Reject the
    // physical modifier/target states that can turn this closed request into
    // a different chord or leave the model racing an already-held key.
    validate_windows_keyboard_state(key)?;

    let inserted = unsafe { SendInput(&inputs, input_size) };
    validate_windows_send_input_count(inserted, expected_count)?;
    if let Err(error) = context.deadline.ensure_remaining() {
        return Err(windows_key_input_attempt_error(&error));
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "key": key,
        "modifiers": modifiers,
        "success": true,
    }))
}

#[cfg(windows)]
pub(crate) fn input_text(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    text: &str,
) -> Result<Value, String> {
    let text_bytes = validate_input_text(text)?;
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: Windows UI Automation protected content cannot receive text input"
                .to_string(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for text input"
                .to_string(),
        );
    }
    if !uia_semantic_text_input_role(&target.role) {
        return Err(
            "input_failed: UI Automation element is outside the bounded Windows text-entry role set"
                .to_string(),
        );
    }

    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    let enabled = unsafe { current.CurrentIsEnabled() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
        .as_bool();
    if !enabled {
        return Err("input_failed: UI Automation text element is disabled".to_string());
    }
    let pattern = uia_text_pattern(&context, &current)?.ok_or_else(|| {
        "input_failed: UI Automation text element does not expose ValuePattern".to_string()
    })?;
    if !uia_value_pattern_writable(&context, &pattern)? {
        return Err("input_failed: UI Automation ValuePattern is read-only".to_string());
    }

    let hwnd = win_hwnd(surface.native_id)?;
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(
            "input_failed: exact Windows surface must already be foreground before text input"
                .to_string(),
        );
    }
    if !uia_element_has_exact_focus(&context, &current)? {
        return Err(
            "input_failed: exact Windows text element must already have keyboard focus".to_string(),
        );
    }

    // Keep emptiness as the final state read before the native write. The
    // value never leaves the Runner; only the empty/non-empty affordance is
    // exposed through element_state.
    let current_value = uia_value_pattern_current_value(&context, &pattern)?;
    if !current_value.is_empty() {
        return Err(
            "input_failed: UI Automation ValuePattern must be empty before bounded text input; observe and reconcile before retrying"
                .to_string(),
        );
    }

    let value = windows::core::BSTR::from(text);
    context.deadline.ensure_remaining()?;
    if let Err(error) = unsafe { pattern.SetValue(&value) } {
        return Err(windows_text_input_attempt_error(&format!(
            "IUIAutomationValuePattern::SetValue HRESULT(0x{:08X})",
            error.code().0 as u32
        )));
    }
    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "element_id": element_id,
        "text_bytes": text_bytes,
        "success": true,
    }))
}
#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_map(
    monitor_x: i32,
    monitor_y: i32,
    source_width: u32,
    source_height: u32,
    virtual_left: i32,
    virtual_top: i32,
    virtual_width: u32,
    virtual_height: u32,
    x: u32,
    y: u32,
) -> Result<(i32, i32, i32, i32), String> {
    map_windows_pointer_coordinate(
        monitor_x,
        monitor_y,
        source_width,
        source_height,
        virtual_left,
        virtual_top,
        virtual_width,
        virtual_height,
        x,
        y,
    )
    .map(|plan| {
        (
            plan.global_x,
            plan.global_y,
            plan.normalized_x,
            plan.normalized_y,
        )
    })
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_dpi_context_metrics() -> Result<
    (
        (i32, i32, u32, u32),
        (i32, i32, u32, u32),
        (i32, i32, u32, u32),
        (i32, i32, u32, u32),
    ),
    String,
> {
    let before = windows_virtual_desktop_metrics()?;
    let (during, xcap_bounds) = {
        let _context = enter_pointer_coordinate_context()?;
        (
            windows_virtual_desktop_metrics()?,
            windows_xcap_virtual_bounds()?,
        )
    };
    let after = windows_virtual_desktop_metrics()?;
    Ok((before, during, xcap_bounds, after))
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_coordinate_spaces(
    virtual_metrics: (i32, i32, u32, u32),
    xcap_bounds: (i32, i32, u32, u32),
) -> Result<(), String> {
    validate_windows_pointer_coordinate_spaces(virtual_metrics, xcap_bounds)
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_state_guard(
    action: PointerAction,
    down_virtual_key: Option<u16>,
) -> Result<(), String> {
    validate_windows_pointer_state_with(action, |candidate| down_virtual_key == Some(candidate.0))
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_move_send_input_count(inserted: u32) -> Result<(), String> {
    validate_windows_pointer_move_send_input_count(inserted)
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_button_send_input_count(inserted: u32) -> Result<(), String> {
    validate_windows_pointer_button_send_input_count(inserted)
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_postcondition(
    global_x: i32,
    global_y: i32,
    cursor_x: i32,
    cursor_y: i32,
    action: PointerAction,
    left_button_down: bool,
) -> Result<(), String> {
    validate_windows_pointer_postcondition(
        PointerPlan {
            global_x,
            global_y,
            normalized_x: 0,
            normalized_y: 0,
        },
        action,
        cursor_x,
        cursor_y,
        left_button_down,
    )
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_input_flags(action: PointerAction) -> Vec<u32> {
    let plan = PointerPlan {
        global_x: 10,
        global_y: 20,
        normalized_x: 100,
        normalized_y: 200,
    };
    let mut flags: Vec<u32> = windows_pointer_move_inputs(plan)
        .iter()
        .map(|input| unsafe { input.Anonymous.mi.dwFlags.0 })
        .collect();
    if action == PointerAction::Click {
        flags.extend(
            windows_pointer_click_button_inputs(plan)
                .iter()
                .map(|input| unsafe { input.Anonymous.mi.dwFlags.0 }),
        );
    }
    flags
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_pointer_dispatch_trace(
    action: PointerAction,
    move_inserted: u32,
    first_cursor: (i32, i32),
    click_state_down_virtual_key: Option<u16>,
    button_inserted: u32,
    final_cursor: (i32, i32),
    final_left_button_down: bool,
) -> (Result<bool, String>, Vec<Vec<u32>>, usize) {
    let plan = PointerPlan {
        global_x: 10,
        global_y: 20,
        normalized_x: 100,
        normalized_y: 200,
    };
    let mut send_calls = 0usize;
    let mut sent_flags = Vec::new();
    let mut cursor_calls = 0usize;
    let mut click_state_checks = 0usize;
    let result = dispatch_windows_pointer_with(
        plan,
        action,
        |inputs| {
            sent_flags.push(
                inputs
                    .iter()
                    .map(|input| unsafe { input.Anonymous.mi.dwFlags.0 })
                    .collect(),
            );
            let inserted = if send_calls == 0 {
                move_inserted
            } else {
                button_inserted
            };
            send_calls += 1;
            inserted
        },
        || {
            let point = if cursor_calls == 0 {
                first_cursor
            } else {
                final_cursor
            };
            cursor_calls += 1;
            Ok(point)
        },
        || {
            click_state_checks += 1;
            validate_windows_pointer_state_with(PointerAction::Click, |candidate| {
                click_state_down_virtual_key == Some(candidate.0)
            })
        },
        || final_left_button_down,
    );
    (result, sent_flags, click_state_checks)
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_key_input_plan(
    key: &str,
    modifiers: &[String],
) -> Result<Vec<(u16, bool, bool)>, String> {
    windows_key_input_plan(key, modifiers).map(|inputs| {
        inputs
            .iter()
            .map(|input| {
                let keyboard = unsafe { input.Anonymous.ki };
                (
                    keyboard.wVk.0,
                    keyboard.dwFlags.contains(KEYEVENTF_KEYUP),
                    keyboard.dwFlags.contains(KEYEVENTF_EXTENDEDKEY),
                )
            })
            .collect()
    })
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_send_input_count(inserted: u32, expected: u32) -> Result<(), String> {
    validate_windows_send_input_count(inserted, expected)
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_keyboard_state_guard(
    key: &str,
    down_virtual_key: Option<u16>,
) -> Result<(), String> {
    validate_windows_keyboard_state_with(key, |candidate| down_virtual_key == Some(candidate.0))
}
