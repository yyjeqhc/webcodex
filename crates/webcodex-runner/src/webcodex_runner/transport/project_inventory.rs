use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::webcodex_runner::config::RunnerConfig;
use crate::webcodex_runner::projects::RunnerProjectCache;
use webcodex_core::runner_protocol::{
    RunnerEnvelope, RunnerProjectSummary, ShellProjectInventoryPage, ShellProjectInventoryStatus,
    PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES, PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
};

use super::{format_delay, RetryBackoff, RunnerRuntimeState, StreamTransport};

/// Retry only transient Server-side project-inventory staging pressure on an
/// otherwise healthy streaming connection. Permanent/malformed inventory
/// failures never enter this backoff.
pub(super) const PROJECT_INVENTORY_STAGING_RETRY_BACKOFF_STEPS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

/// Polling periodically refreshes the Server's project projection so external
/// project-registry and Git metadata changes remain discoverable without attaching
/// the full inventory to every poll.
pub(super) const POLLING_PROJECT_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

static NEXT_PROJECT_INVENTORY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn next_project_inventory_sequence() -> u64 {
    NEXT_PROJECT_INVENTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone)]
struct PendingProjectInventoryPage {
    page: ShellProjectInventoryPage,
    next_cursor: usize,
}

#[derive(Debug, Clone)]
pub(super) struct ProjectInventorySync {
    generation: String,
    pub(super) snapshot_sequence: u64,
    pub(super) projects: Vec<RunnerProjectSummary>,
    cursor: usize,
    page_index: u32,
    pending: Option<PendingProjectInventoryPage>,
}

impl ProjectInventorySync {
    pub(super) fn new(projects: Vec<RunnerProjectSummary>) -> Self {
        Self {
            generation: uuid::Uuid::new_v4().simple().to_string(),
            snapshot_sequence: next_project_inventory_sequence(),
            projects,
            cursor: 0,
            page_index: 0,
            pending: None,
        }
    }

    pub(super) fn generation(&self) -> &str {
        &self.generation
    }

    pub(super) fn total_reported(&self) -> usize {
        self.projects.len()
    }

    pub(super) fn current_page(
        &mut self,
    ) -> Result<Option<ShellProjectInventoryPage>, &'static str> {
        if let Some(pending) = &self.pending {
            return Ok(Some(pending.page.clone()));
        }
        if self.cursor >= self.projects.len() && !(self.projects.is_empty() && self.page_index == 0)
        {
            return Ok(None);
        }
        if self.projects.is_empty() {
            let page = ShellProjectInventoryPage {
                generation: self.generation.clone(),
                snapshot_sequence: self.snapshot_sequence,
                page_index: 0,
                total_reported: 0,
                complete: true,
                projects: Vec::new(),
            };
            self.pending = Some(PendingProjectInventoryPage {
                page: page.clone(),
                next_cursor: 0,
            });
            return Ok(Some(page));
        }

        let mut summaries = Vec::new();
        for project in self.projects[self.cursor..].iter() {
            if summaries.len() == PROJECT_INVENTORY_PAGE_MAX_SUMMARIES {
                break;
            }
            summaries.push(project.clone());
            let next_cursor = self.cursor + summaries.len();
            let candidate = ShellProjectInventoryPage {
                generation: self.generation.clone(),
                snapshot_sequence: self.snapshot_sequence,
                page_index: self.page_index,
                total_reported: self.projects.len(),
                complete: next_cursor == self.projects.len(),
                projects: summaries.clone(),
            };
            let bytes = serde_json::to_vec(&candidate)
                .map_err(|_| "project_inventory_serialization_failed")?;
            if bytes.len() > PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES {
                summaries.pop();
                if summaries.is_empty() {
                    return Err("project_inventory_summary_too_large");
                }
                break;
            }
        }
        if summaries.is_empty() {
            return Err("project_inventory_empty_page");
        }
        let next_cursor = self.cursor + summaries.len();
        let page = ShellProjectInventoryPage {
            generation: self.generation.clone(),
            snapshot_sequence: self.snapshot_sequence,
            page_index: self.page_index,
            total_reported: self.projects.len(),
            complete: next_cursor == self.projects.len(),
            projects: summaries,
        };
        if serde_json::to_vec(&page)
            .map_err(|_| "project_inventory_serialization_failed")?
            .len()
            > PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES
        {
            return Err("project_inventory_page_too_large");
        }
        self.pending = Some(PendingProjectInventoryPage {
            page: page.clone(),
            next_cursor,
        });
        Ok(Some(page))
    }

    pub(super) fn acknowledge(
        &mut self,
        status: &ShellProjectInventoryStatus,
    ) -> Result<bool, String> {
        // Explicit Server failure status owns error classification before any
        // local pending-state or success-ack correlation. In particular,
        // staging-capacity rejection can legitimately carry the previous
        // authoritative generation because page 0 was not admitted.
        if status.sync_state == "degraded" || status.sync_state == "failed" {
            return Err(status
                .last_error_code
                .clone()
                .unwrap_or_else(|| "project_inventory_sync_degraded".to_string()));
        }
        let Some(pending) = self.pending.as_ref() else {
            return Ok(self.cursor >= self.projects.len());
        };
        // Successful in-progress/complete acknowledgements remain strictly
        // fenced by generation, total, and exact cursor progress below.
        if status.generation.as_deref() != Some(self.generation()) {
            return Err("project_inventory_ack_generation_mismatch".to_string());
        }
        if status.total_reported != Some(self.projects.len()) {
            return Err("project_inventory_ack_total_mismatch".to_string());
        }
        if status.total_synced != pending.next_cursor {
            return Err("project_inventory_ack_progress_mismatch".to_string());
        }
        let final_page = pending.page.complete;
        let valid_ack = (final_page && status.sync_state == "complete")
            || (!final_page && status.sync_state == "in_progress");
        if !valid_ack {
            return Err("project_inventory_ack_state_mismatch".to_string());
        }
        self.cursor = pending.next_cursor;
        self.page_index = self.page_index.saturating_add(1);
        self.pending = None;
        Ok(final_page)
    }
}

