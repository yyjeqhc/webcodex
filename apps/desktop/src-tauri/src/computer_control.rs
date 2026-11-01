//! Native local control projection. It never uses remote activity as authority.
use crate::models::DesktopStateSnapshot;
use std::path::PathBuf;
use webcodex_computer_control::{self as local_control, ControlSnapshot, LocalRequest};
pub mod presentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    environment_id: String,
    dir: PathBuf,
    runner: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub target: Target,
    pub state: ControlSnapshot,
}

impl Observation {
    pub fn matches(&self, snapshot: &DesktopStateSnapshot) -> bool {
        snapshot.persistent_environment.as_deref() == Some(self.target.environment_id.as_str())
    }
}

fn target(snapshot: &DesktopStateSnapshot) -> Option<Target> {
    if !cfg!(windows) {
        return None;
    }
    let environment_id = snapshot.persistent_environment.as_ref()?;
    let root = webcodex_environment::default_environment_dir().ok()?;
    let store = webcodex_environment::EnvironmentStore::open(root).ok()?;
    let record = store.load_environment().ok()??;
    if &record.environment_id != environment_id || !record.request.local_runner() {
        return None;
    }
    Some(Target {
        environment_id: environment_id.clone(),
        dir: store.root().join("cu"),
        runner: record.request.binaries.runner,
    })
}

pub fn observe(snapshot: &DesktopStateSnapshot) -> Option<Observation> {
    let target = target(snapshot)?;
    let state = local_control::request(&target.dir, &target.runner, &LocalRequest::Status).ok()?;
    Some(Observation { target, state })
}

pub fn set_paused(
    snapshot: &DesktopStateSnapshot,
    observed: Observation,
    paused: bool,
) -> Result<Observation, String> {
    // Retain the exact observed environment and epoch through the side effect;
    // never redirect an old menu to the newly selected environment.
    if target(snapshot).as_ref() != Some(&observed.target) {
        return Err("computer_control_changed: refresh local status".into());
    }
    let expected_epoch = observed.state.epoch;
    let request = if paused {
        LocalRequest::Pause { expected_epoch }
    } else {
        LocalRequest::Resume { expected_epoch }
    };
    let state = local_control::request(&observed.target.dir, &observed.target.runner, &request)?;
    Ok(Observation {
        target: observed.target,
        state,
    })
}

pub fn status_key(state: Option<&ControlSnapshot>) -> &'static str {
    match state {
        None => "tray.computerUnavailable",
        Some(state) if state.pause_persistence_failed => "tray.computerPauseNotSaved",
        Some(state) if state.paused && state.controlling => "tray.computerPausing",
        Some(state) if state.paused && state.observing => "tray.computerPausedObserving",
        Some(state) if state.paused => "tray.computerPaused",
        Some(state) if state.controlling => "tray.computerControlling",
        Some(state) if state.observing => "tray.computerObserving",
        Some(state) if recent(state.last_control_at_ms) => "tray.computerRecentControl",
        Some(state) if recent(state.last_observation_at_ms) => "tray.computerRecentObservation",
        Some(_) => "tray.computerIdle",
    }
}

pub fn menu_action(state: &ControlSnapshot) -> (bool, &'static str, bool) {
    if state.pause_persistence_failed {
        (true, "tray.computerRetryPause", true)
    } else if state.paused {
        (false, "tray.computerResume", !state.controlling)
    } else {
        (true, "tray.computerPause", true)
    }
}

fn recent(at: Option<u64>) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis());
    at.zip(now).is_some_and(|(at, now)| {
        now.checked_sub(u128::from(at))
            .is_some_and(|age| age < 1500)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paused_observation_and_draining_input_are_distinct() {
        let mut state = ControlSnapshot {
            epoch: "epoch".into(),
            paused: true,
            pause_persistence_failed: false,
            observing: true,
            controlling: false,
            last_observation_at_ms: None,
            last_control_at_ms: None,
        };
        assert_eq!(status_key(Some(&state)), "tray.computerPausedObserving");
        state.controlling = true;
        assert_eq!(status_key(Some(&state)), "tray.computerPausing");
        state.paused = false;
        assert_eq!(status_key(Some(&state)), "tray.computerControlling");
        state.controlling = false;
        assert_eq!(status_key(Some(&state)), "tray.computerObserving");
        assert_eq!(status_key(None), "tray.computerUnavailable");
    }

    #[test]
    fn unsaved_pause_stays_visible_and_offers_retry_instead_of_resume() {
        let mut state = ControlSnapshot {
            epoch: "epoch".into(),
            paused: true,
            pause_persistence_failed: true,
            observing: true,
            controlling: true,
            last_observation_at_ms: None,
            last_control_at_ms: None,
        };
        assert_eq!(status_key(Some(&state)), "tray.computerPauseNotSaved");
        assert_eq!(menu_action(&state), (true, "tray.computerRetryPause", true));
        state.controlling = false;
        assert_eq!(menu_action(&state), (true, "tray.computerRetryPause", true));
        state.pause_persistence_failed = false;
        assert_eq!(status_key(Some(&state)), "tray.computerPausedObserving");
        assert_eq!(menu_action(&state), (false, "tray.computerResume", true));
        state.controlling = true;
        assert_eq!(menu_action(&state), (false, "tray.computerResume", false));
        state.paused = false;
        assert_eq!(menu_action(&state), (true, "tray.computerPause", true));
    }
}
