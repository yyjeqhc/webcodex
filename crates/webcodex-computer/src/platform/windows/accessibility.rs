use super::*;

#[cfg(windows)]
const UIA_CONNECTION_TIMEOUT_MS: u32 = 2_000;
#[cfg(windows)]
const UIA_TRANSACTION_TIMEOUT_MS: u32 = 2_000;
#[cfg(windows)]
const UIA_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(windows)]
const UIA_OBSERVATION_TIMEOUT_ERROR: &str =
    "accessibility_failed: Windows UI Automation observation deadline exceeded";
#[cfg(windows)]
const MAX_UIA_RUNTIME_ID_ELEMENTS: usize = 64;
#[cfg(windows)]
const MAX_UIA_FOCUS_ANCESTORS: usize = 64;

#[cfg(windows)]
pub(super) struct UiaObservationDeadline {
    expires_at: Instant,
}

#[cfg(windows)]
impl UiaObservationDeadline {
    fn new() -> Self {
        Self {
            expires_at: Instant::now() + UIA_OBSERVATION_TIMEOUT,
        }
    }

    pub(super) fn ensure_remaining(&self) -> Result<(), String> {
        self.expires_at
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .map(|_| ())
            .ok_or_else(|| UIA_OBSERVATION_TIMEOUT_ERROR.to_string())
    }
}

#[cfg(windows)]
struct ComInitialization {
    uninitialize: bool,
}

#[cfg(windows)]
impl ComInitialization {
    fn new() -> Result<Self, String> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(Self { uninitialize: true })
        } else if result == RPC_E_CHANGED_MODE {
            // The Runner thread is already initialized in another apartment.
            // UI Automation supports either apartment; do not uninitialize an
            // apartment established by another subsystem.
            Ok(Self {
                uninitialize: false,
            })
        } else {
            Err(format!(
                "accessibility_failed: CoInitializeEx failed with HRESULT(0x{:08X})",
                result.0 as u32
            ))
        }
    }
}

#[cfg(windows)]
impl Drop for ComInitialization {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(windows)]
pub(super) struct UiaContext {
    // COM interfaces must be released before the matching CoUninitialize.
    // Rust drops struct fields in declaration order, so keep the guard last.
    automation: IUIAutomation2,
    walker: IUIAutomationTreeWalker,
    pub(super) deadline: UiaObservationDeadline,
    _com: ComInitialization,
}

