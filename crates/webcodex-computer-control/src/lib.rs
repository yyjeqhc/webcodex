//! Local desktop control ownership. Observation and input are independent facts.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControlSnapshot {
    pub epoch: String,
    pub paused: bool,
    pub pause_persistence_failed: bool,
    pub observing: bool,
    pub controlling: bool,
    pub last_observation_at_ms: Option<u64>,
    pub last_control_at_ms: Option<u64>,
}

struct State {
    epoch: String,
    paused: bool,
    pause_persistence_failed: bool,
    observations: usize,
    controls: usize,
    last_observation_at_ms: Option<u64>,
    last_control_at_ms: Option<u64>,
}

pub struct LocalControl {
    state: Mutex<State>,
    persistence: Option<PathBuf>,
}

impl LocalControl {
    pub fn new() -> Arc<Self> {
        Self::ephemeral(false)
    }

    /// Keep all native control effects closed when no interactive local
    /// management endpoint can exist (for example a Session 0 Runner).
    /// The ordinary Runner may still offer Shell, file, and Job tools.
    pub fn disabled() -> Arc<Self> {
        Self::ephemeral(true)
    }

    fn ephemeral(paused: bool) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                epoch: uuid::Uuid::new_v4().to_string(),
                paused,
                pause_persistence_failed: false,
                observations: 0,
                controls: 0,
                last_observation_at_ms: None,
                last_control_at_ms: None,
            }),
            persistence: None,
        })
    }

    /// Missing state is a fresh installation. Malformed or inaccessible state
    /// does not enable input. The private parent is validated by the local broker.
    pub fn persistent(dir: &Path) -> Result<Arc<Self>, String> {
        let path = dir.join("computer-control-state");
        let paused = match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 16 => {
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|_| "computer_control_unavailable")?
                    .take(17)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "computer_control_unavailable")?;
                match bytes.as_slice() {
                    b"paused\n" => true,
                    b"enabled\n" => false,
                    _ => return Err("computer_control_unavailable: invalid saved state".into()),
                }
            }
            Ok(_) => return Err("computer_control_unavailable: invalid saved state".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err("computer_control_unavailable: saved state unavailable".into()),
        };
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                epoch: uuid::Uuid::new_v4().to_string(),
                paused,
                pause_persistence_failed: false,
                observations: 0,
                controls: 0,
                last_observation_at_ms: None,
                last_control_at_ms: None,
            }),
            persistence: Some(path),
        }))
    }

    pub fn snapshot(&self) -> Result<ControlSnapshot, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "computer_control_unavailable")?;
        Ok(snapshot(&state))
    }

    /// Session/Runner replacement invalidates queued control without releasing
    /// a local pause or changing independent native input reservations.
    pub fn invalidate(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "computer_control_unavailable")?;
        state.epoch = uuid::Uuid::new_v4().to_string();
        Ok(())
    }

    /// The exact observed epoch fences local actions to the observed instance.
    /// Resume cannot race unfinished input, and never releases Browser handoffs.
    pub fn set_paused(
        &self,
        expected_epoch: &str,
        paused: bool,
    ) -> Result<ControlSnapshot, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "computer_control_unavailable")?;
        if state.epoch != expected_epoch {
            return Err("computer_control_changed: refresh local status".into());
        }
        if !paused && state.controls != 0 {
            return Err("computer_control_busy: wait for in-flight control to finish".into());
        }
        let changed = state.paused != paused;
        if changed {
            // Publish the stop before persistence I/O; a failed write never
            // permits further input. Successful resume requires durable state.
            if paused {
                state.paused = true;
                state.epoch = uuid::Uuid::new_v4().to_string();
            }
        }
        // Retry persistence even when a previous failed pause already closed
        // the in-memory gate. Only durable confirmation can report success.
        if let Some(path) = &self.persistence {
            if let Err(error) = persist(path, paused) {
                if paused {
                    state.pause_persistence_failed = true;
                }
                return Err(error);
            }
        }
        state.pause_persistence_failed = false;
        if changed && !paused {
            state.paused = false;
            state.epoch = uuid::Uuid::new_v4().to_string();
        }
        Ok(snapshot(&state))
    }

    pub fn begin(
        self: &Arc<Self>,
        control: bool,
        expected_epoch: Option<&str>,
    ) -> Result<ActivityGuard, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "computer_control_unavailable")?;
        if control {
            check_control(&state, expected_epoch.unwrap_or(&state.epoch))?;
            state.controls = state
                .controls
                .checked_add(1)
                .ok_or("computer_control_unavailable")?;
        } else {
            state.observations = state
                .observations
                .checked_add(1)
                .ok_or("computer_control_unavailable")?;
        }
        Ok(ActivityGuard {
            owner: self.clone(),
            control_epoch: control.then(|| state.epoch.clone()),
        })
    }
}

