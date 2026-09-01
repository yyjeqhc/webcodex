use super::*;

#[cfg(target_os = "macos")]
const MAX_AX_WINDOWS: usize = 64;
#[cfg(target_os = "macos")]
const MAX_AX_CHILD_COUNT: usize = 1_000_000;
#[cfg(target_os = "macos")]
const MAX_AX_ACTION_NAMES: usize = 64;

#[cfg(target_os = "macos")]
fn accessibility_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else {
        format!(
            "accessibility_failed: {operation} failed with AXError({})",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
pub(super) fn checked_surface_pid(surface: &SurfaceRecord) -> Result<libc::pid_t, String> {
    libc::pid_t::try_from(surface.pid)
        .map_err(|_| "stale_surface: surface PID exceeds native range".to_string())
}

#[cfg(target_os = "macos")]
fn control_attempt_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::ActionUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "control_failed: {operation} was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
fn scroll_attempt_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::ActionUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "scroll_failed: {operation} was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
            error.0
        )
    }
}

#[cfg(all(test, target_os = "macos"))]
mod scroll_attempt_tests {
    use super::*;

    #[test]
    fn rejected_scroll_action_is_definite_but_unclassified_native_error_is_unknown() {
        let rejected = scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            AXError::ActionUnsupported,
        );
        assert!(rejected.starts_with("scroll_failed:"), "{rejected}");

        let uncertain = scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            AXError::NoValue,
        );
        assert!(uncertain.starts_with("outcome_unknown:"), "{uncertain}");
    }
}

#[cfg(target_os = "macos")]
fn window_activation_attempt_error(
    operation: &str,
    error: AXError,
    prior_effect_succeeded: bool,
) -> String {
    if prior_effect_succeeded {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after application activation had already succeeded",
            error.0
        )
    } else {
        control_attempt_error(operation, error)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod window_activation_tests {
    use super::*;

    #[test]
    fn partial_window_activation_failure_is_always_outcome_unknown() {
        let partial = window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            AXError::ActionUnsupported,
            true,
        );
        assert!(partial.starts_with("outcome_unknown:"), "{partial}");

        let not_started = window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            AXError::ActionUnsupported,
            false,
        );
        assert!(not_started.starts_with("control_failed:"), "{not_started}");
    }
}

#[cfg(target_os = "macos")]
pub(super) fn prepare_ax_call(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
) -> Result<(), String> {
    let timeout_secs = deadline.remaining_timeout_secs()?;
    let error = unsafe { element.set_messaging_timeout(timeout_secs) };
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementSetMessagingTimeout", error));
    }
    deadline.ensure_remaining()
}

#[cfg(target_os = "macos")]
fn optional_ax_value(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CFRetained<CFType>>, String> {
    let attribute = CFString::from_static_str(attribute);
    let mut raw: *const CFType = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.copy_attribute_value(&attribute, NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => {
            let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
                "accessibility_failed: AX attribute succeeded with null value".to_string()
            })?;
            Ok(Some(unsafe { CFRetained::from_raw(raw) }))
        }
        AXError::AttributeUnsupported | AXError::NoValue => Ok(None),
        error => Err(accessibility_error("AXUIElementCopyAttributeValue", error)),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn optional_ax_string(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<String>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    Ok(value
        .downcast::<CFString>()
        .ok()
        .map(|value| bounded_text(&value.to_string())))
}

#[cfg(target_os = "macos")]
fn element_fingerprint(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    inherited_protected: bool,
) -> Result<ElementFingerprint, String> {
    let role = optional_ax_string(deadline, element, "AXRole")?
        .ok_or_else(|| "accessibility_failed: AX element is missing a string role".to_string())?;
    let protected = inherited_protected
        || optional_ax_bool(deadline, element, "AXProtectedContent")?.unwrap_or(false);
    Ok(ElementFingerprint {
        role,
        subrole: optional_ax_string(deadline, element, "AXSubrole")?,
        identifier: optional_ax_string(deadline, element, "AXIdentifier")?,
        title: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXTitle")?
        },
        description: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXDescription")?
        },
        placeholder: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXPlaceholderValue")?
        },
        protected,
    })
}

#[cfg(target_os = "macos")]
pub(super) fn optional_ax_bool(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<bool>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    Ok(value
        .downcast::<CFBoolean>()
        .ok()
        .map(|value| value.value()))
}

