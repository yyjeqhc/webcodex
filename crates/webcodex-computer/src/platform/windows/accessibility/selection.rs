//! SelectionItem effects stay separate from Invoke and never replay after an attempt.
use super::*;

pub(super) struct SelectionTarget {
    pattern: IUIAutomationSelectionItemPattern,
    container: IUIAutomationElement,
}

pub(super) fn target(
    context: &UiaContext,
    current: &IUIAutomationElement,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Option<SelectionTarget>, String> {
    context.deadline.ensure_remaining()?;
    let role = unsafe { current.CurrentControlType() }
        .map_err(|e| uia_error("IUIAutomationElement::CurrentControlType", &e))?;
    if role != UIA_ListItemControlTypeId && role != UIA_TabItemControlTypeId {
        return Ok(None);
    }
    let Some(pattern) = optional_uia_pattern::<IUIAutomationSelectionItemPattern>(
        context,
        current,
        UIA_SelectionItemPatternId,
    )?
    else {
        return Ok(None);
    };
    context.deadline.ensure_remaining()?;
    let container = unsafe { pattern.CurrentSelectionContainer() }.map_err(|e| {
        uia_error(
            "IUIAutomationSelectionItemPattern::CurrentSelectionContainer",
            &e,
        )
    })?;
    // Require the container to be an observed ancestor, not merely in the same process.
    // Resolve its complete recorded lineage again, including inherited protection.
    let mut ancestor = current.clone();
    for depth in (0..element.path.len()).rev() {
        context.deadline.ensure_remaining()?;
        ancestor = optional_uia_element(
            unsafe { context.walker.GetParentElement(&ancestor) },
            "IUIAutomationTreeWalker::GetParentElement",
        )?
        .ok_or_else(|| "stale_element: selection container ancestry is unavailable".to_string())?;
        context.deadline.ensure_remaining()?;
        if unsafe { context.automation.CompareElements(&container, &ancestor) }
            .map_err(|e| uia_error("IUIAutomation::CompareElements", &e))?
            .as_bool()
        {
            let record = ElementRecord {
                surface_id: element.surface_id.clone(),
                path: element.path[..depth].to_vec(),
                lineage: element.lineage[..=depth].to_vec(),
            };
            if record.contains_protected_content() {
                return Err("permission_denied: protected selection container".into());
            }
            let resolved = resolve_uia_element(context, surface, &record)?;
            context.deadline.ensure_remaining()?;
            if unsafe { context.automation.CompareElements(&container, &resolved) }
                .map_err(|e| uia_error("IUIAutomation::CompareElements", &e))?
                .as_bool()
            {
                return Ok(Some(SelectionTarget { pattern, container }));
            }
            break;
        }
    }
    Err("stale_element: selection container is outside the exact observed ancestry".into())
}

impl SelectionTarget {
    pub(super) fn selected(&self, context: &UiaContext) -> Result<bool, String> {
        context.deadline.ensure_remaining()?;
        unsafe { self.pattern.CurrentIsSelected() }
            .map(|v| v.as_bool())
            .map_err(|e| uia_error("IUIAutomationSelectionItemPattern::CurrentIsSelected", &e))
    }

    fn already_single_selection(&self, context: &UiaContext) -> Result<bool, String> {
        if !self.selected(context)? {
            return Ok(false);
        }
        let Some(container_pattern) = optional_uia_pattern::<IUIAutomationSelectionPattern>(
            context,
            &self.container,
            UIA_SelectionPatternId,
        )?
        else {
            return Ok(false);
        };
        context.deadline.ensure_remaining()?;
        // IsSelected alone does not prove a multi-selection can be left unchanged.
        unsafe { container_pattern.CurrentCanSelectMultiple() }
            .map(|v| !v.as_bool())
            .map_err(|e| {
                uia_error(
                    "IUIAutomationSelectionPattern::CurrentCanSelectMultiple",
                    &e,
                )
            })
    }
}

pub(super) fn select(
    context: &UiaContext,
    current: &IUIAutomationElement,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    control: &local_control::ActivityGuard,
) -> Result<(), String> {
    let selection = target(context, current, surface, element)?.ok_or_else(|| {
        "control_failed: select requires a ListItem or TabItem with SelectionItemPattern"
            .to_string()
    })?;
    if selection.already_single_selection(context)? {
        return Ok(());
    }
    // Repeat identity/ancestry after provider preflight, immediately before the effect.
    let current = resolve_uia_element(context, surface, element)?;
    let selection = target(context, &current, surface, element)?
        .ok_or_else(|| "stale_element: selection pattern changed before execution".to_string())?;
    context.deadline.ensure_remaining()?;
    if !unsafe { current.CurrentIsEnabled() }
        .map_err(|e| uia_error("IUIAutomationElement::CurrentIsEnabled", &e))?
        .as_bool()
    {
        return Err("control_failed: UI Automation element is disabled".into());
    }
    context.deadline.ensure_remaining()?;
    control
        .commit(|| unsafe { selection.pattern.Select() })?
        .map_err(|e| {
            windows_control_attempt_error(&format!(
                "IUIAutomationSelectionItemPattern::Select HRESULT(0x{:08X})",
                e.code().0 as u32
            ))
        })?;
    confirm_selection(
        context
            .deadline
            .expires_at
            .min(Instant::now() + Duration::from_secs(1)),
        || {
            let current = resolve_uia_element(context, surface, element)?;
            let verified = target(context, &current, surface, element)?
                .ok_or_else(|| "stale_element: selection pattern disappeared".to_string())?;
            context.deadline.ensure_remaining()?;
            if !unsafe {
                context
                    .automation
                    .CompareElements(&selection.container, &verified.container)
            }
            .map_err(|e| uia_error("IUIAutomation::CompareElements", &e))?
            .as_bool()
            {
                return Err("stale_element: selection container changed".into());
            }
            if !verified.selected(context)? {
                return Ok(false);
            }
            // When the provider exposes the selection set, confirm replacement,
            // not merely membership. Partial providers still confirm the target.
            let Some(container_pattern) = optional_uia_pattern::<IUIAutomationSelectionPattern>(
                context,
                &verified.container,
                UIA_SelectionPatternId,
            )?
            else {
                return Ok(true);
            };
            context.deadline.ensure_remaining()?;
            let items = unsafe { container_pattern.GetCurrentSelection() }
                .map_err(|e| uia_error("IUIAutomationSelectionPattern::GetCurrentSelection", &e))?;
            context.deadline.ensure_remaining()?;
            if unsafe { items.Length() }
                .map_err(|e| uia_error("IUIAutomationElementArray::Length", &e))?
                != 1
            {
                return Ok(false);
            }
            context.deadline.ensure_remaining()?;
            let item = unsafe { items.GetElement(0) }
                .map_err(|e| uia_error("IUIAutomationElementArray::GetElement", &e))?;
            context.deadline.ensure_remaining()?;
            unsafe { context.automation.CompareElements(&current, &item) }
                .map(|v| v.as_bool())
                .map_err(|e| uia_error("IUIAutomation::CompareElements", &e))
        },
        Instant::now,
        std::thread::sleep,
    )
}

fn confirm_selection(
    expires_at: Instant,
    mut observe: impl FnMut() -> Result<bool, String>,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(Duration),
) -> Result<(), String> {
    loop {
        if now() >= expires_at {
            return Err(windows_control_attempt_error(
                "Select postcondition timeout",
            ));
        }
        let selected = observe().map_err(|e| windows_control_attempt_error(&e))?;
        let remaining = expires_at.saturating_duration_since(now());
        if remaining.is_zero() {
            return Err(windows_control_attempt_error(
                "Select postcondition timeout",
            ));
        }
        if selected {
            return Ok(());
        }
        wait(remaining.min(Duration::from_millis(10)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn selection_confirmation_waits_for_state_without_replaying_effect() {
        let start = Instant::now();
        let now = Cell::new(start);
        let reads = Cell::new(0);
        confirm_selection(
            start + Duration::from_secs(1),
            || {
                reads.set(reads.get() + 1);
                Ok(reads.get() == 3)
            },
            || now.get(),
            |delay| now.set(now.get() + delay),
        )
        .unwrap();
        assert_eq!(reads.get(), 3);
    }

    #[test]
    fn selection_confirmation_errors_and_expiry_are_uncertain() {
        let start = Instant::now();
        for failed_read in [false, true] {
            let now = Cell::new(start);
            let error = confirm_selection(
                start + Duration::from_millis(25),
                || {
                    if failed_read {
                        Err("stale_element: changed".into())
                    } else {
                        Ok(false)
                    }
                },
                || now.get(),
                |delay| now.set(now.get() + delay),
            )
            .unwrap_err();
            assert!(error.starts_with("outcome_unknown:"));
            assert!(now.get() <= start + Duration::from_millis(25));
        }
        let error = confirm_selection(
            start,
            || panic!("expired read"),
            || start,
            |_| panic!("expired wait"),
        )
        .unwrap_err();
        assert!(error.starts_with("outcome_unknown:"));
    }
}