fn snapshot(state: &State) -> ControlSnapshot {
    ControlSnapshot {
        epoch: state.epoch.clone(),
        paused: state.paused,
        pause_persistence_failed: state.pause_persistence_failed,
        observing: state.observations != 0,
        controlling: state.controls != 0,
        last_observation_at_ms: state.last_observation_at_ms,
        last_control_at_ms: state.last_control_at_ms,
    }
}

fn persist(path: &Path, paused: bool) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("computer_control_unavailable")?;
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "computer_control_unavailable")?;
    file.write_all(if paused { b"paused\n" } else { b"enabled\n" })
        .map_err(|_| "computer_control_unavailable")?;
    file.as_file()
        .sync_all()
        .map_err(|_| "computer_control_unavailable")?;
    file.persist(path)
        .map_err(|_| "computer_control_unavailable")?;
    Ok(())
}

pub struct ActivityGuard {
    owner: Arc<LocalControl>,
    control_epoch: Option<String>,
}

fn check_control(state: &State, expected_epoch: &str) -> Result<(), String> {
    if state.paused {
        return Err("computer_control_paused: resume Computer Control on the local device".into());
    }
    if expected_epoch != state.epoch {
        return Err("computer_control_changed: reacquire current Computer state before another control request".into());
    }
    Ok(())
}

impl ActivityGuard {
    /// Admission alone never authorizes an effect after a lock or native preflight.
    pub fn check_control(&self) -> Result<(), String> {
        let expected = self
            .control_epoch
            .as_deref()
            .ok_or("computer_control_unavailable: control activity required")?;
        let state = self
            .owner
            .state
            .lock()
            .map_err(|_| "computer_control_unavailable")?;
        check_control(&state, expected)
    }

    /// Linearize the first effect against pause/invalidation after all waiting
    /// and preflight. The closure must enter native dispatch immediately. Once
    /// committed, the same operation may finish its sequence and cleanup while
    /// pause independently closes admission; its guard still blocks resume.
    pub fn commit<T>(&self, effect: impl FnOnce() -> T) -> Result<T, String> {
        self.check_control()?;
        Ok(effect())
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.owner.state.lock() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|value| value.as_millis().min(u64::MAX as u128) as u64);
            if self.control_epoch.is_some() {
                state.controls -= 1;
                state.last_control_at_ms = now;
            } else {
                state.observations -= 1;
                state.last_observation_at_ms = now;
            }
        }
    }
}

/// Exact local-management error that means this process has no interactive
/// desktop. Callers may continue as a non-Computer Runner with the gate closed.
#[cfg(windows)]
pub const INTERACTIVE_SESSION_REQUIRED_ERROR: &str =
    "computer_control_unavailable: interactive session required";

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{request, serve};

#[cfg(not(windows))]
pub fn request(_: &Path, _: &Path, _: &LocalRequest) -> Result<ControlSnapshot, String> {
    Err("computer_control_unavailable: local control UI requires Windows".into())
}
#[cfg(not(windows))]
pub fn serve(_: &Path, _: Arc<LocalControl>) -> Result<(), String> {
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalRequest {
    Status,
    Pause { expected_epoch: String },
    Resume { expected_epoch: String },
}

impl LocalRequest {
    pub fn apply(&self, control: &LocalControl) -> Result<ControlSnapshot, String> {
        match self {
            Self::Status => control.snapshot(),
            Self::Pause { expected_epoch } => control.set_paused(expected_epoch, true),
            Self::Resume { expected_epoch } => control.set_paused(expected_epoch, false),
        }
    }
}

#[cfg(test)]
mod tests;