#[cfg(target_os = "macos")]
fn optional_ax_point(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CGPoint>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    let Ok(value) = value.downcast::<AXValue>() else {
        return Ok(None);
    };
    if unsafe { value.r#type() } != AXValueType::CGPoint {
        return Ok(None);
    }
    let mut point = CGPoint::ZERO;
    let copied = unsafe { value.value(AXValueType::CGPoint, NonNull::from(&mut point).cast()) };
    Ok(copied.then_some(point))
}

#[cfg(target_os = "macos")]
fn optional_ax_size(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CGSize>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    let Ok(value) = value.downcast::<AXValue>() else {
        return Ok(None);
    };
    if unsafe { value.r#type() } != AXValueType::CGSize {
        return Ok(None);
    }
    let mut size = CGSize::ZERO;
    let copied = unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) };
    Ok(copied.then_some(size))
}

#[cfg(target_os = "macos")]
fn ax_window_geometry_matches(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<bool, String> {
    const TOLERANCE: f64 = 2.0;
    let Some(position) = optional_ax_point(deadline, element, "AXPosition")? else {
        return Ok(false);
    };
    let Some(size) = optional_ax_size(deadline, element, "AXSize")? else {
        return Ok(false);
    };
    Ok((position.x - f64::from(x)).abs() <= TOLERANCE
        && (position.y - f64::from(y)).abs() <= TOLERANCE
        && (size.width - f64::from(width)).abs() <= TOLERANCE
        && (size.height - f64::from(height)).abs() <= TOLERANCE)
}

#[cfg(target_os = "macos")]
fn ax_array_count(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<usize, String> {
    let attribute = CFString::from_static_str(attribute);
    let mut count: CFIndex = 0;
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.attribute_value_count(&attribute, NonNull::from(&mut count)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => {
            let count = usize::try_from(count).map_err(|_| {
                "accessibility_failed: AX array count is negative or too large".to_string()
            })?;
            if count > MAX_AX_CHILD_COUNT {
                return Err(
                    "accessibility_failed: AX child count exceeds bounded inspection limit"
                        .to_string(),
                );
            }
            Ok(count)
        }
        AXError::AttributeUnsupported | AXError::NoValue => Ok(0),
        error => Err(accessibility_error(
            "AXUIElementGetAttributeValueCount",
            error,
        )),
    }
}

#[cfg(target_os = "macos")]
fn ax_elements(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
    count: usize,
) -> Result<Vec<CFRetained<AXUIElement>>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let attribute = CFString::from_static_str(attribute_name);
    let max_values = CFIndex::try_from(count)
        .map_err(|_| "accessibility_failed: AX array request exceeds CFIndex".to_string())?;
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe {
        element.copy_attribute_values(&attribute, 0, max_values, NonNull::from(&mut raw))
    };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
    }
    let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
        "accessibility_failed: AX array copy succeeded with null value".to_string()
    })?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    let mut output = Vec::with_capacity(array.len());
    for value in array.iter() {
        let element = value.downcast::<AXUIElement>().map_err(|_| {
            "accessibility_failed: AX element array contained a non-element value".to_string()
        })?;
        output.push(element);
    }
    Ok(output)
}

