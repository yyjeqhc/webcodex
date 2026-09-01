use super::*;
#[cfg(target_os = "macos")]
const MACOS_POINTER_READBACK_SETTLE_TIMEOUT: Duration = Duration::from_millis(50);
#[cfg(target_os = "macos")]
const MACOS_POINTER_READBACK_POLL_INTERVAL: Duration = Duration::from_millis(1);

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacPointerNativeGeometry {
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
    rotation_degrees: f64,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacPointerPlan {
    target_x: f64,
    target_y: f64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacPointerPreflight {
    target: MacPointerPlan,
    display_id: CGDirectDisplayID,
    geometry: MacPointerNativeGeometry,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MacPointerInputState {
    buttons_down: u32,
    modifier_flags: CGEventFlags,
}

#[cfg(target_os = "macos")]
fn macos_pointer_native_geometry(display_id: CGDirectDisplayID) -> MacPointerNativeGeometry {
    let bounds = CGDisplayBounds(display_id);
    MacPointerNativeGeometry {
        origin_x: bounds.origin.x,
        origin_y: bounds.origin.y,
        width: bounds.size.width,
        height: bounds.size.height,
        rotation_degrees: CGDisplayRotation(display_id),
    }
}

#[cfg(target_os = "macos")]
fn validate_macos_pointer_native_geometry(
    geometry: MacPointerNativeGeometry,
) -> Result<(), String> {
    if !geometry.origin_x.is_finite()
        || !geometry.origin_y.is_finite()
        || !geometry.width.is_finite()
        || !geometry.height.is_finite()
        || !geometry.rotation_degrees.is_finite()
    {
        return Err(
            "pointer_input_failed: macOS display bounds or rotation is non-finite".to_string(),
        );
    }
    if geometry.width <= 0.0 || geometry.height <= 0.0 {
        return Err("pointer_input_failed: macOS display bounds are empty or invalid".to_string());
    }
    let right = geometry.origin_x + geometry.width;
    let bottom = geometry.origin_y + geometry.height;
    if !right.is_finite()
        || !bottom.is_finite()
        || right <= geometry.origin_x
        || bottom <= geometry.origin_y
    {
        return Err(
            "pointer_input_failed: macOS display bounds cannot form an exact half-open rectangle"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn map_macos_pointer_coordinate(
    source_width: u32,
    source_height: u32,
    geometry: MacPointerNativeGeometry,
    x: u32,
    y: u32,
) -> Result<MacPointerPlan, String> {
    if source_width == 0 || source_height == 0 || x >= source_width || y >= source_height {
        return Err(
            "invalid_request: pointer coordinate is outside snapshot source geometry".to_string(),
        );
    }
    validate_macos_pointer_native_geometry(geometry)?;
    if geometry.rotation_degrees != 0.0 {
        return Err(
            "pointer_input_failed: macOS pointer mapping supports only an exact 0-degree display rotation"
                .to_string(),
        );
    }

    let target_x = geometry.origin_x + (f64::from(x) / f64::from(source_width)) * geometry.width;
    let target_y = geometry.origin_y + (f64::from(y) / f64::from(source_height)) * geometry.height;
    let right = geometry.origin_x + geometry.width;
    let bottom = geometry.origin_y + geometry.height;
    if !target_x.is_finite()
        || !target_y.is_finite()
        || target_x < geometry.origin_x
        || target_x >= right
        || target_y < geometry.origin_y
        || target_y >= bottom
    {
        return Err(
            "pointer_input_failed: macOS pointer target is outside exact display bounds"
                .to_string(),
        );
    }
    Ok(MacPointerPlan { target_x, target_y })
}

#[cfg(target_os = "macos")]
fn macos_pointer_input_state() -> MacPointerInputState {
    let state_id = CGEventSourceStateID::CombinedSessionState;
    let mut buttons_down = 0u32;
    for button in 0..32u32 {
        if CGEventSource::button_state(state_id, CGMouseButton(button)) {
            buttons_down |= 1u32 << button;
        }
    }
    MacPointerInputState {
        buttons_down,
        modifier_flags: CGEventSource::flags_state(state_id),
    }
}

#[cfg(target_os = "macos")]
fn validate_macos_pointer_input_state(
    action: PointerAction,
    state: MacPointerInputState,
) -> Result<(), String> {
    if state.buttons_down != 0 {
        return Err(
            "pointer_input_failed: shared desktop mouse button is already down".to_string(),
        );
    }
    if action == PointerAction::Click
        && state.modifier_flags.intersects(
            CGEventFlags::MaskShift
                | CGEventFlags::MaskControl
                | CGEventFlags::MaskAlternate
                | CGEventFlags::MaskCommand,
        )
    {
        return Err(
            "pointer_input_failed: shared desktop modifier key is already active".to_string(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_macos_pointer_permission() -> Result<(), String> {
    if CGPreflightPostEventAccess() {
        Ok(())
    } else {
        Err("permission_denied: macOS event-posting permission is not granted".to_string())
    }
}

#[cfg(target_os = "macos")]
fn prepare_macos_pointer_plan_with(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
    mut revalidate: impl FnMut(&DisplayRecord) -> Result<CGDirectDisplayID, String>,
    mut native_geometry: impl FnMut(CGDirectDisplayID) -> MacPointerNativeGeometry,
    mut input_state: impl FnMut() -> MacPointerInputState,
    mut permission_preflight: impl FnMut() -> Result<(), String>,
) -> Result<MacPointerPreflight, String> {
    let before_display_id = revalidate(display)?;
    let before_geometry = native_geometry(before_display_id);
    let target =
        map_macos_pointer_coordinate(display.width, display.height, before_geometry, x, y)?;

    let after_display_id = revalidate(display)?;
    let after_geometry = native_geometry(after_display_id);
    if after_display_id != before_display_id || after_geometry != before_geometry {
        return Err(
            "stale_display: macOS display placement or rotation changed during pointer preflight"
                .to_string(),
        );
    }
    validate_macos_pointer_native_geometry(after_geometry)?;
    permission_preflight()?;
    validate_macos_pointer_input_state(action, input_state())?;
    Ok(MacPointerPreflight {
        target,
        display_id: after_display_id,
        geometry: after_geometry,
    })
}

#[cfg(target_os = "macos")]
fn prepare_macos_pointer_plan(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
) -> Result<MacPointerPreflight, String> {
    prepare_macos_pointer_plan_with(
        display,
        x,
        y,
        action,
        find_exact_macos_display,
        macos_pointer_native_geometry,
        macos_pointer_input_state,
        validate_macos_pointer_permission,
    )
}

#[cfg(target_os = "macos")]
type MacPreparedPointerEvents = (
    CFRetained<CGEventSource>,
    CFRetained<CGEvent>,
    Option<CFRetained<CGEvent>>,
    Option<CFRetained<CGEvent>>,
);

#[cfg(target_os = "macos")]
fn prepare_macos_pointer_events(
    action: PointerAction,
    target: MacPointerPlan,
) -> Result<MacPreparedPointerEvents, String> {
    let source =
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok_or_else(|| {
            "pointer_input_failed: macOS CombinedSessionState event source could not be created"
                .to_string()
        })?;
    let point = CGPoint {
        x: target.target_x,
        y: target.target_y,
    };
    let move_event = CGEvent::new_mouse_event(
        Some(&source),
        CGEventType::MouseMoved,
        point,
        CGMouseButton::Left,
    )
    .ok_or_else(|| {
        "pointer_input_failed: macOS MouseMoved event could not be created".to_string()
    })?;
    let move_location = CGEvent::location(Some(&move_event));
    if move_location.x != target.target_x || move_location.y != target.target_y {
        return Err(
            "pointer_input_failed: macOS MouseMoved event did not preserve the exact target"
                .to_string(),
        );
    }

    if action == PointerAction::Move {
        return Ok((source, move_event, None, None));
    }

    let down_event = CGEvent::new_mouse_event(
        Some(&source),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    )
    .ok_or_else(|| {
        "pointer_input_failed: macOS LeftMouseDown event could not be created".to_string()
    })?;
    let up_event = CGEvent::new_mouse_event(
        Some(&source),
        CGEventType::LeftMouseUp,
        point,
        CGMouseButton::Left,
    )
    .ok_or_else(|| {
        "pointer_input_failed: macOS LeftMouseUp event could not be created".to_string()
    })?;
    for event in [&down_event, &up_event] {
        let location = CGEvent::location(Some(event));
        if location.x != target.target_x || location.y != target.target_y {
            return Err(
                "pointer_input_failed: macOS click event did not preserve the exact target"
                    .to_string(),
            );
        }
    }
    Ok((source, move_event, Some(down_event), Some(up_event)))
}

#[cfg(target_os = "macos")]
fn pointer_plan_native_geometry(plan: &PointerPlan) -> MacPointerNativeGeometry {
    MacPointerNativeGeometry {
        origin_x: plan.bounds_origin_x,
        origin_y: plan.bounds_origin_y,
        width: plan.bounds_width,
        height: plan.bounds_height,
        rotation_degrees: plan.rotation_degrees,
    }
}

#[cfg(target_os = "macos")]
fn macos_pointer_final_preflight(plan: &PointerPlan, action: PointerAction) -> Result<(), String> {
    let result = (|| {
        let display_id = find_exact_macos_display(&plan.display)?;
        if display_id != plan.native_display_id {
            return Err("stale_display: macOS display id changed before native post".to_string());
        }
        let geometry = macos_pointer_native_geometry(display_id);
        if geometry != pointer_plan_native_geometry(plan) {
            return Err(
                "stale_display: macOS display placement or rotation changed before native post"
                    .to_string(),
            );
        }
        validate_macos_pointer_native_geometry(geometry)?;
        if geometry.rotation_degrees != 0.0 {
            return Err(
                "pointer_input_failed: macOS pointer mapping supports only an exact 0-degree display rotation"
                    .to_string(),
            );
        }
        validate_macos_pointer_permission()?;
        validate_macos_pointer_input_state(action, macos_pointer_input_state())?;
        Ok(())
    })();
    result.map_err(|error: String| {
        format!(
            "not_started: macOS pointer final preflight failed after generation spend but before native event post: {error}"
        )
    })
}

#[cfg(target_os = "macos")]
fn macos_current_pointer_location() -> Result<(f64, f64), String> {
    let event = CGEvent::new(None).ok_or_else(|| {
        "pointer_input_failed: macOS current cursor event could not be created".to_string()
    })?;
    let location = CGEvent::location(Some(&event));
    if !location.x.is_finite() || !location.y.is_finite() {
        return Err(
            "pointer_input_failed: macOS current cursor location is non-finite".to_string(),
        );
    }
    Ok((location.x, location.y))
}

#[cfg(target_os = "macos")]
fn settle_macos_pointer_exact_observation_with(
    target_x: f64,
    target_y: f64,
    timeout: Duration,
    poll_interval: Duration,
    mut cursor_readback: impl FnMut() -> Result<(f64, f64), String>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<(), String> {
    if timeout.is_zero() || poll_interval.is_zero() {
        return Err(
            "pointer_input_failed: macOS cursor readback settle bounds are invalid".to_string(),
        );
    }
    let deadline = now() + timeout;
    let mut first_observation = true;
    loop {
        if !first_observation && now() >= deadline {
            return Err(
                "pointer_input_failed: macOS exact cursor target was not observed before bounded readback settle deadline"
                    .to_string(),
            );
        }
        first_observation = false;
        if cursor_readback().is_ok_and(|cursor| cursor == (target_x, target_y)) {
            return Ok(());
        }
        let observed_at = now();
        if observed_at >= deadline {
            return Err(
                "pointer_input_failed: macOS exact cursor target was not observed before bounded readback settle deadline"
                    .to_string(),
            );
        }
        sleep(poll_interval.min(deadline.saturating_duration_since(observed_at)));
    }
}

#[cfg(target_os = "macos")]
fn settle_macos_pointer_exact_observation(target_x: f64, target_y: f64) -> Result<(), String> {
    settle_macos_pointer_exact_observation_with(
        target_x,
        target_y,
        MACOS_POINTER_READBACK_SETTLE_TIMEOUT,
        MACOS_POINTER_READBACK_POLL_INTERVAL,
        macos_current_pointer_location,
        Instant::now,
        std::thread::sleep,
    )
}

#[cfg(target_os = "macos")]
fn macos_pointer_outcome_unknown(message: &str) -> String {
    format!("outcome_unknown: {message}")
}

#[cfg(target_os = "macos")]
fn dispatch_macos_pointer_with(
    action: PointerAction,
    target_x: f64,
    target_y: f64,
    mut final_preflight: impl FnMut() -> Result<(), String>,
    mut post_move: impl FnMut() -> Result<(), String>,
    mut exact_cursor_observation: impl FnMut(f64, f64) -> Result<(), String>,
    mut second_click_state: impl FnMut() -> Result<(), String>,
    mut post_down: impl FnMut() -> Result<(), String>,
    mut post_up: impl FnMut() -> Result<(), String>,
    mut left_button_down: impl FnMut() -> Result<bool, String>,
) -> Result<bool, String> {
    final_preflight().map_err(|error| {
        if error.starts_with("not_started:") {
            error
        } else {
            format!(
                "not_started: macOS pointer final preflight failed after generation spend but before native event post: {error}"
            )
        }
    })?;

    post_move().map_err(|_| {
        macos_pointer_outcome_unknown("macOS MouseMoved post outcome could not be proven")
    })?;
    exact_cursor_observation(target_x, target_y).map_err(|_| {
        macos_pointer_outcome_unknown(
            "macOS bounded cursor observation did not prove the exact target after MouseMoved",
        )
    })?;
    if action == PointerAction::Move {
        return Ok(true);
    }

    second_click_state().map_err(|_| {
        macos_pointer_outcome_unknown(
            "shared desktop input state changed after the exact pointer move; click button events were not attempted",
        )
    })?;
    post_down().map_err(|_| {
        macos_pointer_outcome_unknown("macOS LeftMouseDown post outcome could not be proven")
    })?;
    post_up().map_err(|_| {
        macos_pointer_outcome_unknown("macOS LeftMouseUp post outcome could not be proven")
    })?;

    exact_cursor_observation(target_x, target_y).map_err(|_| {
        macos_pointer_outcome_unknown(
            "macOS bounded final cursor observation did not prove the exact click target",
        )
    })?;
    if left_button_down().map_err(|_| {
        macos_pointer_outcome_unknown("macOS final left-button readback is unavailable")
    })? {
        return Err(macos_pointer_outcome_unknown(
            "macOS left mouse button remained down after click sequence",
        ));
    }
    Ok(true)
}

#[cfg(all(test, target_os = "macos"))]
mod macos_pointer_tests {
    use super::*;
    use std::cell::Cell;

    fn geometry(origin_x: f64, origin_y: f64, width: f64, height: f64) -> MacPointerNativeGeometry {
        MacPointerNativeGeometry {
            origin_x,
            origin_y,
            width,
            height,
            rotation_degrees: 0.0,
        }
    }

    fn display(width: u32, height: u32) -> DisplayRecord {
        DisplayRecord {
            native_identity: vec![1],
            width,
            height,
            primary: true,
        }
    }

    fn clean_input_state() -> MacPointerInputState {
        MacPointerInputState {
            buttons_down: 0,
            modifier_flags: CGEventFlags::empty(),
        }
    }

    #[test]
    fn macos_pointer_mapping_handles_1x_hidpi_origins_and_exact_edges() {
        let one_x = geometry(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            map_macos_pointer_coordinate(1920, 1080, one_x, 0, 0).unwrap(),
            MacPointerPlan {
                target_x: 0.0,
                target_y: 0.0,
            }
        );
        assert_eq!(
            map_macos_pointer_coordinate(1920, 1080, one_x, 1919, 1079).unwrap(),
            MacPointerPlan {
                target_x: 1919.0,
                target_y: 1079.0,
            }
        );

        let hidpi = geometry(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            map_macos_pointer_coordinate(3840, 2160, hidpi, 3839, 2159).unwrap(),
            MacPointerPlan {
                target_x: 1919.5,
                target_y: 1079.5,
            }
        );

        let negative = geometry(-1920.0, -120.0, 1920.0, 1080.0);
        assert_eq!(
            map_macos_pointer_coordinate(1920, 1080, negative, 1919, 1079).unwrap(),
            MacPointerPlan {
                target_x: -1.0,
                target_y: 959.0,
            }
        );

        let positive = geometry(1920.0, 120.0, 2560.0, 1440.0);
        assert_eq!(
            map_macos_pointer_coordinate(5120, 2880, positive, 5119, 2879).unwrap(),
            MacPointerPlan {
                target_x: 4479.5,
                target_y: 1559.5,
            }
        );
    }

    #[test]
    fn macos_pointer_mapping_rejects_source_edges_invalid_bounds_and_rotation() {
        let valid = geometry(0.0, 0.0, 1920.0, 1080.0);
        for (x, y) in [(1920, 0), (0, 1080)] {
            let error = map_macos_pointer_coordinate(1920, 1080, valid, x, y).unwrap_err();
            assert!(error.starts_with("invalid_request:"), "{error}");
        }

        for invalid in [
            geometry(0.0, 0.0, 0.0, 1080.0),
            geometry(0.0, 0.0, -1.0, 1080.0),
            geometry(f64::NAN, 0.0, 1920.0, 1080.0),
            geometry(0.0, 0.0, f64::INFINITY, 1080.0),
        ] {
            let error = map_macos_pointer_coordinate(1920, 1080, invalid, 0, 0).unwrap_err();
            assert!(error.starts_with("pointer_input_failed:"), "{error}");
        }

        let mut rotated = valid;
        rotated.rotation_degrees = 90.0;
        let error = map_macos_pointer_coordinate(1920, 1080, rotated, 0, 0).unwrap_err();
        assert!(error.contains("0-degree"), "{error}");

        let mut invalid_rotation = valid;
        invalid_rotation.rotation_degrees = f64::NAN;
        let error = map_macos_pointer_coordinate(1920, 1080, invalid_rotation, 0, 0).unwrap_err();
        assert!(error.contains("non-finite"), "{error}");
    }

    #[test]
    fn macos_pointer_preflight_revalidates_bounds_rotation_and_existing_display_fence() {
        let display = display(3840, 2160);
        for changed in [
            geometry(0.0, 0.0, 1919.0, 1080.0),
            MacPointerNativeGeometry {
                rotation_degrees: 90.0,
                ..geometry(0.0, 0.0, 1920.0, 1080.0)
            },
        ] {
            let observations = Cell::new(0usize);
            let error = prepare_macos_pointer_plan_with(
                &display,
                100,
                100,
                PointerAction::Move,
                |_| Ok(7),
                |_| {
                    let call = observations.get();
                    observations.set(call + 1);
                    if call == 0 {
                        geometry(0.0, 0.0, 1920.0, 1080.0)
                    } else {
                        changed
                    }
                },
                clean_input_state,
                || Ok(()),
            )
            .expect_err("placement or rotation changes must stale the display");
            assert!(error.starts_with("stale_display:"), "{error}");
            assert_eq!(observations.get(), 2);
        }

        let validations = Cell::new(0usize);
        let error = prepare_macos_pointer_plan_with(
            &display,
            100,
            100,
            PointerAction::Move,
            |_| {
                let call = validations.get();
                validations.set(call + 1);
                if call == 0 {
                    Ok(7)
                } else {
                    Err("stale_display: simulated M3 identity/source geometry change".to_string())
                }
            },
            |_| geometry(0.0, 0.0, 1920.0, 1080.0),
            clean_input_state,
            || Ok(()),
        )
        .expect_err("existing exact display fence must remain authoritative");
        assert!(error.starts_with("stale_display:"), "{error}");
        assert_eq!(validations.get(), 2);
    }

    #[test]
    fn macos_pointer_shared_input_preflight_distinguishes_move_and_click() {
        let button_down = MacPointerInputState {
            buttons_down: 1 << 17,
            modifier_flags: CGEventFlags::empty(),
        };
        for action in [PointerAction::Move, PointerAction::Click] {
            let error = validate_macos_pointer_input_state(action, button_down).unwrap_err();
            assert!(error.contains("mouse button"), "{error}");
        }

        for modifier in [
            CGEventFlags::MaskShift,
            CGEventFlags::MaskControl,
            CGEventFlags::MaskAlternate,
            CGEventFlags::MaskCommand,
        ] {
            let state = MacPointerInputState {
                buttons_down: 0,
                modifier_flags: modifier,
            };
            validate_macos_pointer_input_state(PointerAction::Move, state)
                .expect("ordinary modifiers do not widen move policy into click policy");
            let error =
                validate_macos_pointer_input_state(PointerAction::Click, state).unwrap_err();
            assert!(error.contains("modifier"), "{error}");
        }
    }

    #[test]
    fn macos_pointer_permission_denial_is_definite_pre_effect() {
        let display = display(3840, 2160);
        let error = prepare_macos_pointer_plan_with(
            &display,
            0,
            0,
            PointerAction::Move,
            |_| Ok(7),
            |_| geometry(0.0, 0.0, 1920.0, 1080.0),
            clean_input_state,
            || Err("permission_denied: macOS event-posting permission is not granted".to_string()),
        )
        .expect_err("permission failure must stay before the generation spend boundary");
        assert!(error.starts_with("permission_denied:"), "{error}");
    }

    #[test]
    fn macos_pointer_event_construction_is_exact_and_non_effecting() {
        let target = MacPointerPlan {
            target_x: 1919.5,
            target_y: 1079.5,
        };
        let (source, move_event, down, up) =
            prepare_macos_pointer_events(PointerAction::Move, target).unwrap();
        assert_eq!(
            CGEventSource::source_state_id(Some(&source)),
            CGEventSourceStateID::CombinedSessionState
        );
        assert_eq!(CGEvent::r#type(Some(&move_event)), CGEventType::MouseMoved);
        assert!(down.is_none());
        assert!(up.is_none());
        let location = CGEvent::location(Some(&move_event));
        assert_eq!((location.x, location.y), (target.target_x, target.target_y));

        let (source, move_event, down, up) =
            prepare_macos_pointer_events(PointerAction::Click, target).unwrap();
        assert_eq!(
            CGEventSource::source_state_id(Some(&source)),
            CGEventSourceStateID::CombinedSessionState
        );
        let down = down.expect("click prepares exactly one left-down event");
        let up = up.expect("click prepares exactly one left-up event");
        assert_eq!(CGEvent::r#type(Some(&move_event)), CGEventType::MouseMoved);
        assert_eq!(CGEvent::r#type(Some(&down)), CGEventType::LeftMouseDown);
        assert_eq!(CGEvent::r#type(Some(&up)), CGEventType::LeftMouseUp);
        for event in [&move_event, &down, &up] {
            let location = CGEvent::location(Some(event));
            assert_eq!((location.x, location.y), (target.target_x, target.target_y));
        }
    }

    #[test]
    fn macos_pointer_exact_readback_settle_is_bounded_and_strict() {
        use std::cell::Cell;

        fn run_settle(
            cursor_sequence: &[(f64, f64)],
            timeout_ms: u64,
        ) -> (Result<(), String>, usize) {
            let reads = Cell::new(0usize);
            let elapsed = Cell::new(Duration::ZERO);
            let epoch = Instant::now();
            let last_cursor = *cursor_sequence.last().expect("non-empty cursor sequence");
            let result = settle_macos_pointer_exact_observation_with(
                10.5,
                20.5,
                Duration::from_millis(timeout_ms),
                Duration::from_millis(1),
                || {
                    let read = reads.get();
                    reads.set(read + 1);
                    Ok(cursor_sequence.get(read).copied().unwrap_or(last_cursor))
                },
                || epoch + elapsed.get(),
                |duration| elapsed.set(elapsed.get() + duration),
            );
            (result, reads.get())
        }

        let (result, reads) = run_settle(&[(10.0, 20.5), (10.5, 20.5)], 5);
        result.expect("first mismatch followed by exact readback must settle");
        assert_eq!(reads, 2);

        let (result, reads) =
            run_settle(&[(10.0, 20.5), (10.0, 20.5), (10.0, 20.5), (10.5, 20.5)], 5);
        result.expect("multiple mismatches followed by exact readback must settle");
        assert_eq!(reads, 4);

        let (result, reads) = run_settle(&[(10.0, 20.5)], 3);
        let error = result.expect_err("bounded settle must expire on only mismatches");
        assert!(error.starts_with("pointer_input_failed:"), "{error}");
        assert_eq!(reads, 3);

        let (result, reads) = run_settle(&[(10.500_000_000_1, 20.5)], 3);
        let error =
            result.expect_err("nearby fractional coordinates must not satisfy exact equality");
        assert!(error.starts_with("pointer_input_failed:"), "{error}");
        assert_eq!(reads, 3);
    }

    #[test]
    fn macos_pointer_dispatch_move_preserves_effect_boundary_and_uses_settle() {
        use std::cell::RefCell;

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Err("simulated stale final fence".to_string())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |_, _| {
                trace.borrow_mut().push("cursor_proof");
                Ok(())
            },
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect_err("final preflight must fail before any post");
        assert!(error.starts_with("not_started:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight"]);

        let trace = RefCell::new(Vec::new());
        let success = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Ok(())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |x, y| {
                trace.borrow_mut().push("cursor_proof");
                assert_eq!((x, y), (10.5, 20.5));
                Ok(())
            },
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect("move should succeed after exact observation proof");
        assert!(success);
        assert_eq!(*trace.borrow(), vec!["preflight", "move", "cursor_proof"]);

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Ok(())
            },
            || {
                trace.borrow_mut().push("move");
                Err("post interrupted".to_string())
            },
            |_, _| unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect_err("uncertain MouseMoved post stays outcome_unknown");
        assert!(error.starts_with("outcome_unknown:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight", "move"]);

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Ok(())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |x, y| {
                trace.borrow_mut().push("cursor_proof");
                assert_eq!((x, y), (10.5, 20.5));
                Err("bounded exact observation exhausted".to_string())
            },
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect_err("settle exhaustion after MouseMoved must stay outcome_unknown");
        assert!(error.starts_with("outcome_unknown:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight", "move", "cursor_proof"]);
    }

    #[test]
    fn macos_pointer_dispatch_click_preserves_two_phase_safety_and_unknown_boundaries() {
        use std::cell::{Cell, RefCell};

        fn run_click(
            proof_sequence: &[Result<(), &'static str>],
            second_state_ok: bool,
            down_ok: bool,
            up_ok: bool,
            final_left_down: bool,
        ) -> (Result<bool, String>, Vec<&'static str>) {
            let trace = RefCell::new(Vec::new());
            let proof_reads = Cell::new(0usize);
            let last_proof = *proof_sequence.last().expect("non-empty proof sequence");
            let result = dispatch_macos_pointer_with(
                PointerAction::Click,
                10.5,
                20.5,
                || {
                    trace.borrow_mut().push("preflight");
                    Ok(())
                },
                || {
                    trace.borrow_mut().push("move");
                    Ok(())
                },
                |x, y| {
                    trace.borrow_mut().push("cursor_proof");
                    assert_eq!((x, y), (10.5, 20.5));
                    let read = proof_reads.get();
                    proof_reads.set(read + 1);
                    proof_sequence
                        .get(read)
                        .copied()
                        .unwrap_or(last_proof)
                        .map_err(str::to_string)
                },
                || {
                    trace.borrow_mut().push("second_state");
                    second_state_ok
                        .then_some(())
                        .ok_or_else(|| "dirty".to_string())
                },
                || {
                    trace.borrow_mut().push("down");
                    down_ok
                        .then_some(())
                        .ok_or_else(|| "down interrupted".to_string())
                },
                || {
                    trace.borrow_mut().push("up");
                    up_ok
                        .then_some(())
                        .ok_or_else(|| "up interrupted".to_string())
                },
                || {
                    trace.borrow_mut().push("left_button");
                    Ok(final_left_down)
                },
            );
            (result, trace.into_inner())
        }

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Click,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Err("simulated final fence".to_string())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |_, _| Ok(()),
            || Ok(()),
            || Ok(()),
            || Ok(()),
            || Ok(false),
        )
        .expect_err("click final preflight must fail before MouseMoved");
        assert!(error.starts_with("not_started:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight"]);

        let (error, trace) = run_click(&[Err("move proof exhausted")], true, true, true, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(trace, vec!["preflight", "move", "cursor_proof"]);
        assert!(!trace.contains(&"second_state"));
        assert!(!trace.contains(&"down"));
        assert!(!trace.contains(&"up"));

        let (success, trace) = run_click(&[Ok(()), Ok(())], true, true, true, false);
        assert!(success.unwrap());
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up",
                "cursor_proof",
                "left_button"
            ]
        );

        let (error, trace) = run_click(&[Ok(())], false, true, true, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec!["preflight", "move", "cursor_proof", "second_state"]
        );

        let (error, trace) = run_click(&[Ok(())], true, false, true, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec!["preflight", "move", "cursor_proof", "second_state", "down"]
        );

        let (error, trace) = run_click(&[Ok(())], true, true, false, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up"
            ]
        );

        let (error, trace) = run_click(
            &[Ok(()), Err("final proof exhausted")],
            true,
            true,
            true,
            false,
        );
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up",
                "cursor_proof"
            ]
        );
        assert!(!trace.contains(&"left_button"));

        let (error, trace) = run_click(&[Ok(()), Ok(())], true, true, true, true);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up",
                "cursor_proof",
                "left_button"
            ]
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) struct MacPointerReadOnlyProbe {
    pub(crate) source_width: u32,
    pub(crate) source_height: u32,
    pub(crate) bounds: (f64, f64, f64, f64),
    pub(crate) rotation_degrees: f64,
    pub(crate) mapped_edge: (f64, f64),
    pub(crate) buttons_down: u32,
    pub(crate) modifier_flags: u64,
    pub(crate) event_post_permission: bool,
    pub(crate) prohibited_modifiers_active: bool,
    pub(crate) constructed_event_count: usize,
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_pointer_read_only_probe_for_test(
    display: &PlatformDisplay,
) -> Result<MacPointerReadOnlyProbe, String> {
    let record = DisplayRecord {
        native_identity: display.native_identity.clone(),
        width: display.width,
        height: display.height,
        primary: display.primary,
    };
    let observed_input = std::cell::Cell::new(None);
    let observed_permission = std::cell::Cell::new(false);
    let plan = prepare_macos_pointer_plan_with(
        &record,
        display.width.saturating_sub(1),
        display.height.saturating_sub(1),
        PointerAction::Move,
        find_exact_macos_display,
        macos_pointer_native_geometry,
        || {
            let state = macos_pointer_input_state();
            observed_input.set(Some(state));
            state
        },
        || {
            let granted = CGPreflightPostEventAccess();
            observed_permission.set(granted);
            if granted {
                Ok(())
            } else {
                Err("permission_denied: macOS event-posting permission is not granted".to_string())
            }
        },
    )?;
    let display_id = find_exact_macos_display(&record)?;
    let native = macos_pointer_native_geometry(display_id);
    validate_macos_pointer_native_geometry(native)?;
    let input = observed_input.get().ok_or_else(|| {
        "pointer_input_failed: macOS pointer input state was not observed".to_string()
    })?;
    let prohibited_modifiers_active = input.modifier_flags.intersects(
        CGEventFlags::MaskShift
            | CGEventFlags::MaskControl
            | CGEventFlags::MaskAlternate
            | CGEventFlags::MaskCommand,
    );
    let (_source, move_event, down_event, up_event) =
        prepare_macos_pointer_events(PointerAction::Click, plan.target)?;
    let constructed_event_count =
        1 + usize::from(down_event.is_some()) + usize::from(up_event.is_some());
    for event in [
        Some(move_event.as_ref()),
        down_event.as_deref(),
        up_event.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let location = CGEvent::location(Some(event));
        if (location.x, location.y) != (plan.target.target_x, plan.target.target_y) {
            return Err(
                "pointer_input_failed: macOS prepared event target did not survive readback"
                    .to_string(),
            );
        }
    }
    Ok(MacPointerReadOnlyProbe {
        source_width: display.width,
        source_height: display.height,
        bounds: (
            native.origin_x,
            native.origin_y,
            native.width,
            native.height,
        ),
        rotation_degrees: native.rotation_degrees,
        mapped_edge: (plan.target.target_x, plan.target.target_y),
        buttons_down: input.buttons_down,
        modifier_flags: input.modifier_flags.bits(),
        event_post_permission: observed_permission.get(),
        prohibited_modifiers_active,
        constructed_event_count,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn prepare_pointer(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
) -> Result<PointerPlan, String> {
    let preflight = prepare_macos_pointer_plan(display, x, y, action)?;
    let (source, move_event, click_down_event, click_up_event) =
        prepare_macos_pointer_events(action, preflight.target)?;
    Ok(PointerPlan {
        display: display.clone(),
        native_display_id: preflight.display_id,
        bounds_origin_x: preflight.geometry.origin_x,
        bounds_origin_y: preflight.geometry.origin_y,
        bounds_width: preflight.geometry.width,
        bounds_height: preflight.geometry.height,
        rotation_degrees: preflight.geometry.rotation_degrees,
        target_x: preflight.target.target_x,
        target_y: preflight.target.target_y,
        _source: source,
        move_event,
        click_down_event,
        click_up_event,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn dispatch_pointer(plan: PointerPlan, action: PointerAction) -> Result<bool, String> {
    let down_event = plan.click_down_event.as_deref();
    let up_event = plan.click_up_event.as_deref();
    if action == PointerAction::Click && (down_event.is_none() || up_event.is_none()) {
        return Err(
            "not_started: macOS click plan is incomplete after generation spend but before native event post"
                .to_string(),
        );
    }
    dispatch_macos_pointer_with(
        action,
        plan.target_x,
        plan.target_y,
        || macos_pointer_final_preflight(&plan, action),
        || {
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&plan.move_event));
            Ok(())
        },
        settle_macos_pointer_exact_observation,
        || validate_macos_pointer_input_state(PointerAction::Click, macos_pointer_input_state()),
        || {
            CGEvent::post(CGEventTapLocation::HIDEventTap, down_event);
            Ok(())
        },
        || {
            CGEvent::post(CGEventTapLocation::HIDEventTap, up_event);
            Ok(())
        },
        || {
            Ok(CGEventSource::button_state(
                CGEventSourceStateID::CombinedSessionState,
                CGMouseButton::Left,
            ))
        },
    )
}

#[cfg(target_os = "macos")]
fn key_code(key: &str) -> Result<CGKeyCode, String> {
    match key {
        "enter" => Ok(0x24),
        "tab" => Ok(0x30),
        "escape" => Ok(0x35),
        "home" => Ok(0x73),
        "page_up" => Ok(0x74),
        "end" => Ok(0x77),
        "page_down" => Ok(0x79),
        "arrow_left" => Ok(0x7b),
        "arrow_right" => Ok(0x7c),
        "arrow_down" => Ok(0x7d),
        "arrow_up" => Ok(0x7e),
        _ => Err("invalid_request: computer key is outside the closed vocabulary".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn key_modifier_flags(modifiers: &[String]) -> Result<CGEventFlags, String> {
    validate_key_modifiers(modifiers)?;
    let mut flags = CGEventFlags::empty();
    for modifier in modifiers {
        flags |= match modifier.as_str() {
            "shift" => CGEventFlags::MaskShift,
            "control" => CGEventFlags::MaskControl,
            "option" => CGEventFlags::MaskAlternate,
            "command" => CGEventFlags::MaskCommand,
            _ => unreachable!("validate_key_input closed modifier vocabulary"),
        };
    }
    Ok(flags)
}

#[cfg(all(test, target_os = "macos"))]
mod key_input_native_contract_tests {
    use super::*;

    #[test]
    fn closed_key_codes_and_modifier_flags_are_stable() {
        for (key, expected) in [
            ("enter", 0x24),
            ("tab", 0x30),
            ("escape", 0x35),
            ("home", 0x73),
            ("page_up", 0x74),
            ("end", 0x77),
            ("page_down", 0x79),
            ("arrow_left", 0x7b),
            ("arrow_right", 0x7c),
            ("arrow_down", 0x7d),
            ("arrow_up", 0x7e),
        ] {
            assert_eq!(key_code(key).unwrap(), expected, "{key}");
        }
        assert!(key_code("a").is_err());

        let flags = key_modifier_flags(&["shift".to_string(), "command".to_string()]).unwrap();
        assert!(flags.contains(CGEventFlags::MaskShift));
        assert!(flags.contains(CGEventFlags::MaskCommand));
        assert!(!flags.contains(CGEventFlags::MaskAlternate));

        let mut surface = SurfaceRecord {
            native_id: 1,
            pid: 1,
            identity_hash: [0; 32],
            application: "test".to_string(),
            title: "test".to_string(),
            width: 1,
            height: 1,
        };
        assert_eq!(checked_surface_pid(&surface).unwrap(), 1);
        surface.pid = u32::MAX;
        assert!(checked_surface_pid(&surface)
            .unwrap_err()
            .starts_with("stale_surface:"));
    }
}

#[cfg(target_os = "macos")]
fn text_input_attempt_error(error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "input_failed: AXUIElementSetAttributeValue(AXValue) was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: AXUIElementSetAttributeValue(AXValue) returned AXError({}) after the native text write was attempted",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn key_input(
    surface_id: &str,
    surface: &SurfaceRecord,
    key: &str,
    modifiers: &[String],
) -> Result<Value, String> {
    validate_key_input(key, modifiers)?;
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    if !CGPreflightPostEventAccess() {
        return Err("permission_denied: macOS event-posting permission is not granted".to_string());
    }

    let pid = checked_surface_pid(surface)?;
    let deadline = AxObservationDeadline::new();
    let exact_window = exact_ax_window(surface, &deadline)?;
    let application = unsafe { AXUIElement::new_application(pid) };

    let key_code = key_code(key)?;
    let flags = key_modifier_flags(modifiers)?;
    let key_down = CGEvent::new_keyboard_event(None, key_code, true)
        .ok_or_else(|| "key_input_failed: could not create native key-down event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(None, key_code, false)
        .ok_or_else(|| "key_input_failed: could not create native key-up event".to_string())?;
    CGEvent::set_flags(Some(&key_down), flags);
    CGEvent::set_flags(Some(&key_up), flags);

    // This is the final authority/privacy check before the first effect. Quartz
    // posts keyboard events to a process rather than a specific window, so keep
    // the exact focused-window check as close to dispatch as possible.
    validate_key_input_target(&deadline, &application, &exact_window)?;
    deadline.ensure_remaining()?;

    CGEvent::post_to_pid(pid, Some(&key_down));
    CGEvent::post_to_pid(pid, Some(&key_up));
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "key": key,
        "modifiers": modifiers,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn input_text(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    text: &str,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let text_bytes = validate_input_text(text)?;
    validate_text_input_target(element)?;

    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
    let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
    let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
    // This is deliberately the final read before the effect. The helper may
    // truncate a non-empty string for bounded observation, but emptiness is
    // preserved exactly and caller text is never transformed or normalized.
    let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
    validate_text_input_preflight(enabled, focused, value_settable, current_value.as_deref())?;

    let text_value = CFString::from_str(text);
    prepare_ax_call(&deadline, &current)?;
    let error =
        unsafe { current.set_attribute_value(&CFString::from_static_str("AXValue"), &text_value) };
    if error != AXError::Success {
        return Err(text_input_attempt_error(error));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "text_bytes": text_bytes,
        "success": true,
    }))
}