#[cfg(windows)]
impl UiaContext {
    pub(super) fn new() -> Result<Self, String> {
        let com = ComInitialization::new()?;
        let automation: IUIAutomation2 =
            unsafe { CoCreateInstance(&CUIAutomation8, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
                .map_err(|error| uia_error("CoCreateInstance(CUIAutomation8)", &error))?;
        unsafe { automation.SetConnectionTimeout(UIA_CONNECTION_TIMEOUT_MS) }
            .map_err(|error| uia_error("IUIAutomation2::SetConnectionTimeout", &error))?;
        unsafe { automation.SetTransactionTimeout(UIA_TRANSACTION_TIMEOUT_MS) }
            .map_err(|error| uia_error("IUIAutomation2::SetTransactionTimeout", &error))?;
        let walker = unsafe { automation.ControlViewWalker() }
            .map_err(|error| uia_error("IUIAutomation::ControlViewWalker", &error))?;
        Ok(Self {
            automation,
            walker,
            deadline: UiaObservationDeadline::new(),
            _com: com,
        })
    }
}

#[cfg(windows)]
struct OwnedSafeArray(NonNull<SAFEARRAY>);

#[cfg(windows)]
impl OwnedSafeArray {
    fn new(array: *mut SAFEARRAY) -> Result<Self, String> {
        NonNull::new(array)
            .map(Self)
            .ok_or_else(|| "stale_element: UI Automation runtime id is missing".to_string())
    }

    fn as_ptr(&self) -> *mut SAFEARRAY {
        self.0.as_ptr()
    }
}

#[cfg(windows)]
impl Drop for OwnedSafeArray {
    fn drop(&mut self) {
        unsafe {
            let _ = SafeArrayDestroy(self.as_ptr());
        }
    }
}

#[cfg(windows)]
pub(super) fn uia_error(operation: &str, error: &windows::core::Error) -> String {
    format!(
        "accessibility_failed: {operation} failed with HRESULT(0x{:08X})",
        error.code().0 as u32
    )
}

#[cfg(windows)]
fn uia_error_code(error: &windows::core::Error) -> u32 {
    error.code().0 as u32
}

#[cfg(windows)]
fn optional_uia_element(
    result: windows::core::Result<IUIAutomationElement>,
    operation: &str,
) -> Result<Option<IUIAutomationElement>, String> {
    match result {
        Ok(element) => Ok(Some(element)),
        // UIA tree-walker APIs use S_OK + a null interface for end-of-list.
        // windows-rs represents that nullable success as Error::empty() (S_OK).
        Err(error) if error.code().is_ok() || error.code() == E_POINTER => Ok(None),
        Err(error) => Err(uia_error(operation, &error)),
    }
}

#[cfg(windows)]
fn optional_uia_pattern<T: Interface>(
    context: &UiaContext,
    element: &IUIAutomationElement,
    pattern: UIA_PATTERN_ID,
) -> Result<Option<T>, String> {
    context.deadline.ensure_remaining()?;
    match unsafe { element.GetCurrentPatternAs::<T>(pattern) } {
        Ok(pattern) => Ok(Some(pattern)),
        Err(error)
            if error.code().is_ok()
                || error.code() == E_NOINTERFACE
                || error.code() == E_POINTER
                || uia_error_code(&error) == UIA_E_NOTSUPPORTED =>
        {
            Ok(None)
        }
        Err(error) if uia_error_code(&error) == UIA_E_ELEMENTNOTAVAILABLE => {
            Err("stale_element: UI Automation element is no longer available".to_string())
        }
        Err(error) => Err(uia_error(
            "IUIAutomationElement::GetCurrentPatternAs",
            &error,
        )),
    }
}

#[cfg(windows)]
pub(super) fn uia_element_has_exact_focus(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<bool, String> {
    context.deadline.ensure_remaining()?;
    let Some(focused) = optional_uia_element(
        unsafe { context.automation.GetFocusedElement() },
        "IUIAutomation::GetFocusedElement",
    )?
    else {
        return Ok(false);
    };
    context.deadline.ensure_remaining()?;
    unsafe { context.automation.CompareElements(element, &focused) }
        .map(|same| same.as_bool())
        .map_err(|error| uia_error("IUIAutomation::CompareElements", &error))
}

#[cfg(windows)]
fn uia_string(
    result: windows::core::Result<windows::core::BSTR>,
    operation: &str,
) -> Result<Option<String>, String> {
    let value = result.map_err(|error| uia_error(operation, &error))?;
    let value = bounded_text(&value.to_string());
    Ok((!value.is_empty()).then_some(value))
}

#[cfg(windows)]
pub(crate) fn uia_control_role(control_type: UIA_CONTROLTYPE_ID) -> String {
    let role = if control_type == UIA_WindowControlTypeId {
        Some("AXWindow")
    } else if control_type == UIA_ButtonControlTypeId {
        Some("AXButton")
    } else if control_type == UIA_EditControlTypeId {
        Some("AXTextField")
    } else if control_type == UIA_DocumentControlTypeId {
        Some("AXTextArea")
    } else if control_type == UIA_HyperlinkControlTypeId {
        Some("AXLink")
    } else if control_type == UIA_CheckBoxControlTypeId {
        Some("AXCheckBox")
    } else if control_type == UIA_RadioButtonControlTypeId {
        Some("AXRadioButton")
    } else if control_type == UIA_ComboBoxControlTypeId {
        Some("AXComboBox")
    } else if control_type == UIA_ListControlTypeId {
        Some("AXList")
    } else if control_type == UIA_ListItemControlTypeId
        || control_type == UIA_TreeItemControlTypeId
        || control_type == UIA_DataItemControlTypeId
    {
        Some("AXRow")
    } else if control_type == UIA_MenuControlTypeId {
        Some("AXMenu")
    } else if control_type == UIA_MenuItemControlTypeId {
        Some("AXMenuItem")
    } else if control_type == UIA_TreeControlTypeId {
        Some("AXOutline")
    } else if control_type == UIA_TabControlTypeId {
        Some("AXTabGroup")
    } else if control_type == UIA_TabItemControlTypeId {
        Some("AXRadioButton")
    } else if control_type == UIA_TextControlTypeId {
        Some("AXStaticText")
    } else if control_type == UIA_TableControlTypeId || control_type == UIA_DataGridControlTypeId {
        Some("AXTable")
    } else if control_type == UIA_ToolBarControlTypeId {
        Some("AXToolbar")
    } else if control_type == UIA_ScrollBarControlTypeId {
        Some("AXScrollBar")
    } else if control_type == UIA_SliderControlTypeId {
        Some("AXSlider")
    } else if control_type == UIA_SpinnerControlTypeId {
        Some("AXIncrementor")
    } else if control_type == UIA_ProgressBarControlTypeId {
        Some("AXProgressIndicator")
    } else if control_type == UIA_HeaderItemControlTypeId {
        Some("AXColumn")
    } else if control_type == UIA_PaneControlTypeId
        || control_type == UIA_GroupControlTypeId
        || control_type == UIA_CustomControlTypeId
        || control_type == UIA_HeaderControlTypeId
        || control_type == UIA_StatusBarControlTypeId
        || control_type == UIA_SeparatorControlTypeId
        || control_type == UIA_ToolTipControlTypeId
    {
        Some("AXGroup")
    } else {
        None
    };
    role.map(str::to_string)
        .unwrap_or_else(|| format!("UIAControlType({})", control_type.0))
}

#[cfg(windows)]
pub(crate) fn uia_semantic_focus_role(role: &str) -> bool {
    role == "AXTextField"
}

#[cfg(windows)]
pub(crate) fn uia_semantic_text_input_role(role: &str) -> bool {
    role == "AXTextField"
}

#[cfg(windows)]
fn uia_runtime_id(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<Vec<i32>, String> {
    context.deadline.ensure_remaining()?;
    let array = unsafe { element.GetRuntimeId() }.map_err(|error| {
        if uia_error_code(&error) == UIA_E_ELEMENTNOTAVAILABLE {
            "stale_element: UI Automation element is no longer available".to_string()
        } else {
            uia_error("IUIAutomationElement::GetRuntimeId", &error)
        }
    })?;
    let array = OwnedSafeArray::new(array)?;
    if unsafe { SafeArrayGetDim(array.as_ptr()) } != 1
        || unsafe { SafeArrayGetElemsize(array.as_ptr()) } != std::mem::size_of::<i32>() as u32
    {
        return Err(
            "stale_element: UI Automation runtime id has an invalid SAFEARRAY shape".to_string(),
        );
    }
    let lower = unsafe { SafeArrayGetLBound(array.as_ptr(), 1) }
        .map_err(|error| uia_error("SafeArrayGetLBound(runtime id)", &error))?;
    let upper = unsafe { SafeArrayGetUBound(array.as_ptr(), 1) }
        .map_err(|error| uia_error("SafeArrayGetUBound(runtime id)", &error))?;
    let length = upper
        .checked_sub(lower)
        .and_then(|span| span.checked_add(1))
        .and_then(|length| usize::try_from(length).ok())
        .filter(|length| (1..=MAX_UIA_RUNTIME_ID_ELEMENTS).contains(length))
        .ok_or_else(|| {
            "stale_element: UI Automation runtime id length is invalid or exceeds the bound"
                .to_string()
        })?;
    let mut runtime_id = Vec::with_capacity(length);
    for offset in 0..length {
        context.deadline.ensure_remaining()?;
        let index = lower
            .checked_add(i32::try_from(offset).map_err(|_| {
                "stale_element: UI Automation runtime id index exceeds the bound".to_string()
            })?)
            .ok_or_else(|| "stale_element: UI Automation runtime id index overflow".to_string())?;
        let mut value = 0i32;
        unsafe {
            SafeArrayGetElement(
                array.as_ptr(),
                &index,
                (&mut value as *mut i32).cast::<std::ffi::c_void>(),
            )
        }
        .map_err(|error| uia_error("SafeArrayGetElement(runtime id)", &error))?;
        runtime_id.push(value);
    }
    Ok(runtime_id)
}

#[cfg(windows)]
fn uia_fingerprint(
    context: &UiaContext,
    element: &IUIAutomationElement,
    inherited_protected: bool,
) -> Result<ElementFingerprint, String> {
    context.deadline.ensure_remaining()?;
    let control_type = unsafe { element.CurrentControlType() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentControlType", &error))?;
    context.deadline.ensure_remaining()?;
    let is_password = unsafe { element.CurrentIsPassword() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsPassword", &error))?
        .as_bool();
    let protected = inherited_protected || is_password;
    let native_runtime_id = uia_runtime_id(context, element)?;
    context.deadline.ensure_remaining()?;
    let identifier = uia_string(
        unsafe { element.CurrentAutomationId() },
        "IUIAutomationElement::CurrentAutomationId",
    )?;
    let title = if protected {
        None
    } else {
        context.deadline.ensure_remaining()?;
        uia_string(
            unsafe { element.CurrentName() },
            "IUIAutomationElement::CurrentName",
        )?
    };
    let description = if protected {
        None
    } else {
        context.deadline.ensure_remaining()?;
        uia_string(
            unsafe { element.CurrentHelpText() },
            "IUIAutomationElement::CurrentHelpText",
        )?
    };
    Ok(ElementFingerprint {
        role: uia_control_role(control_type),
        native_runtime_id,
        subrole: None,
        identifier,
        title,
        description,
        placeholder: None,
        protected,
    })
}

#[cfg(windows)]
pub(super) fn uia_text_pattern(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<Option<IUIAutomationValuePattern>, String> {
    optional_uia_pattern::<IUIAutomationValuePattern>(context, element, UIA_ValuePatternId)
}

#[cfg(windows)]
pub(super) fn uia_value_pattern_current_value(
    context: &UiaContext,
    pattern: &IUIAutomationValuePattern,
) -> Result<String, String> {
    context.deadline.ensure_remaining()?;
    uia_string(
        unsafe { pattern.CurrentValue() },
        "IUIAutomationValuePattern::CurrentValue",
    )
    .map(|value| value.unwrap_or_default())
}

#[cfg(windows)]
pub(super) fn uia_value_pattern_writable(
    context: &UiaContext,
    pattern: &IUIAutomationValuePattern,
) -> Result<bool, String> {
    context.deadline.ensure_remaining()?;
    unsafe { pattern.CurrentIsReadOnly() }
        .map(|read_only| !read_only.as_bool())
        .map_err(|error| uia_error("IUIAutomationValuePattern::CurrentIsReadOnly", &error))
}

#[cfg(windows)]
fn uia_text_value(
    context: &UiaContext,
    element: &IUIAutomationElement,
) -> Result<Option<String>, String> {
    let Some(pattern) = uia_text_pattern(context, element)? else {
        return Ok(None);
    };
    uia_value_pattern_current_value(context, &pattern).map(Some)
}

#[cfg(windows)]
fn uia_children(
    context: &UiaContext,
    element: &IUIAutomationElement,
    limit: usize,
) -> Result<(Vec<IUIAutomationElement>, bool), String> {
    let mut output = Vec::with_capacity(limit.min(32));
    context.deadline.ensure_remaining()?;
    let mut current = optional_uia_element(
        unsafe { context.walker.GetFirstChildElement(element) },
        "IUIAutomationTreeWalker::GetFirstChildElement",
    )?;
    while let Some(element) = current {
        if output.len() >= limit {
            return Ok((output, true));
        }
        context.deadline.ensure_remaining()?;
        current = optional_uia_element(
            unsafe { context.walker.GetNextSiblingElement(&element) },
            "IUIAutomationTreeWalker::GetNextSiblingElement",
        )?;
        output.push(element);
    }
    Ok((output, false))
}

#[cfg(windows)]
pub(crate) fn win_hwnd(native_id: u32) -> Result<WinHwnd, String> {
    let hwnd = WinHwnd(native_id as i32 as isize as *mut std::ffi::c_void);
    if hwnd.0.is_null() {
        Err("stale_surface: window handle is invalid".to_string())
    } else {
        Ok(hwnd)
    }
}

#[cfg(windows)]
pub(super) fn exact_uia_window(
    context: &UiaContext,
    surface: &SurfaceRecord,
) -> Result<IUIAutomationElement, String> {
    let _window = resolve_surface_window(surface)?;
    let hwnd = win_hwnd(surface.native_id)?;
    context.deadline.ensure_remaining()?;
    let root = unsafe { context.automation.ElementFromHandle(hwnd) }
        .map_err(|error| uia_error("IUIAutomation::ElementFromHandle", &error))?;
    context.deadline.ensure_remaining()?;
    let current_hwnd = unsafe { root.CurrentNativeWindowHandle() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentNativeWindowHandle", &error))?;
    context.deadline.ensure_remaining()?;
    let current_pid = unsafe { root.CurrentProcessId() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentProcessId", &error))?;
    let current_pid = u32::try_from(current_pid)
        .map_err(|_| "stale_surface: UI Automation process id is invalid".to_string())?;
    if current_hwnd != hwnd || current_pid != surface.pid {
        return Err(
            "stale_surface: UI Automation root no longer matches the observed HWND/PID".to_string(),
        );
    }
    Ok(root)
}

#[cfg(windows)]
fn validate_uia_focused_element_root(
    context: &UiaContext,
    root: &IUIAutomationElement,
) -> Result<(), String> {
    context.deadline.ensure_remaining()?;
    let focused = optional_uia_element(
        unsafe { context.automation.GetFocusedElement() },
        "IUIAutomation::GetFocusedElement",
    )?
    .ok_or_else(|| "key_input_failed: Windows UI Automation has no focused element".to_string())?;
    let mut current = focused;

    for depth in 0..=MAX_UIA_FOCUS_ANCESTORS {
        context.deadline.ensure_remaining()?;
        let password = unsafe { current.CurrentIsPassword() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsPassword", &error))?
            .as_bool();
        if password {
            return Err(
                "permission_denied: protected or password UI Automation content cannot receive key input"
                    .to_string(),
            );
        }

        context.deadline.ensure_remaining()?;
        let same_root = unsafe { context.automation.CompareElements(root, &current) }
            .map_err(|error| uia_error("IUIAutomation::CompareElements", &error))?
            .as_bool();
        if same_root {
            return Ok(());
        }
        if depth == MAX_UIA_FOCUS_ANCESTORS {
            break;
        }

        context.deadline.ensure_remaining()?;
        current = optional_uia_element(
            unsafe { context.walker.GetParentElement(&current) },
            "IUIAutomationTreeWalker::GetParentElement",
        )?
        .ok_or_else(|| {
            "key_input_failed: focused UI Automation element is outside the exact window root"
                .to_string()
        })?;
    }

    Err(
        "key_input_failed: focused UI Automation ancestry exceeds the bounded exact-window check"
            .to_string(),
    )
}

#[cfg(windows)]
pub(super) fn validate_windows_key_input_target(
    context: &UiaContext,
    surface: &SurfaceRecord,
    root: &IUIAutomationElement,
) -> Result<(), String> {
    let hwnd = win_hwnd(surface.native_id)?;
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(
            "key_input_failed: exact Windows surface must already be the foreground window"
                .to_string(),
        );
    }
    validate_uia_focused_element_root(context, root)?;
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(
            "key_input_failed: exact Windows surface lost foreground during UI Automation focus preflight"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn resolve_uia_element(
    context: &UiaContext,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<IUIAutomationElement, String> {
    if element.lineage.len() != element.path.len() + 1 {
        return Err("stale_element: UIA element correlation lineage is incomplete".to_string());
    }
    let mut current = exact_uia_window(context, surface)?;
    let current_root = uia_fingerprint(context, &current, false)?;
    if current_root != element.lineage[0] {
        return Err("stale_element: UIA root identity changed since observation".to_string());
    }
    for (depth, &index) in element.path.iter().enumerate() {
        let (children, has_more) = uia_children(context, &current, index + 1)?;
        if children.len() <= index {
            return Err("stale_element: UIA child path no longer exists".to_string());
        }
        if has_more && index >= crate::MAX_ACCESSIBILITY_NODES {
            return Err("stale_element: UIA child path exceeds bounded correlation".to_string());
        }
        current = children[index].clone();
        let current_fingerprint =
            uia_fingerprint(context, &current, element.lineage[depth].protected)?;
        if current_fingerprint != element.lineage[depth + 1] {
            return Err("stale_element: UIA element lineage changed since observation".to_string());
        }
    }
    Ok(current)
}

#[cfg(windows)]
pub(crate) fn accessibility_status() -> Result<Value, String> {
    let _context = UiaContext::new()?;
    Ok(json!({"platform": "windows", "trusted": true}))
}

#[cfg(windows)]
pub(crate) fn accessibility_tree(
    surface_id: &str,
    surface: &SurfaceRecord,
    max_depth: usize,
    max_nodes: usize,
) -> Result<AccessibilityTreeResult, String> {
    let context = UiaContext::new()?;
    let root = exact_uia_window(&context, surface)?;
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

    while let Some((current, parent_element_id, depth, path, mut lineage, inherited_protected)) =
        queue.pop_front()
    {
        if nodes.len() >= max_nodes {
            truncated = true;
            break;
        }

        let element_id = format!("element_{}", Uuid::new_v4().simple());
        let fingerprint = uia_fingerprint(&context, &current, inherited_protected)?;
        let role = fingerprint.role.clone();
        let subrole = fingerprint.subrole.clone();
        let title = fingerprint.title.clone();
        let description = fingerprint.description.clone();
        let placeholder = fingerprint.placeholder.clone();
        let protected = fingerprint.protected;
        let value = if protected || !is_supported_text_input_fingerprint(&fingerprint) {
            None
        } else {
            uia_text_value(&context, &current)?
        };
        context.deadline.ensure_remaining()?;
        let enabled = unsafe { current.CurrentIsEnabled() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
            .as_bool();
        context.deadline.ensure_remaining()?;
        let focused = unsafe { current.CurrentHasKeyboardFocus() }
            .map_err(|error| uia_error("IUIAutomationElement::CurrentHasKeyboardFocus", &error))?
            .as_bool();
        lineage.push(fingerprint);

        let reserved = nodes.len() + queue.len() + 1;
        let remaining = max_nodes.saturating_sub(reserved);
        let inspect_limit = if depth < max_depth {
            remaining.saturating_add(1).max(1)
        } else {
            1
        };
        let (children, has_more_children) = uia_children(&context, &current, inspect_limit)?;
        let child_count = children.len() + usize::from(has_more_children);

        if depth < max_depth {
            if children.len() > remaining || has_more_children {
                truncated = true;
            }
            for (index, child) in children.into_iter().take(remaining).enumerate() {
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
    let node_count = nodes.len();
    Ok(AccessibilityTreeResult {
        output: json!({
            "platform": "windows",
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

#[cfg(windows)]
pub(crate) fn element_state(
    surface_id: &str,
    element_id: &str,
    observation_generation: u32,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for state".to_string(),
        );
    }
    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    let enabled = unsafe { current.CurrentIsEnabled() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsEnabled", &error))?
        .as_bool();
    let focused = uia_element_has_exact_focus(&context, &current)?;
    let protected = element.contains_protected_content();
    let (value_empty, value_writable) = if !protected && is_supported_text_input_fingerprint(target)
    {
        match uia_text_pattern(&context, &current)? {
            Some(pattern) => {
                let value = uia_value_pattern_current_value(&context, &pattern)?;
                let writable = uia_value_pattern_writable(&context, &pattern)?;
                (Some(value.is_empty()), writable)
            }
            None => (None, false),
        }
    } else {
        (None, false)
    };
    let hwnd = win_hwnd(surface.native_id)?;
    let surface_foreground = unsafe { GetForegroundWindow() == hwnd };
    let can_press = if protected || !enabled {
        false
    } else {
        optional_uia_pattern::<IUIAutomationInvokePattern>(&context, &current, UIA_InvokePatternId)?
            .is_some()
    };
    let can_focus =
        if protected || !enabled || !surface_foreground || !uia_semantic_focus_role(&target.role) {
            false
        } else {
            context.deadline.ensure_remaining()?;
            unsafe { current.CurrentIsKeyboardFocusable() }
                .map_err(|error| {
                    uia_error("IUIAutomationElement::CurrentIsKeyboardFocusable", &error)
                })?
                .as_bool()
        };
    let can_input_text = !protected
        && enabled
        && surface_foreground
        && focused
        && uia_semantic_text_input_role(&target.role)
        && value_writable
        && value_empty == Some(true);

    Ok(json!({
        "platform": "windows",
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

#[cfg(windows)]
pub(crate) fn windows_window_activation_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} did not establish the exact foreground-window postcondition after the native activation attempt"
    )
}

#[cfg(windows)]
pub(crate) fn windows_control_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} returned after the exact Windows UI Automation control effect was attempted"
    )
}

#[cfg(windows)]
pub(crate) fn windows_scroll_attempt_error(operation: &str) -> String {
    format!(
        "outcome_unknown: {operation} returned after the exact Windows UI Automation scroll effect was attempted"
    )
}

#[cfg(windows)]
pub(crate) fn activate_window(surface_id: &str, surface: &SurfaceRecord) -> Result<Value, String> {
    // Resolve the exact xcap identity immediately before the first native
    // effect. This never falls back to an application name, PID, or title.
    let _window = resolve_surface_window(surface)?;
    let hwnd = win_hwnd(surface.native_id)?;
    let already_foreground = unsafe { GetForegroundWindow() == hwnd };
    let minimized = unsafe { IsIconic(hwnd).as_bool() };
    if already_foreground && !minimized {
        return Ok(json!({
            "platform": "windows",
            "surface_id": surface_id,
            "success": true,
        }));
    }

    // Obtain the exact UIA root before the first effect. This revalidates
    // the same HWND/PID lineage used by read-only Windows observation.
    let context = UiaContext::new()?;
    let root = exact_uia_window(&context, surface)?;
    context.deadline.ensure_remaining()?;
    let control_type = unsafe { root.CurrentControlType() }
        .map_err(|error| uia_error("IUIAutomationElement::CurrentControlType", &error))?;
    if control_type != UIA_WindowControlTypeId {
        return Err(
            "control_failed: exact Windows UIA root is not an activatable Window control"
                .to_string(),
        );
    }
    let mut prior_effect = false;
    if minimized {
        // Restoring a foreign UI thread must not synchronously wait on a
        // stalled target. Queue the exact restore request asynchronously,
        // then observe only the local window-state predicate for a bounded
        // interval before proceeding.
        let restore_expires_at = Instant::now() + Duration::from_secs(2);
        let _ = unsafe { ShowWindowAsync(hwnd, SW_RESTORE) };
        prior_effect = true;
        while unsafe { IsIconic(hwnd).as_bool() } {
            if let Err(error) = context.deadline.ensure_remaining() {
                return Err(windows_window_activation_attempt_error(&error));
            }
            if Instant::now() >= restore_expires_at {
                return Err(windows_window_activation_attempt_error(
                    "ShowWindowAsync(SW_RESTORE) timeout",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // A background Runner is normally denied by SetForegroundWindow.
    // UI Automation's exact SetFocus is the native automation primitive;
    // once attempted, any error or mismatched postcondition is uncertain.
    if let Err(error) = context.deadline.ensure_remaining() {
        if prior_effect {
            return Err(windows_window_activation_attempt_error(&error));
        }
        return Err(error);
    }
    if let Err(error) = unsafe { root.SetFocus() } {
        return Err(windows_window_activation_attempt_error(&format!(
            "IUIAutomationElement::SetFocus HRESULT(0x{:08X})",
            error.code().0 as u32
        )));
    }
    if unsafe { GetForegroundWindow() != hwnd } {
        return Err(windows_window_activation_attempt_error(
            "IUIAutomationElement::SetFocus postcondition",
        ));
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "success": true,
    }))
}

#[cfg(windows)]
pub(crate) fn control(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    action: ComputerAction,
) -> Result<Value, String> {
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: Windows UI Automation protected content cannot be controlled"
                .to_string(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for control"
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
        return Err("control_failed: UI Automation element is disabled".to_string());
    }

    match action {
        ComputerAction::Press => {
            let pattern = optional_uia_pattern::<IUIAutomationInvokePattern>(
                &context,
                &current,
                UIA_InvokePatternId,
            )?
            .ok_or_else(|| {
                "control_failed: UI Automation element does not support InvokePattern".to_string()
            })?;
            context.deadline.ensure_remaining()?;
            if let Err(error) = unsafe { pattern.Invoke() } {
                return Err(windows_control_attempt_error(&format!(
                    "IUIAutomationInvokePattern::Invoke HRESULT(0x{:08X})",
                    error.code().0 as u32
                )));
            }
        }
        ComputerAction::Focus => {
            if !uia_semantic_focus_role(&target.role) {
                return Err(
                    "control_failed: UI Automation element role is outside the bounded semantic focus set"
                        .to_string(),
                );
            }
            let hwnd = win_hwnd(surface.native_id)?;
            if unsafe { GetForegroundWindow() != hwnd } {
                return Err(
                    "control_failed: exact Windows surface must already be foreground before element focus"
                        .to_string(),
                );
            }
            context.deadline.ensure_remaining()?;
            let focusable = unsafe { current.CurrentIsKeyboardFocusable() }
                .map_err(|error| {
                    uia_error("IUIAutomationElement::CurrentIsKeyboardFocusable", &error)
                })?
                .as_bool();
            if !focusable {
                return Err(
                    "control_failed: UI Automation element is not keyboard-focusable".to_string(),
                );
            }
            let already_focused = uia_element_has_exact_focus(&context, &current)?;
            if !already_focused {
                context.deadline.ensure_remaining()?;
                if let Err(error) = unsafe { current.SetFocus() } {
                    return Err(windows_control_attempt_error(&format!(
                        "IUIAutomationElement::SetFocus HRESULT(0x{:08X})",
                        error.code().0 as u32
                    )));
                }
                let focus_expires_at = Instant::now() + Duration::from_secs(1);
                loop {
                    if let Err(error) = context.deadline.ensure_remaining() {
                        return Err(windows_control_attempt_error(&error));
                    }
                    let focused = match uia_element_has_exact_focus(&context, &current) {
                        Ok(focused) => focused,
                        Err(error) => return Err(windows_control_attempt_error(&error)),
                    };
                    if focused {
                        break;
                    }
                    if Instant::now() >= focus_expires_at {
                        return Err(windows_control_attempt_error(
                            "IUIAutomationElement::SetFocus postcondition timeout",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "element_id": element_id,
        "action": action.as_str(),
        "success": true,
    }))
}

#[cfg(windows)]
pub(crate) fn scroll_to_element(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: Windows UI Automation protected content cannot be scrolled"
                .to_string(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for scrolling"
                .to_string(),
        );
    }

    // Revalidate the exact xcap surface, HWND/PID UIA root, and complete
    // RuntimeId-bearing root -> ancestor -> target lineage before the effect.
    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    let pattern = optional_uia_pattern::<IUIAutomationScrollItemPattern>(
        &context,
        &current,
        UIA_ScrollItemPatternId,
    )?
    .ok_or_else(|| {
        "scroll_failed: UI Automation element does not support ScrollItemPattern".to_string()
    })?;
    context.deadline.ensure_remaining()?;
    if let Err(error) = unsafe { pattern.ScrollIntoView() } {
        return Err(windows_scroll_attempt_error(&format!(
            "IUIAutomationScrollItemPattern::ScrollIntoView HRESULT(0x{:08X})",
            error.code().0 as u32
        )));
    }
    if let Err(error) = context.deadline.ensure_remaining() {
        return Err(windows_scroll_attempt_error(&error));
    }

    Ok(json!({
        "platform": "windows",
        "surface_id": surface_id,
        "element_id": element_id,
        "success": true,
    }))
}
#[cfg(all(test, windows))]
pub(crate) fn test_uia_is_offscreen(
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<bool, String> {
    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    unsafe { current.CurrentIsOffscreen() }
        .map(|value| value.as_bool())
        .map_err(|error| uia_error("IUIAutomationElement::CurrentIsOffscreen", &error))
}

#[cfg(all(test, windows))]
pub(crate) fn test_windows_focused_element_belongs_to_surface(
    surface: &SurfaceRecord,
) -> Result<(), String> {
    let context = UiaContext::new()?;
    let root = exact_uia_window(&context, surface)?;
    validate_uia_focused_element_root(&context, &root)
}