#[cfg(target_os = "macos")]
fn ax_supports_action(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    expected_action: &'static str,
) -> Result<bool, String> {
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.copy_action_names(NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyActionNames", error));
    }
    let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
        "accessibility_failed: AX action-name copy succeeded with null value".to_string()
    })?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    if array.len() > MAX_AX_ACTION_NAMES {
        return Err(
            "accessibility_failed: AX action-name list exceeds bounded inspection limit"
                .to_string(),
        );
    }
    let expected_utf16_len = expected_action.encode_utf16().count();
    for value in array.iter() {
        let action = value.downcast::<CFString>().map_err(|_| {
            "accessibility_failed: AX action-name array contained a non-string value".to_string()
        })?;
        let action_len = usize::try_from(action.length()).map_err(|_| {
            "accessibility_failed: AX action name has invalid string length".to_string()
        })?;
        if action_len == expected_utf16_len && action.to_string() == expected_action {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
pub(super) fn ax_attribute_settable(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
) -> Result<bool, String> {
    let attribute = CFString::from_static_str(attribute_name);
    let mut settable = 0u8;
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.is_attribute_settable(&attribute, NonNull::from(&mut settable)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => Ok(settable != 0),
        AXError::AttributeUnsupported => Ok(false),
        error => Err(accessibility_error("AXUIElementIsAttributeSettable", error)),
    }
}

#[cfg(target_os = "macos")]
fn ax_element_at(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
    index: usize,
) -> Result<CFRetained<AXUIElement>, String> {
    let attribute = CFString::from_static_str(attribute_name);
    let index = CFIndex::try_from(index)
        .map_err(|_| "stale_element: AX child index exceeds CFIndex".to_string())?;
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error =
        unsafe { element.copy_attribute_values(&attribute, index, 1, NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
    }
    let raw = NonNull::new(raw.cast_mut())
        .ok_or_else(|| "stale_element: AX child lookup returned null".to_string())?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    if array.len() != 1 {
        return Err("stale_element: AX child path no longer resolves exactly".to_string());
    }
    array
        .iter()
        .next()
        .expect("single AX child")
        .downcast::<AXUIElement>()
        .map_err(|_| "stale_element: AX child path resolved to a non-element value".to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn exact_ax_window(
    surface: &SurfaceRecord,
    deadline: &AxObservationDeadline,
) -> Result<CFRetained<AXUIElement>, String> {
    let native_window = resolve_surface_window(surface)?;
    deadline.ensure_remaining()?;
    let x = native_window.x().map_err(map_error)?;
    let y = native_window.y().map_err(map_error)?;
    let width = native_window.width().map_err(map_error)?;
    let height = native_window.height().map_err(map_error)?;
    let pid = checked_surface_pid(surface)?;
    let application = unsafe { AXUIElement::new_application(pid) };
    let window_count = ax_array_count(deadline, &application, "AXWindows")?;
    if window_count == 0 || window_count > MAX_AX_WINDOWS {
        return Err(
            "accessibility_failed: exact AX window cannot be resolved within the bounded window set"
                .to_string(),
        );
    }
    let mut windows = ax_elements(deadline, &application, "AXWindows", window_count)?;
    let mut geometry_matches = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        if ax_window_geometry_matches(deadline, window, x, y, width, height)? {
            geometry_matches.push(index);
        }
    }
    if !geometry_matches.is_empty() {
        let index = select_exact_ax_window_index(&geometry_matches, &[], windows.len())?;
        return Ok(windows.swap_remove(index));
    }

    let mut title_matches = Vec::new();
    if !surface.title.is_empty() {
        for (index, window) in windows.iter().enumerate() {
            if optional_ax_string(deadline, window, "AXTitle")?
                .is_some_and(|title| bounded_text(&title) == surface.title)
            {
                title_matches.push(index);
            }
        }
    }
    let index = select_exact_ax_window_index(&geometry_matches, &title_matches, windows.len())?;
    Ok(windows.swap_remove(index))
}

#[cfg(target_os = "macos")]
pub(crate) fn accessibility_status() -> Result<Value, String> {
    Ok(json!({
        "platform": "macos",
        "trusted": unsafe { AXIsProcessTrusted() },
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn accessibility_tree(
    surface_id: &str,
    surface: &SurfaceRecord,
    max_depth: usize,
    max_nodes: usize,
) -> Result<AccessibilityTreeResult, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let deadline = AxObservationDeadline::new();
    let root = exact_ax_window(surface, &deadline)?;
    let mut queue = VecDeque::from([(
        root,
        None::<String>,
        0usize,
        Vec::<usize>::new(),
        Vec::<ElementFingerprint>::new(),
        false,
    )]);
    let mut nodes = Vec::with_capacity(max_nodes.min(64));
    let mut elements = Vec::with_capacity(max_nodes.min(64));
    let mut truncated = false;
    while let Some((element, parent_element_id, depth, path, mut lineage, inherited_protected)) =
        queue.pop_front()
    {
        deadline.ensure_remaining()?;
        if nodes.len() >= max_nodes {
            truncated = true;
            break;
        }
        let element_id = format!("element_{}", Uuid::new_v4().simple());
        let fingerprint = element_fingerprint(&deadline, &element, inherited_protected)?;
        let role = fingerprint.role.clone();
        let subrole = fingerprint.subrole.clone();
        let title = fingerprint.title.clone();
        let description = fingerprint.description.clone();
        let placeholder = fingerprint.placeholder.clone();
        let protected = fingerprint.protected;
        lineage.push(fingerprint);
        let sensitive = role == "AXSecureTextField"
            || subrole
                .as_deref()
                .is_some_and(|value| value.contains("Secure"));
        let value = if sensitive || protected {
            None
        } else {
            optional_ax_string(&deadline, &element, "AXValue")?
        };
        let enabled = optional_ax_bool(&deadline, &element, "AXEnabled")?;
        let focused = optional_ax_bool(&deadline, &element, "AXFocused")?;
        let child_count = ax_array_count(&deadline, &element, "AXChildren")?;
        if depth < max_depth && child_count > 0 {
            let reserved = nodes.len() + queue.len() + 1;
            let remaining = max_nodes.saturating_sub(reserved);
            let take = child_count.min(remaining);
            if take < child_count {
                truncated = true;
            }
            for (index, child) in ax_elements(&deadline, &element, "AXChildren", take)?
                .into_iter()
                .enumerate()
            {
                let mut child_path = path.clone();
                child_path.push(index);
                queue.push_back((
                    child,
                    Some(element_id.clone()),
                    depth + 1,
                    child_path,
                    lineage.clone(),
                    protected,
                ));
            }
        } else if child_count > 0 {
            truncated = true;
        }
        elements.push((
            element_id.clone(),
            ElementRecord {
                surface_id: surface_id.to_string(),
                path,
                lineage,
            },
        ));
        nodes.push(json!({
            "element_id": element_id,
            "parent_element_id": parent_element_id,
            "depth": depth,
            "role": role,
            "subrole": subrole,
            "title": title,
            "description": description,
            "value": value,
            "placeholder": placeholder,
            "enabled": enabled,
            "focused": focused,
            "child_count": child_count,
        }));
    }
    if !queue.is_empty() {
        truncated = true;
    }
    deadline.ensure_remaining()?;
    let node_count = nodes.len();
    Ok(AccessibilityTreeResult {
        output: json!({
            "platform": "macos",
            "surface_id": surface_id,
            "nodes": nodes,
            "node_count": node_count,
            "truncated": truncated,
            "max_depth": max_depth,
            "max_nodes": max_nodes,
        }),
        elements,
    })
}

#[cfg(target_os = "macos")]
pub(super) fn resolve_correlated_element(
    surface: &SurfaceRecord,
    element: &ElementRecord,
    deadline: &AxObservationDeadline,
) -> Result<CFRetained<AXUIElement>, String> {
    if element.lineage.len() != element.path.len() + 1 {
        return Err("stale_element: AX element correlation lineage is incomplete".to_string());
    }
    let mut current = exact_ax_window(surface, deadline)?;
    let current_root_fingerprint = element_fingerprint(deadline, &current, false)?;
    ensure_correlated_fingerprint(&element.lineage[0], &current_root_fingerprint, true)?;
    for (depth, &index) in element.path.iter().enumerate() {
        let child_count = ax_array_count(deadline, &current, "AXChildren")?;
        if index >= child_count {
            return Err("stale_element: AX child path no longer exists".to_string());
        }
        current = ax_element_at(deadline, &current, "AXChildren", index)?;
        let current_fingerprint =
            element_fingerprint(deadline, &current, element.lineage[depth].protected)?;
        ensure_correlated_fingerprint(&element.lineage[depth + 1], &current_fingerprint, false)?;
    }
    Ok(current)
}

#[cfg(target_os = "macos")]
pub(crate) fn element_state(
    surface_id: &str,
    element_id: &str,
    observation_generation: u32,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target = validate_element_state_target(element)?;
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
    let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
    let protected = element.contains_protected_content()
        || element.lineage.iter().any(is_secure_text_fingerprint);
    let enabled_for_effect = enabled != Some(false);
    let can_press =
        !protected && enabled_for_effect && ax_supports_action(&deadline, &current, "AXPress")?;
    let can_focus = !protected
        && enabled_for_effect
        && ax_attribute_settable(&deadline, &current, "AXFocused")?;

    let supported_text = !protected && is_supported_text_input_fingerprint(target);
    let (value_empty, can_input_text) = if supported_text {
        let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
        let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
        let value_empty = current_value.as_deref().map(str::is_empty);
        let can_input_text = enabled != Some(false)
            && focused == Some(true)
            && value_settable
            && value_empty == Some(true);
        (value_empty, can_input_text)
    } else {
        (None, false)
    };
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "observation_generation": observation_generation,
        "enabled": enabled,
        "focused": focused,
        "protected": protected,
        "value_empty": value_empty,
        "can_press": can_press,
        "can_focus": can_focus,
        "can_input_text": can_input_text,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn activate_window(surface_id: &str, surface: &SurfaceRecord) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }

    // Re-resolve the native surface and exact AX window immediately before
    // any effect so an opaque stale surface cannot drift to another window.
    let deadline = AxObservationDeadline::new();
    let window = exact_ax_window(surface, &deadline)?;
    let application = unsafe { AXUIElement::new_application(surface.pid as _) };
    let frontmost = optional_ax_bool(&deadline, &application, "AXFrontmost")?;
    if frontmost != Some(true) && !ax_attribute_settable(&deadline, &application, "AXFrontmost")? {
        return Err(
            "control_failed: AX application does not allow AXFrontmost to be set".to_string(),
        );
    }
    if !ax_supports_action(&deadline, &window, "AXRaise")? {
        return Err("control_failed: exact AX window does not support AXRaise".to_string());
    }

    // Prepare both native call sites before the first mutation. After the
    // application becomes frontmost, any later failure is a partial effect.
    prepare_ax_call(&deadline, &application)?;
    prepare_ax_call(&deadline, &window)?;
    let mut application_activated = false;
    if frontmost != Some(true) {
        let error = unsafe {
            application.set_attribute_value(
                &CFString::from_static_str("AXFrontmost"),
                CFBoolean::new(true),
            )
        };
        if error != AXError::Success {
            return Err(window_activation_attempt_error(
                "AXUIElementSetAttributeValue(AXFrontmost)",
                error,
                false,
            ));
        }
        application_activated = true;
    }

    let error = unsafe { window.perform_action(&CFString::from_static_str("AXRaise")) };
    if error != AXError::Success {
        return Err(window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            error,
            application_activated,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn control(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    action: ComputerAction,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target_fingerprint = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: macOS Accessibility protected content cannot be controlled"
                .to_string(),
        );
    }
    if !target_fingerprint.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for control".to_string(),
        );
    }
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;

    match action {
        ComputerAction::Press if !ax_supports_action(&deadline, &current, "AXPress")? => {
            return Err(
                "control_failed: AX element does not support the AXPress action".to_string(),
            );
        }
        ComputerAction::Focus if !ax_attribute_settable(&deadline, &current, "AXFocused")? => {
            return Err(
                "control_failed: AX element does not allow AXFocused to be set".to_string(),
            );
        }
        _ => {}
    }

    prepare_ax_call(&deadline, &current)?;
    let error = match action {
        ComputerAction::Press => unsafe {
            current.perform_action(&CFString::from_static_str("AXPress"))
        },
        ComputerAction::Focus => unsafe {
            current.set_attribute_value(
                &CFString::from_static_str("AXFocused"),
                CFBoolean::new(true),
            )
        },
    };
    if error != AXError::Success {
        return Err(control_attempt_error(
            match action {
                ComputerAction::Press => "AXUIElementPerformAction(AXPress)",
                ComputerAction::Focus => "AXUIElementSetAttributeValue(AXFocused)",
            },
            error,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "action": action.as_str(),
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn scroll_to_element(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target_fingerprint = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: macOS Accessibility protected content cannot be scrolled"
                .to_string(),
        );
    }
    if !target_fingerprint.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for scrolling"
                .to_string(),
        );
    }
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    if !ax_supports_action(&deadline, &current, "AXScrollToVisible")? {
        return Err(
            "scroll_failed: AX element does not support the AXScrollToVisible action".to_string(),
        );
    }
    prepare_ax_call(&deadline, &current)?;
    let error = unsafe { current.perform_action(&CFString::from_static_str("AXScrollToVisible")) };
    if error != AXError::Success {
        return Err(scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            error,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(super) fn validate_key_input_target(
    deadline: &AxObservationDeadline,
    application: &AXUIElement,
    exact_window: &CFRetained<AXUIElement>,
) -> Result<(), String> {
    if optional_ax_bool(deadline, application, "AXFrontmost")? != Some(true) {
        return Err(
            "key_input_failed: exact surface application must already be frontmost".to_string(),
        );
    }
    let focused_window = optional_ax_value(deadline, application, "AXFocusedWindow")?
        .ok_or_else(|| {
            "key_input_failed: exact surface application has no focused window".to_string()
        })?
        .downcast::<AXUIElement>()
        .map_err(|_| "accessibility_failed: AXFocusedWindow is not an AXUIElement".to_string())?;
    if &focused_window != exact_window {
        return Err(
            "key_input_failed: exact surface must already be the focused window".to_string(),
        );
    }

    if let Some(focused_value) = optional_ax_value(deadline, application, "AXFocusedUIElement")? {
        let focused_element = focused_value.downcast::<AXUIElement>().map_err(|_| {
            "accessibility_failed: AXFocusedUIElement is not an AXUIElement".to_string()
        })?;
        let fingerprint = element_fingerprint(deadline, &focused_element, false)?;
        if fingerprint.protected || is_secure_text_fingerprint(&fingerprint) {
            return Err(
                "permission_denied: protected or secure Accessibility content cannot receive key input"
                    .to_string(),
            );
        }
    }
    Ok(())
}