pub(super) fn log_project_inventory_degraded(transport: &str, projects: usize, reason_code: &str) {
    eprintln!(
        "webcodex-runner project inventory sync degraded transport={} projects={} reason_code={}; runner remains online",
        transport, projects, reason_code
    );
}

pub(super) fn paged_sync_after_registration(
    projects: Vec<RunnerProjectSummary>,
) -> ProjectInventorySync {
    ProjectInventorySync::new(projects)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PollingProjectRefresh {
    last_sent_at: Instant,
}

impl PollingProjectRefresh {
    pub(super) fn new(now: Instant) -> Self {
        Self { last_sent_at: now }
    }

    pub(super) fn mark_sent(&mut self, now: Instant) {
        self.last_sent_at = now;
    }

    pub(super) fn should_refresh(&self, project_cache: &RunnerProjectCache, now: Instant) -> bool {
        project_cache.needs_refresh()
            || now.saturating_duration_since(self.last_sent_at) >= POLLING_PROJECT_REFRESH_INTERVAL
    }
}

pub(super) fn polling_projects_for_poll(
    refresh: &PollingProjectRefresh,
    project_cache: &mut RunnerProjectCache,
    cfg: &RunnerConfig,
    shutdown: &AtomicBool,
    now: Instant,
) -> Option<Vec<RunnerProjectSummary>> {
    refresh
        .should_refresh(project_cache, now)
        .then(|| project_cache.get_with_shutdown(cfg, Some(shutdown)))
}

pub(super) fn try_queue_project_inventory_page(
    transport: StreamTransport,
    sync: &mut Option<ProjectInventorySync>,
    out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
) {
    let next = sync
        .as_mut()
        .map(|state| (state.total_reported(), state.current_page()));
    match next {
        Some((_, Ok(Some(page)))) => {
            let _ = super::try_send_runner_stream_control(
                transport,
                out_tx,
                RunnerEnvelope::ProjectInventoryPage { page },
            );
        }
        Some((_, Ok(None))) => {
            *sync = None;
        }
        Some((projects, Err(reason_code))) => {
            log_project_inventory_degraded(transport.name(), projects, reason_code);
            *sync = None;
        }
        None => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectInventoryStatusAction {
    None,
    IgnoreDelayedAck,
    RetryExactAfter(Duration),
    FreshResnapshot,
}

fn project_inventory_status_requests_fresh_resnapshot(
    status: &ShellProjectInventoryStatus,
) -> bool {
    matches!(
        status.last_error_code.as_deref(),
        Some(
            "project_inventory_stale_generation" | "project_inventory_missing_or_stale_generation"
        )
    )
}

fn project_inventory_status_is_delayed_success_ack(
    state: &ProjectInventorySync,
    status: &ShellProjectInventoryStatus,
) -> bool {
    if status.last_error_code.is_some()
        || !matches!(status.sync_state.as_str(), "in_progress" | "complete")
    {
        return false;
    }
    if status.generation.as_deref() != Some(state.generation()) {
        return true;
    }
    state.pending.as_ref().is_some_and(|pending| {
        status.total_reported == Some(state.total_reported())
            && status.total_synced < pending.next_cursor
    })
}

pub(super) fn handle_project_inventory_status(
    transport: StreamTransport,
    status: ShellProjectInventoryStatus,
    sync: &mut Option<ProjectInventorySync>,
    out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    retry_backoff: &mut RetryBackoff,
) -> ProjectInventoryStatusAction {
    // Stale-generation statuses can retain the last authoritative sync state
    // and generation while carrying the actual recovery reason only in
    // `last_error_code`. Those explicit reasons invalidate the current logical
    // snapshot before ordinary success-ack correlation is attempted.
    if project_inventory_status_requests_fresh_resnapshot(&status) {
        if let Some(state) = sync.as_ref() {
            log_project_inventory_degraded(
                transport.name(),
                state.total_reported(),
                status
                    .last_error_code
                    .as_deref()
                    .unwrap_or("project_inventory_stale_generation"),
            );
        }
        *sync = None;
        retry_backoff.reset();
        return ProjectInventoryStatusAction::FreshResnapshot;
    }

    let Some(state) = sync.as_mut() else {
        retry_backoff.reset();
        return ProjectInventoryStatusAction::None;
    };
    if project_inventory_status_is_delayed_success_ack(state, &status) {
        tracing::debug!(
            transport = transport.name(),
            current_generation = state.generation(),
            ack_generation = status.generation.as_deref(),
            ack_total_synced = status.total_synced,
            "ignoring delayed project inventory success acknowledgement"
        );
        return ProjectInventoryStatusAction::IgnoreDelayedAck;
    }
    let projects = state.total_reported();
    match state.acknowledge(&status) {
        Ok(true) => {
            *sync = None;
            retry_backoff.reset();
            ProjectInventoryStatusAction::None
        }
        Ok(false) => {
            retry_backoff.reset();
            try_queue_project_inventory_page(transport, sync, out_tx);
            ProjectInventoryStatusAction::None
        }
        Err(reason_code) if reason_code == "project_inventory_staging_capacity" => {
            let delay = retry_backoff.next_delay();
            log_project_inventory_degraded(transport.name(), projects, &reason_code);
            eprintln!(
                "webcodex-runner project inventory retry scheduled transport={} reason_code={} delay={}",
                transport.name(),
                reason_code,
                format_delay(delay)
            );
            // Keep the exact pending page/generation/snapshot_sequence. The
            // Server rejected page 0 before advancing its high-water fence, so
            // replaying this exact page after bounded delay is safe.
            ProjectInventoryStatusAction::RetryExactAfter(delay)
        }
        Err(reason_code) => {
            log_project_inventory_degraded(transport.name(), projects, &reason_code);
            *sync = None;
            retry_backoff.reset();
            ProjectInventoryStatusAction::None
        }
    }
}

pub(super) struct StreamingProjectInventoryCoordinator {
    supported: bool,
    pub(super) sync: Option<ProjectInventorySync>,
    project_cache: RunnerProjectCache,
    pub(super) retry_backoff: RetryBackoff,
    pub(super) retry_at: Option<tokio::time::Instant>,
}

impl StreamingProjectInventoryCoordinator {
    pub(super) fn new(sync: Option<ProjectInventorySync>) -> Self {
        Self {
            supported: sync.is_some(),
            sync,
            project_cache: RunnerProjectCache::default(),
            retry_backoff: RetryBackoff::new(&PROJECT_INVENTORY_STAGING_RETRY_BACKOFF_STEPS),
            retry_at: None,
        }
    }

    pub(super) fn retry_at(&self) -> Option<tokio::time::Instant> {
        self.retry_at
    }

    pub(super) fn queue_pending(
        &mut self,
        transport: StreamTransport,
        out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    ) {
        try_queue_project_inventory_page(transport, &mut self.sync, out_tx);
    }

    pub(super) fn retry_pending_now(
        &mut self,
        transport: StreamTransport,
        out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    ) {
        self.retry_at = None;
        self.queue_pending(transport, out_tx);
    }

    pub(super) fn refresh_from_current_projects(
        &mut self,
        transport: StreamTransport,
        cfg: &RunnerConfig,
        runtime: &RunnerRuntimeState,
        out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
        reason_code: &str,
    ) {
        if !self.supported {
            return;
        }
        self.project_cache.invalidate();
        let projects = runtime.project_summaries(&mut self.project_cache, cfg);
        let projects_count = projects.len();
        self.sync = Some(ProjectInventorySync::new(projects));
        self.retry_backoff.reset();
        self.retry_at = None;
        eprintln!(
            "webcodex-runner project inventory resnapshot transport={} projects={} reason_code={}",
            transport.name(),
            projects_count,
            reason_code
        );
        self.queue_pending(transport, out_tx);
    }

    pub(super) fn handle_status(
        &mut self,
        transport: StreamTransport,
        status: ShellProjectInventoryStatus,
        cfg: &RunnerConfig,
        runtime: &RunnerRuntimeState,
        out_tx: &tokio::sync::mpsc::Sender<RunnerEnvelope>,
    ) {
        match handle_project_inventory_status(
            transport,
            status,
            &mut self.sync,
            out_tx,
            &mut self.retry_backoff,
        ) {
            ProjectInventoryStatusAction::None => self.retry_at = None,
            ProjectInventoryStatusAction::IgnoreDelayedAck => {}
            ProjectInventoryStatusAction::RetryExactAfter(delay) => {
                self.retry_at = Some(tokio::time::Instant::now() + delay);
            }
            ProjectInventoryStatusAction::FreshResnapshot => self.refresh_from_current_projects(
                transport,
                cfg,
                runtime,
                out_tx,
                "project_inventory_server_invalidated_snapshot",
            ),
        }
    }
}
