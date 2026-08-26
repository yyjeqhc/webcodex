use super::state::{ProjectInventoryStaging, ProjectInventoryState, ShellClientRecord};
use super::validation::{
    normalize_project_summaries, sha256_hex, validate_agent_instance_id,
    validate_project_summary_batch,
};
use super::{now_ts, ShellClientRegistry};
use crate::shell_protocol::{
    AgentProjectInventoryStrategy, ShellAgentProjectSummary, ShellProjectInventoryPage,
    ShellProjectInventoryStatus, PROJECT_INVENTORY_GENERATION_MAX_BYTES,
    PROJECT_INVENTORY_MAX_CONCURRENT_SYNCS, PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
    PROJECT_INVENTORY_PAGE_MAX_SUMMARIES, PROJECT_INVENTORY_SNAPSHOT_MAX_SERIALIZED_BYTES,
    PROJECT_INVENTORY_STAGING_TTL_SECS,
};
use std::collections::{HashSet, VecDeque};

const MAX_RETIRED_PROJECT_GENERATIONS: usize = 16;

fn status(
    sync_state: &str,
    generation: Option<String>,
    total_reported: Option<usize>,
    total_synced: usize,
    last_error_code: Option<String>,
    last_sync_at: Option<i64>,
) -> ShellProjectInventoryStatus {
    ShellProjectInventoryStatus {
        sync_state: sync_state.to_string(),
        generation,
        total_reported,
        total_synced,
        last_error_code,
        last_sync_at,
        max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
        max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
    }
}

pub(super) fn pending_inventory_state(total_synced: usize) -> ProjectInventoryState {
    ProjectInventoryState {
        status: ShellProjectInventoryStatus::pending(total_synced),
        staging: None,
        retired_generations: VecDeque::new(),
        highest_snapshot_sequence: 0,
        last_page_generation: None,
        last_page_index: None,
        last_page_digest: None,
    }
}

pub(super) fn complete_legacy_inventory_state(
    total_synced: usize,
    now: i64,
) -> ProjectInventoryState {
    ProjectInventoryState {
        status: status(
            "complete",
            Some("legacy-inline".to_string()),
            Some(total_synced),
            total_synced,
            None,
            Some(now),
        ),
        ..pending_inventory_state(total_synced)
    }
}

pub(super) fn degraded_inventory_state(
    total_synced: usize,
    error_code: &str,
    now: i64,
) -> ProjectInventoryState {
    ProjectInventoryState {
        status: status(
            "degraded",
            None,
            None,
            total_synced,
            Some(error_code.to_string()),
            Some(now),
        ),
        ..pending_inventory_state(total_synced)
    }
}

fn retire_generation(state: &mut ProjectInventoryState, generation: String) {
    if generation.is_empty()
        || state
            .retired_generations
            .iter()
            .any(|existing| existing == &generation)
    {
        return;
    }
    state.retired_generations.push_back(generation);
    while state.retired_generations.len() > MAX_RETIRED_PROJECT_GENERATIONS {
        state.retired_generations.pop_front();
    }
}

fn clear_staging(client: &mut ShellClientRecord, error_code: Option<&str>, now: i64) {
    if let Some(staging) = client.project_inventory.staging.take() {
        retire_generation(&mut client.project_inventory, staging.generation);
    }
    if let Some(error_code) = error_code {
        client.project_inventory.status = status(
            "degraded",
            client.project_inventory.status.generation.clone(),
            client.project_inventory.status.total_reported,
            client.projects.len(),
            Some(error_code.to_string()),
            client.project_inventory.status.last_sync_at.or(Some(now)),
        );
    }
}

pub(super) fn preserve_authoritative_with_error(
    existing: &ProjectInventoryState,
    total_synced: usize,
    error_code: &str,
    now: i64,
) -> ProjectInventoryState {
    let mut state = existing.clone();
    if let Some(staging) = state.staging.take() {
        retire_generation(&mut state, staging.generation);
    }
    state.status = status(
        "degraded",
        state.status.generation.clone(),
        state.status.total_reported,
        total_synced,
        Some(error_code.to_string()),
        state.status.last_sync_at.or(Some(now)),
    );
    state
}

pub(super) fn preserve_authoritative_pending(
    existing: &ProjectInventoryState,
    total_synced: usize,
) -> ProjectInventoryState {
    let mut state = existing.clone();
    if let Some(staging) = state.staging.take() {
        retire_generation(&mut state, staging.generation);
    }
    state.status = status(
        "pending",
        state.status.generation.clone(),
        None,
        total_synced,
        None,
        state.status.last_sync_at,
    );
    state
}

