//! Process-local UI intent only. No Runtime ownership or persistent preferences.
use super::{CloseDisposition, NavigationTarget};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Loaded,
    Destroying { reopen: bool },
    Lightweight,
    Recreating { generation: u32 },
    ExitRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NavigationIntent {
    pub sequence: u32,
    pub target: NavigationTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenAction {
    Show,
    Recreate(u32),
    None,
}

pub(super) struct Lifecycle {
    pub phase: Phase,
    generation: u32,
    navigation_sequence: u32,
    pending_navigation: Option<NavigationIntent>,
    bootstrap_complete: bool,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            phase: Phase::Loaded,
            generation: 0,
            navigation_sequence: 0,
            pending_navigation: None,
            bootstrap_complete: false,
        }
    }
}

impl Lifecycle {
    pub fn close_disposition(&self) -> CloseDisposition {
        if self.phase == Phase::ExitRequested {
            CloseDisposition::AllowExit
        } else {
            CloseDisposition::HideWindow
        }
    }

    pub fn request_exit(&mut self) {
        self.phase = Phase::ExitRequested;
        self.pending_navigation = None;
    }

    pub fn prevent_implicit_exit(&self, code: Option<i32>) -> bool {
        code.is_none()
            && matches!(
                self.phase,
                Phase::Destroying { .. } | Phase::Lightweight | Phase::Recreating { .. }
            )
    }

    pub fn can_enter_lightweight(&self) -> bool {
        self.phase == Phase::Loaded && self.bootstrap_complete
    }

    pub fn mark_bootstrap_complete(&mut self) {
        if self.phase != Phase::ExitRequested {
            self.bootstrap_complete = true;
        }
    }

    pub fn begin_lightweight(&mut self) -> bool {
        if !self.can_enter_lightweight() {
            return false;
        }
        self.phase = Phase::Destroying { reopen: false };
        self.pending_navigation = None;
        true
    }

    pub fn destroy_failed(&mut self) {
        if matches!(self.phase, Phase::Destroying { .. }) {
            self.phase = Phase::Loaded;
        }
    }

    pub fn destroyed(&mut self) -> OpenAction {
        let Phase::Destroying { reopen } = self.phase else {
            return OpenAction::None;
        };
        self.phase = Phase::Lightweight;
        if reopen {
            self.open(None)
        } else {
            OpenAction::None
        }
    }

    pub fn open(&mut self, target: Option<NavigationTarget>) -> OpenAction {
        if self.phase == Phase::ExitRequested {
            return OpenAction::None;
        }
        if let Some(target) = target {
            // Fail closed on the finite counter rather than wrapping an ACK onto
            // a newer intent. One bounded slot, latest explicit target wins.
            if let Some(sequence) = self.navigation_sequence.checked_add(1) {
                self.navigation_sequence = sequence;
                self.pending_navigation = Some(NavigationIntent { sequence, target });
            }
        }
        match self.phase {
            Phase::Loaded => OpenAction::Show,
            Phase::Lightweight => {
                let Some(generation) = self.generation.checked_add(1) else {
                    return OpenAction::None;
                };
                self.generation = generation;
                self.phase = Phase::Recreating { generation };
                OpenAction::Recreate(generation)
            }
            Phase::Destroying { .. } => {
                self.phase = Phase::Destroying { reopen: true };
                OpenAction::None
            }
            Phase::Recreating { .. } | Phase::ExitRequested => OpenAction::None,
        }
    }

    pub fn is_recreating(&self, generation: u32) -> bool {
        self.phase == Phase::Recreating { generation }
    }

    pub fn recreated(&mut self, generation: u32) -> bool {
        if !self.is_recreating(generation) {
            return false;
        }
        self.phase = Phase::Loaded;
        true
    }

    pub fn recreate_failed(&mut self, generation: u32) {
        if self.is_recreating(generation) {
            self.phase = Phase::Lightweight;
        }
    }

    pub fn restore_only(&self) -> bool {
        self.bootstrap_complete
            || matches!(self.phase, Phase::Destroying { .. } | Phase::ExitRequested)
    }

    pub fn pending_navigation(&self) -> Option<NavigationIntent> {
        matches!(self.phase, Phase::Loaded | Phase::Recreating { .. })
            .then_some(self.pending_navigation)
            .flatten()
    }

    pub fn acknowledge_navigation(&mut self, sequence: u32) {
        if self
            .pending_navigation
            .is_some_and(|intent| intent.sequence == sequence)
        {
            self.pending_navigation = None;
        }
    }
}

#[cfg(test)]
mod tests;
