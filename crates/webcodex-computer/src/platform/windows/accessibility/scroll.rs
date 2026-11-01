use super::*;
use webcodex_core::computer_scroll_contract::{
    valid_scroll_percent, ComputerScrollAmount, ComputerScrollDirection, ComputerScrollState,
};

fn pattern(
    context: &UiaContext,
    current: &IUIAutomationElement,
) -> Result<Option<IUIAutomationScrollPattern>, String> {
    optional_uia_pattern(context, current, UIA_ScrollPatternId)
}

fn read_axis(
    context: &UiaContext,
    pattern: &IUIAutomationScrollPattern,
    horizontal: bool,
) -> Result<Option<f64>, String> {
    context.deadline.ensure_remaining()?;
    let scrollable = if horizontal {
        unsafe { pattern.CurrentHorizontallyScrollable() }
    } else {
        unsafe { pattern.CurrentVerticallyScrollable() }
    }
    .map_err(|e| uia_error("IUIAutomationScrollPattern::Scrollable", &e))?
    .as_bool();
    context.deadline.ensure_remaining()?;
    if !scrollable {
        return Ok(None);
    }
    context.deadline.ensure_remaining()?;
    let position = if horizontal {
        unsafe { pattern.CurrentHorizontalScrollPercent() }
    } else {
        unsafe { pattern.CurrentVerticalScrollPercent() }
    }
    .map_err(|e| uia_error("IUIAutomationScrollPattern::ScrollPercent", &e))?;
    context.deadline.ensure_remaining()?;
    if !valid_scroll_percent(position) {
        return Err("scroll_failed: UI Automation scroll position is invalid".into());
    }
    Ok(Some(position))
}

fn read_state(
    context: &UiaContext,
    pattern: &IUIAutomationScrollPattern,
) -> Result<ComputerScrollState, String> {
    Ok(ComputerScrollState {
        horizontal_percent: read_axis(context, pattern, true)?,
        vertical_percent: read_axis(context, pattern, false)?,
    })
}

pub(super) fn state(
    context: &UiaContext,
    current: &IUIAutomationElement,
) -> Result<Option<ComputerScrollState>, String> {
    pattern(context, current)?
        .map(|p| read_state(context, &p))
        .transpose()
}

fn native_amount(direction: ComputerScrollDirection, amount: ComputerScrollAmount) -> ScrollAmount {
    match (direction.increasing(), amount) {
        (true, ComputerScrollAmount::Small) => ScrollAmount_SmallIncrement,
        (true, ComputerScrollAmount::Large) => ScrollAmount_LargeIncrement,
        (false, ComputerScrollAmount::Small) => ScrollAmount_SmallDecrement,
        (false, ComputerScrollAmount::Large) => ScrollAmount_LargeDecrement,
    }
}

/// Native success and one fresh position read are distinct facts. Never issue a
/// second effect to force a position change or recover a failed read.
fn scroll_once(
    before: f64,
    direction: ComputerScrollDirection,
    attempt: impl FnOnce() -> Result<(), String>,
    read_after: impl FnOnce() -> Result<f64, String>,
) -> Result<f64, String> {
    if (direction.increasing() && before == 100.0) || (!direction.increasing() && before == 0.0) {
        return Ok(before);
    }
    attempt()?;
    let after = read_after().map_err(|e| windows_scroll_attempt_error(&e))?;
    if !valid_scroll_percent(after) {
        return Err(windows_scroll_attempt_error(
            "scroll position read-back is invalid",
        ));
    }
    Ok(after)
}