pub(super) fn prepare_legacy_inventory(
    projects: Option<Vec<ShellAgentProjectSummary>>,
    now: i64,
) -> (
    bool,
    Vec<ShellAgentProjectSummary>,
    ProjectInventoryState,
    Option<&'static str>,
) {
    let Some(projects) = projects else {
        return (false, Vec::new(), pending_inventory_state(0), None);
    };
    match validate_project_summary_batch(&projects) {
        Ok(_) => {
            let projects = normalize_project_summaries(Some(projects));
            let state = complete_legacy_inventory_state(projects.len(), now);
            (true, projects, state, None)
        }
        Err(code) => (
            true,
            Vec::new(),
            degraded_inventory_state(0, code, now),
            Some(code),
        ),
    }
}

pub(super) fn apply_legacy_refresh(
    client: &mut ShellClientRecord,
    projects: Vec<ShellAgentProjectSummary>,
    now: i64,
) {
    match validate_project_summary_batch(&projects) {
        Ok(_) => {
            clear_staging(client, None, now);
            if let Some(previous) = client.project_inventory.status.generation.clone() {
                if previous != "legacy-inline" {
                    retire_generation(&mut client.project_inventory, previous);
                }
            }
            client.projects = normalize_project_summaries(Some(projects));
            client.project_inventory.status = status(
                "complete",
                Some("legacy-inline".to_string()),
                Some(client.projects.len()),
                client.projects.len(),
                None,
                Some(now),
            );
        }
        Err(code) => {
            clear_staging(client, Some(code), now);
        }
    }
}

pub(super) fn reconcile_dynamic_projection(client: &mut ShellClientRecord, now: i64) {
    clear_staging(client, None, now);
    if let Some(previous) = client.project_inventory.status.generation.clone() {
        if previous != "legacy-inline" {
            retire_generation(&mut client.project_inventory, previous);
        }
    }
    // Dynamic mutation is authoritative but is not itself a full snapshot
    // generation. Retiring the prior paged generation prevents a delayed page
    // from undoing a just-committed register/unregister projection.
    client.project_inventory.status = status(
        "complete",
        None,
        Some(client.projects.len()),
        client.projects.len(),
        None,
        Some(now),
    );
}

pub(super) fn expire_staging(client: &mut ShellClientRecord, now: i64) {
    let expired = client
        .project_inventory
        .staging
        .as_ref()
        .is_some_and(|staging| {
            now.saturating_sub(staging.started_at) > PROJECT_INVENTORY_STAGING_TTL_SECS
        });
    if expired {
        clear_staging(client, Some("project_inventory_sync_timeout"), now);
    }
}

fn validate_generation(generation: &str) -> Result<(), &'static str> {
    if generation.is_empty()
        || generation.len() > PROJECT_INVENTORY_GENERATION_MAX_BYTES
        || !generation.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err("project_inventory_invalid_generation");
    }
    Ok(())
}

fn validate_page(page: &ShellProjectInventoryPage) -> Result<(usize, String), &'static str> {
    validate_generation(&page.generation)?;
    if page.snapshot_sequence == 0 {
        return Err("project_inventory_invalid_snapshot_sequence");
    }
    if page.projects.len() > PROJECT_INVENTORY_PAGE_MAX_SUMMARIES {
        return Err("project_inventory_page_summary_limit");
    }
    if page.total_reported == 0 {
        if page.page_index != 0 || !page.complete || !page.projects.is_empty() {
            return Err("project_inventory_empty_snapshot_malformed");
        }
    } else if page.projects.is_empty() {
        return Err("project_inventory_empty_page");
    }
    validate_project_summary_batch(&page.projects)?;
    let bytes = serde_json::to_vec(page).map_err(|_| "project_inventory_serialization_failed")?;
    if bytes.len() > PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES {
        return Err("project_inventory_page_too_large");
    }
    let digest = sha256_hex(
        std::str::from_utf8(&bytes).map_err(|_| "project_inventory_serialization_failed")?,
    );
    Ok((bytes.len(), digest))
}

fn note_nonfatal_error(client: &mut ShellClientRecord, code: &str) {
    client.project_inventory.status.last_error_code = Some(code.to_string());
}

fn fail_current_staging(client: &mut ShellClientRecord, code: &str, now: i64) {
    clear_staging(client, Some(code), now);
}

impl ShellClientRegistry {
    pub(crate) async fn apply_project_inventory_page(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        page: ShellProjectInventoryPage,
    ) -> Result<ShellProjectInventoryStatus, String> {
        self.apply_project_inventory_page_checked(client_id, agent_instance_id, None, page)
            .await
    }

