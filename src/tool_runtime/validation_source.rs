//! Process-local observation of canonical potential mutation dispatches.
//! Unlike the Code Mode serialization fence this includes direct calls and all
//! Sessions. It is NOT a filesystem watcher, write lock, or source snapshot.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use webcodex_core::validation_source::{
    ValidationSourceFence, ValidationSourceState, MAX_SOURCE_GENERATION,
};

const MAX_TRACKED_PROJECTS: usize = 4096;

#[derive(Debug)]
struct ProjectObservation {
    epoch: String,
    generation: u64,
    active: usize,
    uncertain: bool,
}

impl ProjectObservation {
    fn advance(&mut self) {
        if self.generation < MAX_SOURCE_GENERATION {
            self.generation += 1;
        } else {
            self.uncertain = true;
        }
    }

    fn snapshot(&self) -> ValidationSourceFence {
        ValidationSourceFence {
            epoch: self.epoch.clone(),
            generation: self.generation,
            quiescent: self.active == 0 && !self.uncertain,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ValidationSourceRegistry {
    projects: Mutex<BTreeMap<String, Arc<Mutex<ProjectObservation>>>>,
}

impl ValidationSourceRegistry {
    fn project(&self, project: &str) -> Option<Arc<Mutex<ProjectObservation>>> {
        let mut projects = self.projects.lock().ok()?;
        if let Some(state) = projects.get(project) {
            return Some(Arc::clone(state));
        }
        // Never evict an active/uncertain writer and silently recreate a clean
        // baseline. Capacity exhaustion makes new Projects unproven instead.
        if projects.len() >= MAX_TRACKED_PROJECTS {
            return None;
        }
        let state = Arc::new(Mutex::new(ProjectObservation {
            epoch: uuid::Uuid::new_v4().simple().to_string(),
            generation: 0,
            active: 0,
            uncertain: false,
        }));
        projects.insert(project.to_string(), Arc::clone(&state));
        Some(state)
    }

    pub(crate) fn capture(&self, project: &str) -> Option<ValidationSourceFence> {
        let state = self.project(project)?;
        let state = state.lock().ok()?;
        Some(state.snapshot())
    }

    pub(crate) fn observe(
        &self,
        project: &str,
        start: Option<&ValidationSourceFence>,
    ) -> ValidationSourceState {
        ValidationSourceState::observe(start, self.capture(project).as_ref())
    }

    pub(crate) fn begin(&self, project: &str) -> Option<MutationObservationGuard> {
        let state = self.project(project)?;
        {
            let mut state = state.lock().ok()?;
            state.advance();
            state.active += 1;
        }
        Some(MutationObservationGuard {
            state,
            completed: false,
        })
    }
}

pub(crate) struct MutationObservationGuard {
    state: Arc<Mutex<ProjectObservation>>,
    completed: bool,
}

impl MutationObservationGuard {
    pub(crate) fn finish(mut self, result: &super::ToolResult) {
        // Returning a Job, losing delivery, or lacking mutation truth cannot
        // prove that the potential writer stopped. Keep this epoch uncertain.
        let output = &result.output;
        self.completed = output
            .get("execution_state")
            .and_then(serde_json::Value::as_str)
            != Some("outcome_unknown")
            && output
                .get("failure_kind")
                .and_then(serde_json::Value::as_str)
                != Some("outcome_unknown")
            && output.get("job_id").filter(|id| !id.is_null()).is_none()
            && (output
                .get("state_changed")
                .and_then(serde_json::Value::as_bool)
                .is_some()
                || output
                    .get("command_completed")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                || output
                    .get("command_started")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false));
    }
}

impl Drop for MutationObservationGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.advance();
            state.active = state.active.saturating_sub(1);
            state.uncertain |= !self.completed;
        }
    }
}

/// Conservative potential-source-effect classification, derived from canonical
/// metadata. Wrappers do not write: their canonical children are observed here.
/// Read-only structured validators can themselves run arbitrary build/test code;
/// that is deliberately OUTSIDE this dispatch fence, hence freshness is unproven.
pub(crate) fn observes_potential_mutation(call: &super::ToolCall) -> bool {
    let name = call.tool_name();
    if matches!(
        name,
        "code_mode_exec"
            | "code_mode_exec_effectful"
            | "code_mode_exec_mutating"
            | "project_validate"
            | "cargo_check"
            | "cargo_test"
            | "go_test"
    ) {
        return false;
    }
    if matches!(
        call,
        super::ToolCall::CargoFmt {
            check: Some(true),
            ..
        }
    ) {
        return false;
    }
    let metadata = webcodex_tool_contracts::runtime_tool_metadata(name);
    metadata.requires_project
        && metadata.effect != webcodex_tool_contracts::ToolEffect::Observe
        && (metadata.shell_like
            || matches!(
                metadata.risk,
                webcodex_tool_contracts::ToolRisk::ProjectWrite
                    | webcodex_tool_contracts::ToolRisk::JobRun
                    | webcodex_tool_contracts::ToolRisk::CheckpointManage
            ))
}

#[cfg(test)]
#[path = "tests/validation_source.rs"]
mod tests;
