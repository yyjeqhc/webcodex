//! Process-local observation of canonical potential mutation dispatches.
//! Unlike the Code Mode serialization fence this includes direct calls and all
//! Sessions. It is NOT a filesystem watcher, write lock, or source snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use webcodex_core::validation_source::{
    ValidationSourceFence, ValidationSourceState, MAX_SOURCE_GENERATION,
};

const MAX_TRACKED_PROJECTS: usize = 4096;
const MAX_PRESENTATION_JOBS: usize = 128;

/// Equality-only presentation invalidation, never validation freshness proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PresentationSourceFence {
    epoch: String,
    generation: u64,
    pub(crate) pending_jobs: Vec<String>,
}

#[derive(Debug)]
struct ProjectObservation {
    epoch: String,
    generation: u64,
    active: usize,
    uncertain: bool,
    presentation_unknown: bool,
    presentation_jobs: BTreeSet<String>,
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
            presentation_unknown: false,
            presentation_jobs: BTreeSet::new(),
        }));
        projects.insert(project.to_string(), Arc::clone(&state));
        Some(state)
    }

    pub(crate) fn capture(&self, project: &str) -> Option<ValidationSourceFence> {
        let state = self.project(project)?;
        let state = state.lock().ok()?;
        Some(state.snapshot())
    }

    pub(crate) fn capture_presentation(&self, project: &str) -> Option<PresentationSourceFence> {
        let state = self.project(project)?;
        let state = state.lock().ok()?;
        if state.active != 0
            || state.presentation_unknown
            || state.generation == MAX_SOURCE_GENERATION
        {
            return None;
        }
        Some(PresentationSourceFence {
            epoch: state.epoch.clone(),
            generation: state.generation,
            pending_jobs: state.presentation_jobs.iter().cloned().collect(),
        })
    }

    /// The caller proved these exact Jobs ended under current Project authority.
    /// Compare before removing references; never clear validation uncertainty.
    pub(crate) fn resolve_presentation_jobs(
        &self,
        project: &str,
        fence: &PresentationSourceFence,
    ) -> bool {
        let Some(state) = self.project(project) else {
            return false;
        };
        let Ok(mut state) = state.lock() else {
            return false;
        };
        if state.epoch != fence.epoch
            || state.generation != fence.generation
            || state.active != 0
            || state.presentation_unknown
        {
            return false;
        }
        for id in &fence.pending_jobs {
            state.presentation_jobs.remove(id);
        }
        if !fence.pending_jobs.is_empty() {
            state.advance();
        }
        true
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
            presentation_job: None,
        })
    }
}

pub(crate) struct MutationObservationGuard {
    state: Arc<Mutex<ProjectObservation>>,
    completed: bool,
    presentation_job: Option<String>,
}

impl MutationObservationGuard {
    pub(crate) fn finish(mut self, result: &super::ToolResult) {
        // Returning a Job, losing delivery, or lacking mutation truth cannot
        // prove that the potential writer stopped. Keep this epoch uncertain.
        let output = &result.output;
        if output["execution_state"] != "outcome_unknown"
            && output["failure_kind"] != "outcome_unknown"
        {
            self.presentation_job = output
                .get("job_id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| webcodex_core::workflow_session_contract::is_safe_job_id(id))
                .map(str::to_owned);
        }

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
            if !self.completed {
                if let Some(id) = self.presentation_job.take() {
                    if state.presentation_jobs.len() < MAX_PRESENTATION_JOBS
                        || state.presentation_jobs.contains(&id)
                    {
                        state.presentation_jobs.insert(id);
                    } else {
                        state.presentation_unknown = true;
                    }
                } else {
                    state.presentation_unknown = true;
                }
            }
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