    pub(crate) async fn apply_project_inventory_page_for_connection(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        connection_id: &str,
        page: ShellProjectInventoryPage,
    ) -> Result<ShellProjectInventoryStatus, String> {
        self.apply_project_inventory_page_checked(
            client_id,
            agent_instance_id,
            Some(connection_id),
            page,
        )
        .await
    }

    async fn apply_project_inventory_page_checked(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        expected_connection_id: Option<&str>,
        page: ShellProjectInventoryPage,
    ) -> Result<ShellProjectInventoryStatus, String> {
        validate_agent_instance_id(agent_instance_id)?;
        let now = now_ts();
        let mut inner = self.inner.lock().await;
        // Capacity admission must not count abandoned expired snapshots. Cleanup
        // all bounded staging records under the same registry lock before the
        // concurrent-work check; no background timer or unbounded task list is
        // required.
        for existing in inner.clients.values_mut() {
            expire_staging(existing, now);
        }
        let concurrent_staging = inner
            .clients
            .values()
            .filter(|client| client.project_inventory.staging.is_some())
            .count();
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {client_id}"));
        };
        if client.agent_instance_id != agent_instance_id {
            return Err(format!(
                "agent client {client_id} is no longer the active instance (stale or replaced)"
            ));
        }
        if expected_connection_id
            .is_some_and(|expected| client.connection_id.as_deref() != Some(expected))
        {
            return Err(format!(
                "agent client {client_id} transport connection is no longer active"
            ));
        }
        // Inventory failure is deliberately subordinate to Runner liveness.
        client.last_seen = now;
        expire_staging(client, now);
        if !matches!(
            client.accepted_protocol.project_inventory(),
            AgentProjectInventoryStrategy::Paged
        ) {
            note_nonfatal_error(client, "project_inventory_paging_not_negotiated");
            return Ok(client.project_inventory.status.clone());
        }

        let (page_bytes, page_digest) = match validate_page(&page) {
            Ok(validated) => validated,
            Err(code) => {
                fail_current_staging(client, code, now);
                return Ok(client.project_inventory.status.clone());
            }
        };

        let staging_matches_page =
            client
                .project_inventory
                .staging
                .as_ref()
                .is_some_and(|staging| {
                    staging.generation == page.generation
                        && staging.snapshot_sequence == page.snapshot_sequence
                });
        let completed_generation_matches_page = client.project_inventory.staging.is_none()
            && client.project_inventory.status.sync_state == "complete"
            && client.project_inventory.status.generation.as_deref() == Some(&page.generation);
        let exact_last_page_replay = client.project_inventory.last_page_generation.as_deref()
            == Some(&page.generation)
            && client.project_inventory.last_page_index == Some(page.page_index)
            && client.project_inventory.last_page_digest.as_deref() == Some(&page_digest);
        // Exact replay is idempotent only while the page still belongs to the
        // active staging generation or the current completed authoritative
        // generation. Dynamic mutation, timeout, or a newer snapshot can retire
        // the same generation after its last page was recorded; in that case the
        // replay must continue through the stale-generation fences below.
        if exact_last_page_replay && (staging_matches_page || completed_generation_matches_page) {
            return Ok(client.project_inventory.status.clone());
        }
        if page.snapshot_sequence < client.project_inventory.highest_snapshot_sequence
            || (page.snapshot_sequence == client.project_inventory.highest_snapshot_sequence
                && !staging_matches_page)
        {
            note_nonfatal_error(client, "project_inventory_stale_generation");
            return Ok(client.project_inventory.status.clone());
        }
        if client
            .project_inventory
            .retired_generations
            .iter()
            .any(|generation| generation == &page.generation)
            || (client.project_inventory.staging.is_none()
                && client.project_inventory.status.sync_state == "complete"
                && client.project_inventory.status.generation.as_deref() == Some(&page.generation))
        {
            note_nonfatal_error(client, "project_inventory_stale_generation");
            return Ok(client.project_inventory.status.clone());
        }