pub(crate) fn scroll(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    direction: ComputerScrollDirection,
    amount: ComputerScrollAmount,
    control: &local_control::ActivityGuard,
) -> Result<Value, String> {
    let target = element.target_fingerprint().ok_or_else(|| {
        "stale_element: UIA element correlation lineage is incomplete".to_string()
    })?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: Windows UI Automation protected content cannot be scrolled".into(),
        );
    }
    if !target.has_positive_evidence() {
        return Err(
            "stale_element: UIA element lacks positive correlation evidence for scrolling".into(),
        );
    }
    let context = UiaContext::new()?;
    let current = resolve_uia_element(&context, surface, element)?;
    context.deadline.ensure_remaining()?;
    if !unsafe { current.CurrentIsEnabled() }
        .map_err(|e| uia_error("IUIAutomationElement::CurrentIsEnabled", &e))?
        .as_bool()
    {
        return Err("scroll_failed: UI Automation scroll container is disabled".into());
    }
    let scroll_pattern = pattern(&context, &current)?.ok_or_else(|| {
        "scroll_failed: exact element does not support ScrollPattern; observe a scrollable container".to_string()
    })?;
    let before =
        read_axis(&context, &scroll_pattern, direction.horizontal())?.ok_or_else(|| {
            "scroll_failed: exact container does not support the requested scroll axis".to_string()
        })?;
    // Resolve the complete exact lineage again after provider preflight.
    let current = resolve_uia_element(&context, surface, element)?;
    let scroll_pattern = pattern(&context, &current)?
        .ok_or_else(|| "stale_element: scroll pattern disappeared before execution".to_string())?;
    context.deadline.ensure_remaining()?;
    if !unsafe { current.CurrentIsEnabled() }
        .map_err(|e| uia_error("IUIAutomationElement::CurrentIsEnabled", &e))?
        .as_bool()
    {
        return Err("scroll_failed: UI Automation scroll container became disabled".into());
    }
    let fresh_before = read_axis(&context, &scroll_pattern, direction.horizontal())?
        .ok_or_else(|| "scroll_failed: scroll axis changed before execution".to_string())?;
    if fresh_before != before {
        return Err("stale_element: container scroll position changed during preflight; re-observe before scrolling".into());
    }
    context.deadline.ensure_remaining()?;
    let after = scroll_once(
        before,
        direction,
        || {
            let step = native_amount(direction, amount);
            let (horizontal, vertical) = if direction.horizontal() {
                (step, ScrollAmount_NoAmount)
            } else {
                (ScrollAmount_NoAmount, step)
            };
            control
                .commit(|| unsafe { scroll_pattern.Scroll(horizontal, vertical) })?
                .map_err(|e| {
                    windows_scroll_attempt_error(&format!(
                        "IUIAutomationScrollPattern::Scroll HRESULT(0x{:08X})",
                        e.code().0 as u32
                    ))
                })?;
            context
                .deadline
                .ensure_remaining()
                .map_err(|e| windows_scroll_attempt_error(&e))
        },
        || {
            let current = resolve_uia_element(&context, surface, element)?;
            let verified = pattern(&context, &current)?.ok_or_else(|| {
                "stale_element: scroll pattern disappeared after execution".to_string()
            })?;
            read_axis(&context, &verified, direction.horizontal())?
                .ok_or_else(|| "scroll_failed: scroll axis disappeared after execution".to_string())
        },
    )?;
    Ok(json!({
        "platform": "windows", "surface_id": surface_id, "element_id": element_id,
        "direction": direction, "amount": amount,
        "position_before": before, "position_after": after, "success": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn bounded_scroll_preserves_commit_rejection_before_native_input() {
        let error = scroll_once(
            20.0,
            ComputerScrollDirection::Down,
            || Err("not_started: local control paused".into()),
            || panic!("no post-effect read after commit rejection"),
        )
        .unwrap_err();
        assert_eq!(error, "not_started: local control paused");
    }

    #[test]
    fn bounded_scroll_boundary_is_a_noop_and_never_reads_after_an_effect() {
        for (before, direction) in [
            (0.0, ComputerScrollDirection::Up),
            (100.0, ComputerScrollDirection::Down),
        ] {
            assert_eq!(
                scroll_once(
                    before,
                    direction,
                    || panic!("no effect at boundary"),
                    || panic!("no post-effect read")
                )
                .unwrap(),
                before
            );
        }
    }

    #[test]
    fn bounded_scroll_does_not_replay_native_failure_or_failed_confirmation() {
        for fail_effect in [false, true] {
            let calls = Cell::new(0);
            let reads = Cell::new(0);
            let error = scroll_once(
                20.0,
                ComputerScrollDirection::Down,
                || {
                    calls.set(calls.get() + 1);
                    if fail_effect {
                        Err(windows_scroll_attempt_error("native failure"))
                    } else {
                        Ok(())
                    }
                },
                || {
                    reads.set(reads.get() + 1);
                    Err("read failure".into())
                },
            )
            .unwrap_err();
            assert!(error.starts_with("outcome_unknown:"));
            assert_eq!(calls.get(), 1);
            assert_eq!(reads.get(), if fail_effect { 0 } else { 1 });
        }
    }

    #[test]
    fn bounded_scroll_preserves_unchanged_or_intermediate_native_positions() {
        for after in [20.0, 21.0, 100.0] {
            assert_eq!(
                scroll_once(20.0, ComputerScrollDirection::Down, || Ok(()), || Ok(after)).unwrap(),
                after
            );
        }
        assert!(scroll_once(
            20.0,
            ComputerScrollDirection::Down,
            || Ok(()),
            || Ok(f64::NAN)
        )
        .unwrap_err()
        .starts_with("outcome_unknown:"));
    }
}
