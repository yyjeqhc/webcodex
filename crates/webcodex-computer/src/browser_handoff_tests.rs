use super::*;
fn window(pid: u32, native_id: u32, title: &str) -> PlatformWindow {
    PlatformWindow {
        pid,
        native_id,
        x: 0,
        y: 0,
        identity_hash: [native_id as u8; 32],
        application: "Chrome".into(),
        title: title.into(),
        width: 800,
        height: 600,
        focused: Some(false),
        active: Some(false),
    }
}
#[test]
fn handoff_uses_exact_process_not_a_matching_title_or_renderer() {
    let found = exact_process_window(
        vec![window(11, 1, "same title"), window(22, 2, "same title")],
        22,
    )
    .unwrap();
    assert_eq!(found.native_id, 2);
    assert!(exact_process_window(vec![window(11, 1, "same title")], 22).is_err());
}
#[test]
fn multiple_windows_and_truncated_inventory_are_not_guessed() {
    assert!(
        exact_process_window(vec![window(22, 1, "active"), window(22, 2, "other")], 22).is_err()
    );
    let mut first = window(22, 1, "active");
    first.x = 10;
    first.y = 20;
    let mut second = window(22, 2, "other");
    second.x = 900;
    second.y = 20;
    let found =
        exact_process_window_with_bounds(vec![first.clone(), second], 22, Some((10, 20, 800, 600)))
            .unwrap();
    assert_eq!(found.native_id, 1);
    assert!(exact_process_window_with_bounds(
        vec![first.clone(), first],
        22,
        Some((10, 20, 800, 600)),
    )
    .is_err());
    assert!(exact_process_window(
        (0..=MAX_WINDOWS)
            .map(|id| window(22, id as u32, "tab"))
            .collect(),
        22
    )
    .is_err());
    assert!(exact_process_window(Vec::new(), 22).is_err());
    assert!(exact_process_window(vec![window(0, 1, "tab")], 0).is_err());
}