        if !staging_matches_page {
            if page.page_index != 0 {
                note_nonfatal_error(client, "project_inventory_missing_or_stale_generation");
                return Ok(client.project_inventory.status.clone());
            }
            if client.project_inventory.staging.is_none()
                && concurrent_staging >= PROJECT_INVENTORY_MAX_CONCURRENT_SYNCS
            {
                client.project_inventory.status = status(
                    "degraded",
                    client.project_inventory.status.generation.clone(),
                    client.project_inventory.status.total_reported,
                    client.projects.len(),
                    Some("project_inventory_staging_capacity".to_string()),
                    client.project_inventory.status.last_sync_at,
                );
                return Ok(client.project_inventory.status.clone());
            }
            if page.snapshot_sequence <= client.project_inventory.highest_snapshot_sequence {
                note_nonfatal_error(client, "project_inventory_stale_generation");
                return Ok(client.project_inventory.status.clone());
            }
            client.project_inventory.highest_snapshot_sequence = page.snapshot_sequence;
            // As soon as a fresh page-0 generation is accepted, permanently
            // retire the generation that produced the still-authoritative
            // snapshot. Its projects stay published until this new generation
            // completes, but delayed old pages can no longer restart a sync and
            // resurrect removed entries.
            if client.project_inventory.status.sync_state == "complete" {
                if let Some(previous) = client.project_inventory.status.generation.clone() {
                    if previous != "legacy-inline" && previous != page.generation {
                        retire_generation(&mut client.project_inventory, previous);
                    }
                }
            }
            if let Some(staging) = client.project_inventory.staging.take() {
                retire_generation(&mut client.project_inventory, staging.generation);
            }
            client.project_inventory.staging = Some(ProjectInventoryStaging {
                generation: page.generation.clone(),
                snapshot_sequence: page.snapshot_sequence,
                total_reported: page.total_reported,
                next_page_index: 0,
                projects: Vec::new(),
                seen_ids: HashSet::new(),
                serialized_bytes: 0,
                started_at: now,
            });
        }

        let Some(staging) = client.project_inventory.staging.as_mut() else {
            unreachable!("staging initialized above");
        };
        if staging.generation != page.generation
            || staging.snapshot_sequence != page.snapshot_sequence
        {
            note_nonfatal_error(client, "project_inventory_stale_generation");
            return Ok(client.project_inventory.status.clone());
        }
        if staging.next_page_index != page.page_index {
            fail_current_staging(client, "project_inventory_page_out_of_order", now);
            return Ok(client.project_inventory.status.clone());
        }
        if staging.total_reported != page.total_reported {
            fail_current_staging(client, "project_inventory_total_changed", now);
            return Ok(client.project_inventory.status.clone());
        }
        if page
            .projects
            .iter()
            .any(|project| staging.seen_ids.contains(&project.id))
        {
            fail_current_staging(client, "project_inventory_duplicate_project_id", now);
            return Ok(client.project_inventory.status.clone());
        }
        let next_total = staging.projects.len().saturating_add(page.projects.len());
        if next_total > staging.total_reported
            || (page.complete && next_total != staging.total_reported)
            || (!page.complete && next_total >= staging.total_reported)
        {
            fail_current_staging(client, "project_inventory_completion_mismatch", now);
            return Ok(client.project_inventory.status.clone());
        }
        let next_bytes = staging.serialized_bytes.saturating_add(page_bytes);
        if next_bytes > PROJECT_INVENTORY_SNAPSHOT_MAX_SERIALIZED_BYTES {
            fail_current_staging(client, "project_inventory_snapshot_too_large", now);
            return Ok(client.project_inventory.status.clone());
        }

        for project in &page.projects {
            staging.seen_ids.insert(project.id.clone());
        }
        staging.projects.extend(page.projects.iter().cloned());
        staging.serialized_bytes = next_bytes;
        staging.next_page_index = staging.next_page_index.saturating_add(1);
        client.project_inventory.last_page_generation = Some(page.generation.clone());
        client.project_inventory.last_page_index = Some(page.page_index);
        client.project_inventory.last_page_digest = Some(page_digest);

        if page.complete {
            let mut completed = client
                .project_inventory
                .staging
                .take()
                .expect("completed page has staging");
            completed
                .projects
                .sort_by(|left, right| left.id.cmp(&right.id));
            client.projects = completed.projects;
            client.project_inventory.status = status(
                "complete",
                Some(page.generation),
                Some(client.projects.len()),
                client.projects.len(),
                None,
                Some(now),
            );
        } else {
            client.project_inventory.status = status(
                "in_progress",
                Some(page.generation),
                Some(page.total_reported),
                staging.projects.len(),
                None,
                client.project_inventory.status.last_sync_at,
            );
        }
        Ok(client.project_inventory.status.clone())
    }

    #[cfg(test)]
    pub(crate) async fn project_inventory_status_for_test(
        &self,
        client_id: &str,
    ) -> Option<ShellProjectInventoryStatus> {
        let mut inner = self.inner.lock().await;
        let client = inner.clients.get_mut(client_id)?;
        expire_staging(client, now_ts());
        Some(client.project_inventory.status.clone())
    }
}
