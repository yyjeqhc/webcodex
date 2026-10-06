use super::*;

fn lightweight() -> Lifecycle {
    let mut state = Lifecycle::default();
    state.mark_bootstrap_complete();
    assert!(state.begin_lightweight());
    assert_eq!(state.destroyed(), OpenAction::None);
    assert_eq!(state.phase, Phase::Lightweight);
    state
}

#[test]
fn ordinary_close_never_destroys_and_unexpected_exit_is_not_swallowed() {
    let mut state = Lifecycle::default();
    assert_eq!(state.close_disposition(), CloseDisposition::HideWindow);
    assert!(!state.prevent_implicit_exit(None));
    assert_eq!(state.destroyed(), OpenAction::None);
    assert_eq!(state.phase, Phase::Loaded);
    assert!(!state.restore_only());
    assert!(!state.can_enter_lightweight());
}

#[test]
fn lightweight_is_gated_until_the_initial_renderer_bootstrap_completes() {
    let mut state = Lifecycle::default();
    assert!(!state.can_enter_lightweight());
    assert!(!state.begin_lightweight());
    assert!(!state.restore_only());
    state.mark_bootstrap_complete();
    assert!(state.can_enter_lightweight());
    assert!(state.restore_only());
}

#[test]
fn lightweight_requires_destroyed_confirmation_and_is_idempotent() {
    let mut state = Lifecycle::default();
    state.mark_bootstrap_complete();
    assert!(state.begin_lightweight());
    assert!(!state.begin_lightweight());
    assert_eq!(state.phase, Phase::Destroying { reopen: false });
    assert!(state.prevent_implicit_exit(None));
    assert!(!state.prevent_implicit_exit(Some(0)));
    state.destroyed();
    assert_eq!(state.phase, Phase::Lightweight);
    assert!(!state.begin_lightweight());
    assert!(state.restore_only());
    assert!(state.prevent_implicit_exit(None));
}

#[test]
fn destroy_failure_restores_loaded_without_losing_completed_bootstrap() {
    let mut state = Lifecycle::default();
    state.mark_bootstrap_complete();
    state.begin_lightweight();
    state.destroy_failed();
    assert_eq!(state.phase, Phase::Loaded);
    assert!(state.can_enter_lightweight());
    assert!(state.restore_only());
    assert!(!state.prevent_implicit_exit(None));
}

#[test]
fn concurrent_open_and_navigation_share_one_recreation_latest_target_wins() {
    let mut state = lightweight();
    assert_eq!(state.open(None), OpenAction::Recreate(1));
    assert_eq!(state.open(None), OpenAction::None);
    assert_eq!(
        state.open(Some(NavigationTarget::Activity)),
        OpenAction::None
    );
    assert_eq!(
        state.open(Some(NavigationTarget::Settings)),
        OpenAction::None
    );
    assert!(!state.begin_lightweight());
    assert_eq!(
        state.pending_navigation().unwrap().target,
        NavigationTarget::Settings
    );
    assert!(state.recreated(1));
    assert_eq!(state.close_disposition(), CloseDisposition::HideWindow);
    assert!(!state.recreated(1));
    assert_eq!(state.open(None), OpenAction::Show);
}

#[test]
fn open_during_destruction_waits_for_label_removal() {
    let mut state = Lifecycle::default();
    state.mark_bootstrap_complete();
    state.begin_lightweight();
    assert_eq!(
        state.open(Some(NavigationTarget::Connections)),
        OpenAction::None
    );
    assert!(state.pending_navigation().is_none());
    assert_eq!(state.destroyed(), OpenAction::Recreate(1));
    assert_eq!(
        state.pending_navigation().unwrap().target,
        NavigationTarget::Connections
    );
    assert!(state.prevent_implicit_exit(None));
}

#[test]
fn failed_recreation_is_retryable_and_stale_completion_cannot_win() {
    let mut state = lightweight();
    state.open(Some(NavigationTarget::Activity));
    state.recreate_failed(1);
    assert_eq!(state.phase, Phase::Lightweight);
    assert!(state.prevent_implicit_exit(None));
    assert_eq!(state.open(None), OpenAction::Recreate(2));
    assert!(!state.recreated(1));
    state.recreate_failed(1);
    assert!(state.recreated(2));
    assert_eq!(
        state.pending_navigation().unwrap().target,
        NavigationTarget::Activity
    );
}

#[test]
fn quit_wins_in_every_phase_and_late_callbacks_cannot_revive_ui() {
    for stage in 0..4 {
        let mut state = Lifecycle::default();
        if stage > 0 {
            state.mark_bootstrap_complete();
            state.begin_lightweight();
        }
        if stage > 1 {
            state.destroyed();
        }
        if stage > 2 {
            state.open(None);
        }
        state.request_exit();
        assert!(!state.prevent_implicit_exit(None));
        assert!(!state.prevent_implicit_exit(Some(0)));
        assert_eq!(state.close_disposition(), CloseDisposition::AllowExit);
        assert_eq!(
            state.open(Some(NavigationTarget::Settings)),
            OpenAction::None
        );
        assert!(!state.begin_lightweight());
        assert!(!state.recreated(1));
        state.destroyed();
        state.destroy_failed();
        state.recreate_failed(1);
        assert_eq!(state.phase, Phase::ExitRequested);
        assert!(state.pending_navigation().is_none());
    }
}

#[test]
fn navigation_ack_consumes_once_and_cannot_clear_a_newer_intent() {
    let mut state = Lifecycle::default();
    state.open(Some(NavigationTarget::Activity));
    let first = state.pending_navigation().unwrap();
    assert_eq!(state.pending_navigation(), Some(first));
    state.open(Some(NavigationTarget::Connections));
    let latest = state.pending_navigation().unwrap();
    state.acknowledge_navigation(first.sequence);
    assert_eq!(state.pending_navigation(), Some(latest));
    state.acknowledge_navigation(latest.sequence);
    assert!(state.pending_navigation().is_none());
    state.acknowledge_navigation(latest.sequence);
    state.open(None);
    assert!(state.pending_navigation().is_none());
    state.open(Some(NavigationTarget::Settings));
    state.mark_bootstrap_complete();
    state.begin_lightweight();
    state.destroyed();
    state.open(None);
    assert!(state.pending_navigation().is_none());
}
