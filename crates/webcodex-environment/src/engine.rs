use crate::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciliation {
    Complete,
    Missing,
    Unknown,
}

/// Host effects are behind one adapter, so retries and ordering cannot diverge
/// between Desktop and CLI. Reconciliation must not create credentials/services.
pub trait EnvironmentBackend {
    async fn reconcile(
        &mut self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
        step: SetupStep,
    ) -> SetupResultValue<Reconciliation>;
    async fn apply(
        &mut self,
        store: &EnvironmentStore,
        record: &mut EnvironmentRecord,
        step: SetupStep,
        secrets: &SetupSecrets,
    ) -> SetupResultValue<()>;
    async fn observe(
        &mut self,
        store: &EnvironmentStore,
        record: &EnvironmentRecord,
    ) -> SetupResultValue<RuntimeObservation>;
}

pub struct EnvironmentSetup<B> {
    pub backend: B,
}

impl<B: EnvironmentBackend> EnvironmentSetup<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub async fn configure(
        &mut self,
        store: &EnvironmentStore,
        request: SetupRequest,
        secrets: &SetupSecrets,
        mut progress: impl FnMut(SetupProgress),
    ) -> SetupResultValue<SetupResult> {
        let lock = store.lock()?;
        self.configure_under_lock(store, &lock, request, secrets, &mut progress)
            .await
    }

    pub(crate) async fn configure_under_lock(
        &mut self,
        store: &EnvironmentStore,
        _lock: &crate::storage::EnvironmentLock,
        request: SetupRequest,
        secrets: &SetupSecrets,
        mut progress: impl FnMut(SetupProgress),
    ) -> SetupResultValue<SetupResult> {
        ensure_upgrade_idle_under_lock(store)?;
        let mut journal = match store.load_journal()? {
            Some(existing) => {
                if existing.schema_version != ENVIRONMENT_SCHEMA
                    || existing.environment.request != request
                {
                    return Err(SetupDiagnostic::new("environment_conflict", "Saved setup belongs to a different Server, project, account or runtime installation", "Resume the saved setup; use add-project or an explicit migration to change it"));
                }
                existing
            }
            None => {
                if store.load_environment()?.is_some() {
                    return Err(SetupDiagnostic::new(
                        "journal_missing",
                        "An environment exists without its setup journal",
                        "Inspect and recover the existing environment before configuring another",
                    ));
                }
                let environment = EnvironmentRecord {
                    schema_version: ENVIRONMENT_SCHEMA,
                    environment_id: uuid::Uuid::new_v4().to_string(),
                    request,
                    username: None,
                    runner_client_id: None,
                    projects: Vec::new(),
                    configured: false,
                };
                SetupJournal {
                    schema_version: ENVIRONMENT_SCHEMA,
                    operation_id: uuid::Uuid::new_v4().to_string(),
                    environment,
                    steps: BTreeMap::new(),
                    last_diagnostic: None,
                }
            }
        };
        store.save_journal(&journal)?;
        for step in journal.environment.request.steps() {
            let prior = journal.steps.get(&step).copied();
            let reconciled = self
                .backend
                .reconcile(store, &mut journal.environment, step)
                .await;
            let result = match reconciled {
                Ok(Reconciliation::Complete) => Ok(()),
                Ok(Reconciliation::Unknown) => Err(SetupDiagnostic::new("operation_uncertain", "The previous external operation has an unknown result", "Inspect its actual state before retrying; do not start another instance")),
                Ok(Reconciliation::Missing) if step == SetupStep::RunnerEnrollment && prior.is_some() && !secrets.replacement_pairing_code => {
                    Err(SetupDiagnostic::new("pairing_recovery_required", "Pairing may have consumed its code, but saved credentials could not be verified", "Recover the credentials or explicitly resume with a new pairing code"))
                }
                Ok(Reconciliation::Missing) => {
                    journal.steps.insert(step, StepState::Started);
                    journal.last_diagnostic = None;
                    store.save_journal(&journal)?;
                    progress(SetupProgress { operation_id: journal.operation_id.clone(), step, state: StepState::Started });
                    self.backend.apply(store, &mut journal.environment, step, secrets).await
                }
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                journal.last_diagnostic = Some(error.clone());
                // Preserve the original actionable failure if writing its diagnostic fails.
                let _ = store.save_journal(&journal);
                return Err(error);
            }
            journal.steps.insert(step, StepState::Complete);
            store.save_journal(&journal)?;
            progress(SetupProgress {
                operation_id: journal.operation_id.clone(),
                step,
                state: StepState::Complete,
            });
        }
        let observation = self.backend.observe(store, &journal.environment).await?;
        journal.environment.configured = true;
        store.save_journal(&journal)?;
        store.save_environment(&journal.environment)?;
        Ok(SetupResult {
            environment: journal.environment,
            observation,
        })
    }

    pub async fn resume(
        &mut self,
        store: &EnvironmentStore,
        secrets: &SetupSecrets,
        progress: impl FnMut(SetupProgress),
    ) -> SetupResultValue<SetupResult> {
        let journal = store.load_journal()?.ok_or_else(|| {
            SetupDiagnostic::new(
                "setup_not_found",
                "No saved setup was found",
                "Configure this environment first",
            )
        })?;
        self.configure(store, journal.environment.request, secrets, progress)
            .await
    }
}

impl EnvironmentSetup<NativeEnvironment> {
    /// Explicitly add this machine's first project without rebinding the
    /// existing Server or replacing its user identity.
    pub async fn enable_runner(
        &mut self,
        store: &EnvironmentStore,
        project: std::path::PathBuf,
        secrets: &SetupSecrets,
        progress: impl FnMut(SetupProgress),
    ) -> SetupResultValue<SetupResult> {
        let lock = store.lock()?;
        ensure_upgrade_idle_under_lock(store)?;
        let record = store.load_environment()?.ok_or_else(|| {
            SetupDiagnostic::new(
                "not_configured",
                "Configure this environment first",
                "Run environment configure",
            )
        })?;
        if record.runner_client_id.is_some() {
            drop(lock);
            return self.backend.add_project(store, &project).await;
        }
        let project = project.canonicalize().map_err(|_| SetupDiagnostic::io())?;
        let mut request = record.request.clone();
        request.project = Some(project);
        crate::native::validate_existing_request(store, &request)?;
        let mut journal = store.load_journal()?.ok_or_else(SetupDiagnostic::io)?;
        if journal.environment.request != record.request && journal.environment.request != request {
            return Err(SetupDiagnostic::new(
                "environment_conflict",
                "Another setup transition is unfinished",
                "Resume that operation before adding this project",
            ));
        }
        if journal.environment.request == record.request {
            journal.operation_id = uuid::Uuid::new_v4().to_string();
            journal.environment.request = request.clone();
            journal.environment.configured = false;
            journal.steps.remove(&SetupStep::Preflight);
            journal.steps.remove(&SetupStep::Readiness);
            journal.last_diagnostic = None;
            store.save_journal(&journal)?;
        }
        drop(lock);
        self.configure(store, request, secrets, progress).await
    }
}

#[cfg(test)]
mod tests;
