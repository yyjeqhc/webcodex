use super::*;
use crate::shell_protocol::{
    ShellProjectInventoryPage, AGENT_PROTOCOL_VERSION_POLLING_V2,
    PROJECT_INVENTORY_MAX_CONCURRENT_SYNCS, PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
    PROJECT_INVENTORY_PAGE_MAX_SUMMARIES, PROJECT_INVENTORY_STAGING_TTL_SECS,
};

fn synthetic_projects(count: usize) -> Vec<ShellAgentProjectSummary> {
    (0..count)
        .map(|index| {
            project_summary(
                &format!("project-{index:04}"),
                &format!("/tmp/project-{index:04}"),
            )
        })
        .collect()
}

fn paged_registration(client_id: &str, instance_id: &str) -> ShellClientRegisterRequest {
    let mut registration = runner_registration(client_id, instance_id, Vec::new());
    registration.projects = None;
    registration.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_POLLING_V2.to_string());
    registration
}

fn snapshot_pages(
    generation: &str,
    snapshot_sequence: u64,
    projects: &[ShellAgentProjectSummary],
) -> Vec<ShellProjectInventoryPage> {
    if projects.is_empty() {
        return vec![ShellProjectInventoryPage {
            generation: generation.to_string(),
            snapshot_sequence,
            page_index: 0,
            total_reported: 0,
            complete: true,
            projects: Vec::new(),
        }];
    }
    let chunks = projects
        .chunks(PROJECT_INVENTORY_PAGE_MAX_SUMMARIES)
        .collect::<Vec<_>>();
    let last = chunks.len() - 1;
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| ShellProjectInventoryPage {
            generation: generation.to_string(),
            snapshot_sequence,
            page_index: index as u32,
            total_reported: projects.len(),
            complete: index == last,
            projects: chunk.to_vec(),
        })
        .collect()
}

async fn apply_snapshot(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance_id: &str,
    generation: &str,
    snapshot_sequence: u64,
    projects: &[ShellAgentProjectSummary],
) -> crate::shell_protocol::ShellProjectInventoryStatus {
    let mut status = None;
    for page in snapshot_pages(generation, snapshot_sequence, projects) {
        status = Some(
            registry
                .apply_project_inventory_page(client_id, instance_id, page)
                .await
                .expect("inventory page should respect active Runner fence"),
        );
    }
    status.expect("snapshot always contains at least one page")
}

fn assert_resolves_edges(projects: &[ShellAgentProjectSummary], count: usize) {
    for index in [0, count / 2, count - 1] {
        let id = format!("project-{index:04}");
        assert!(
            projects.iter().any(|project| project.id == id),
            "missing expected project {id}"
        );
    }
}

#[tokio::test]
async fn legacy_full_inventory_has_no_project_count_liveness_boundary() {
    let registry = ShellClientRegistry::default();
    for count in [64usize, 65, 100, 256, 1024] {
        let client_id = format!("legacy-scale-{count}");
        let view = registry
            .register(runner_registration(
                &client_id,
                &format!("legacy-instance-{count}"),
                synthetic_projects(count),
            ))
            .await
            .unwrap_or_else(|error| panic!("legacy {count}-project registration failed: {error}"));
        assert!(view.connected, "Runner must remain online at count {count}");
        assert_eq!(view.projects.len(), count);
        assert_eq!(
            view.project_inventory
                .as_ref()
                .map(|status| status.sync_state.as_str()),
            Some("complete")
        );
        assert_resolves_edges(&view.projects, count);
    }
}

#[tokio::test]
async fn v2_registration_project_bootstrap_is_not_published_as_authoritative_inventory() {
    let registry = ShellClientRegistry::default();
    let client_id = "v2-bootstrap";
    let instance_id = "v2-bootstrap-instance";
    let mut registration = runner_registration(
        client_id,
        instance_id,
        vec![
            project_summary("project-0000", "/tmp/project-0000"),
            project_summary("project-0064", "/tmp/project-0064"),
        ],
    );
    registration.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_POLLING_V2.to_string());
    let view = registry.register(registration).await.unwrap();
    assert!(view.connected);
    assert!(
        view.projects.is_empty(),
        "V2 bootstrap summaries validate the register envelope but are not routable inventory"
    );
    assert_eq!(
        view.project_inventory
            .as_ref()
            .map(|status| status.sync_state.as_str()),
        Some("pending")
    );

    let projects = synthetic_projects(65);
    let completed = apply_snapshot(
        &registry,
        client_id,
        instance_id,
        "v2-authoritative",
        1,
        &projects,
    )
    .await;
    assert_eq!(completed.sync_state, "complete");
    assert_eq!(
        registry
            .list_client_projects(client_id)
            .await
            .unwrap()
            .len(),
        65
    );
}

#[tokio::test]
async fn inventory_pages_require_paged_registration_strategy() {
    let registry = ShellClientRegistry::default();
    let client_id = "inventory-strategy-inline";
    let instance_id = "inventory-strategy-inline-instance";
    let original = project_summary("original", "/tmp/original");
    let registered = registry
        .register(runner_registration(
            client_id,
            instance_id,
            vec![original.clone()],
        ))
        .await
        .unwrap();
    assert_eq!(registered.projects.len(), 1);
    assert_eq!(registered.projects[0].id, "original");

    let replacement = vec![project_summary("replacement", "/tmp/replacement")];
    let status = registry
        .apply_project_inventory_page(
            client_id,
            instance_id,
            snapshot_pages("unexpected-page", 1, &replacement).remove(0),
        )
        .await
        .unwrap();
    assert_eq!(
        status.last_error_code.as_deref(),
        Some("project_inventory_paging_not_negotiated")
    );
    let published = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].id, "original");
}

#[tokio::test]
async fn unsupported_protocol_registration_does_not_publish_project_inventory() {
    let registry = ShellClientRegistry::default();
    let client_id = "inventory-strategy-unsupported";
    let instance_id = "inventory-strategy-unsupported-instance";
    let mut registration = runner_registration(
        client_id,
        instance_id,
        vec![project_summary("untrusted", "/tmp/untrusted")],
    );
    registration.agent_protocol_version = Some("future-v2".to_string());

    let error = registry.register(registration).await.unwrap_err();
    assert_eq!(error, "agent_protocol_version is unsupported");
    assert!(registry.get_client_view(client_id).await.is_none());
    assert!(registry.list_client_projects(client_id).await.is_err());
}

#[tokio::test]
async fn paged_inventory_scales_and_reconnects_without_cardinality_admission() {
    for count in [100usize, 256, 1024] {
        let registry = ShellClientRegistry::default();
        let client_id = format!("paged-scale-{count}");
        let instance_id = format!("paged-instance-{count}");
        let initial = registry
            .register(paged_registration(&client_id, &instance_id))
            .await
            .expect("base liveness registration should not depend on project inventory");
        assert!(initial.connected);
        assert!(initial.projects.is_empty());
        assert_eq!(
            initial
                .project_inventory
                .as_ref()
                .map(|status| status.sync_state.as_str()),
            Some("pending")
        );

        let projects = synthetic_projects(count);
        let pages = snapshot_pages("generation-a", 1, &projects);
        assert_eq!(
            pages[0].projects.len(),
            PROJECT_INVENTORY_PAGE_MAX_SUMMARIES
        );
        if count % PROJECT_INVENTORY_PAGE_MAX_SUMMARIES != 0 {
            assert!(
                pages.last().unwrap().projects.len() < PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
                "scale fixture should exercise a short final page"
            );
        }
        let status = apply_snapshot(
            &registry,
            &client_id,
            &instance_id,
            "generation-a",
            1,
            &projects,
        )
        .await;
        assert_eq!(status.sync_state, "complete");
        assert_eq!(status.total_reported, Some(count));
        assert_eq!(status.total_synced, count);
        let published = registry.list_client_projects(&client_id).await.unwrap();
        assert_eq!(published.len(), count);
        assert_resolves_edges(&published, count);

        let reconnect = registry
            .register(paged_registration(&client_id, &instance_id))
            .await
            .expect("same-instance reconnect base registration should stay online");
        assert_eq!(
            reconnect.projects.len(),
            count,
            "reconnect preserves trusted snapshot"
        );
        assert_eq!(
            reconnect
                .project_inventory
                .as_ref()
                .map(|status| status.sync_state.as_str()),
            Some("pending")
        );
        let status = apply_snapshot(
            &registry,
            &client_id,
            &instance_id,
            "generation-b",
            2,
            &projects,
        )
        .await;
        assert_eq!(status.sync_state, "complete");
        assert_eq!(status.total_synced, count);
    }
}

#[tokio::test]
async fn completed_snapshot_atomically_removes_projects_and_stale_pages_cannot_resurrect_them() {
    let registry = ShellClientRegistry::default();
    let client_id = "inventory-removal";
    let instance_id = "inventory-removal-instance";
    registry
        .register(paged_registration(client_id, instance_id))
        .await
        .unwrap();

    let generation_a = synthetic_projects(200);
    apply_snapshot(
        &registry,
        client_id,
        instance_id,
        "generation-a",
        1,
        &generation_a,
    )
    .await;
    let generation_a_pages = snapshot_pages("generation-a", 1, &generation_a);

    let generation_b = generation_a
        .iter()
        .enumerate()
        .filter(|(index, _)| !matches!(index, 37 | 88 | 199))
        .map(|(_, project)| project.clone())
        .collect::<Vec<_>>();
    let generation_b_pages = snapshot_pages("generation-b", 2, &generation_b);
    let first_status = registry
        .apply_project_inventory_page(client_id, instance_id, generation_b_pages[0].clone())
        .await
        .unwrap();
    assert_eq!(first_status.sync_state, "in_progress");
    let during = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(
        during.len(),
        200,
        "incomplete generation must not publish removals"
    );
    for removed in [37usize, 88, 199] {
        assert!(during
            .iter()
            .any(|project| project.id == format!("project-{removed:04}")));
    }

    let duplicate = registry
        .apply_project_inventory_page(client_id, instance_id, generation_b_pages[0].clone())
        .await
        .unwrap();
    assert_eq!(
        duplicate.sync_state, "in_progress",
        "exact duplicate page is idempotent"
    );

    for page in generation_b_pages.iter().skip(1).cloned() {
        registry
            .apply_project_inventory_page(client_id, instance_id, page)
            .await
            .unwrap();
    }
    let completed = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(completed.len(), 197);
    for removed in [37usize, 88, 199] {
        assert!(
            completed
                .iter()
                .all(|project| project.id != format!("project-{removed:04}")),
            "completed replacement must remove project {removed}"
        );
    }
    assert!(completed.iter().any(|project| project.id == "project-0000"));
    assert!(completed.iter().any(|project| project.id == "project-0198"));

    let stale = registry
        .apply_project_inventory_page(client_id, instance_id, generation_a_pages[0].clone())
        .await
        .unwrap();
    assert_eq!(stale.sync_state, "complete");
    assert_eq!(
        stale.last_error_code.as_deref(),
        Some("project_inventory_stale_generation")
    );
    assert_eq!(
        registry
            .list_client_projects(client_id)
            .await
            .unwrap()
            .len(),
        197
    );
}

#[tokio::test]
async fn snapshot_sequence_high_water_rejects_replay_after_retired_generation_cleanup() {
    let registry = ShellClientRegistry::default();
    let client_id = "inventory-sequence-high-water";
    let instance_id = "inventory-sequence-instance";
    registry
        .register(paged_registration(client_id, instance_id))
        .await
        .unwrap();

    let oldest = vec![project_summary("current", "/tmp/generation-1")];
    apply_snapshot(
        &registry,
        client_id,
        instance_id,
        "generation-1",
        1,
        &oldest,
    )
    .await;
    let oldest_page = snapshot_pages("generation-1", 1, &oldest)[0].clone();

    for sequence in 2..=24u64 {
        let projects = vec![project_summary(
            "current",
            &format!("/tmp/generation-{sequence}"),
        )];
        apply_snapshot(
            &registry,
            client_id,
            instance_id,
            &format!("generation-{sequence}"),
            sequence,
            &projects,
        )
        .await;
    }
    {
        let inner = registry.inner.lock().await;
        let state = &inner.clients.get(client_id).unwrap().project_inventory;
        assert_eq!(state.highest_snapshot_sequence, 24);
        assert!(
            !state
                .retired_generations
                .iter()
                .any(|generation| generation == "generation-1"),
            "fixture must prove stale rejection no longer depends on the bounded retired ring"
        );
    }

    let replay = registry
        .apply_project_inventory_page(client_id, instance_id, oldest_page)
        .await
        .unwrap();
    assert_eq!(replay.sync_state, "complete");
    assert_eq!(
        replay.last_error_code.as_deref(),
        Some("project_inventory_stale_generation")
    );
    let projects = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].path, "/tmp/generation-24");
}

#[tokio::test]
async fn dynamic_register_unregister_retire_prior_generation_and_converge_with_full_snapshot() {
    let registry = ShellClientRegistry::default();
    let client_id = "inventory-dynamic-convergence";
    let instance_id = "inventory-dynamic-instance";
    registry
        .register(paged_registration(client_id, instance_id))
        .await
        .unwrap();

    // Generation A is deliberately incomplete: only page 0 is staged when a
    // dynamic mutation becomes immediately authoritative.
    let generation_a = synthetic_projects(70);
    let stale_a_pages = snapshot_pages("generation-a", 1, &generation_a);
    let staged_a = registry
        .apply_project_inventory_page(client_id, instance_id, stale_a_pages[0].clone())
        .await
        .unwrap();
    assert_eq!(staged_a.sync_state, "in_progress");
    assert!(
        registry
            .list_client_projects(client_id)
            .await
            .unwrap()
            .is_empty(),
        "incomplete generation A must not partially publish"
    );

    registry
        .upsert_client_project_for_instance(
            client_id,
            instance_id,
            project_summary("dynamic-added", "/tmp/dynamic-added"),
        )
        .await
        .expect("dynamic register projection should commit against the active instance");
    let after_register = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(after_register.len(), 1);
    assert_eq!(after_register[0].id, "dynamic-added");

    // Exact retransmission of the last accepted A/page0 must not hit the
    // duplicate fast path after dynamic projection has retired A. The status
    // must explicitly fence the old generation so a streaming Runner can
    // resnapshot instead of waiting forever on an unrelated current status.
    let duplicate_a0_after_register = registry
        .apply_project_inventory_page(client_id, instance_id, stale_a_pages[0].clone())
        .await
        .unwrap();
    assert_eq!(
        duplicate_a0_after_register.last_error_code.as_deref(),
        Some("project_inventory_stale_generation")
    );

    // Dynamic projection clears A staging and retires the pre-mutation
    // generation. A later page from A must be rejected rather than replacing
    // the just-committed mutation with its stale 70-project snapshot.
    let stale_after_register = registry
        .apply_project_inventory_page(client_id, instance_id, stale_a_pages[1].clone())
        .await
        .unwrap();
    assert_eq!(stale_after_register.sync_state, "complete");
    assert_eq!(
        stale_after_register.last_error_code.as_deref(),
        Some("project_inventory_stale_generation")
    );
    let still_dynamic_only = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(still_dynamic_only.len(), 1);
    assert_eq!(still_dynamic_only[0].id, "dynamic-added");

    // A fresh post-mutation full snapshot restores every pre-existing project
    // plus the dynamically added project atomically.
    let mut generation_b = generation_a.clone();
    generation_b.push(project_summary("dynamic-added", "/tmp/dynamic-added"));
    let converged_after_register = apply_snapshot(
        &registry,
        client_id,
        instance_id,
        "generation-b",
        2,
        &generation_b,
    )
    .await;
    assert_eq!(converged_after_register.sync_state, "complete");
    assert_eq!(converged_after_register.total_synced, 71);
    let after_full_register = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(after_full_register.len(), 71);
    assert!(after_full_register
        .iter()
        .any(|project| project.id == "project-0037"));
    assert!(after_full_register
        .iter()
        .any(|project| project.id == "dynamic-added"));

    assert!(registry
        .remove_client_project_for_instance(client_id, instance_id, "project-0037")
        .await
        .expect("dynamic unregister projection should commit against the active instance"));
    let after_unregister = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(after_unregister.len(), 70);
    assert!(after_unregister
        .iter()
        .all(|project| project.id != "project-0037"));
    assert!(after_unregister
        .iter()
        .any(|project| project.id == "dynamic-added"));

    let stale_b_pages = snapshot_pages("generation-b", 2, &generation_b);
    let stale_after_unregister = registry
        .apply_project_inventory_page(client_id, instance_id, stale_b_pages[0].clone())
        .await
        .unwrap();
    assert_eq!(
        stale_after_unregister.last_error_code.as_deref(),
        Some("project_inventory_stale_generation")
    );

    let converged = apply_snapshot(
        &registry,
        client_id,
        instance_id,
        "generation-c",
        3,
        &after_unregister,
    )
    .await;
    assert_eq!(converged.sync_state, "complete");
    assert_eq!(converged.total_synced, 70);
    let final_projects = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(final_projects.len(), 70);
    assert!(final_projects
        .iter()
        .all(|project| project.id != "project-0037"));
    assert!(final_projects
        .iter()
        .any(|project| project.id == "dynamic-added"));

    registry
        .apply_project_inventory_page(client_id, instance_id, stale_a_pages[1].clone())
        .await
        .unwrap();
    let after_stale = registry.list_client_projects(client_id).await.unwrap();
    assert_eq!(after_stale.len(), 70);
    assert!(after_stale
        .iter()
        .all(|project| project.id != "project-0037"));
    assert!(after_stale
        .iter()
        .any(|project| project.id == "dynamic-added"));
}

#[tokio::test]
async fn missing_out_of_order_and_restart_midway_keep_atomicity_without_cross_instance_routing() {
    let registry = ShellClientRegistry::default();
    let client_id = "inventory-restart";
    let old_instance = "inventory-old-instance";
    registry
        .register(paged_registration(client_id, old_instance))
        .await
        .unwrap();
    let projects = synthetic_projects(160);
    apply_snapshot(&registry, client_id, old_instance, "stable", 1, &projects).await;

    let missing_pages = snapshot_pages("missing", 2, &projects);
    registry
        .apply_project_inventory_page(client_id, old_instance, missing_pages[0].clone())
        .await
        .unwrap();
    let degraded = registry
        .apply_project_inventory_page(client_id, old_instance, missing_pages[2].clone())
        .await
        .unwrap();
    assert_eq!(degraded.sync_state, "degraded");
    assert_eq!(
        degraded.last_error_code.as_deref(),
        Some("project_inventory_page_out_of_order")
    );
    assert_eq!(
        registry
            .list_client_projects(client_id)
            .await
            .unwrap()
            .len(),
        160
    );

    let out_of_order_pages = snapshot_pages("out-of-order", 3, &projects);
    let rejected = registry
        .apply_project_inventory_page(client_id, old_instance, out_of_order_pages[1].clone())
        .await
        .unwrap();
    assert_eq!(
        registry
            .list_client_projects(client_id)
            .await
            .unwrap()
            .len(),
        160
    );
    assert_eq!(
        rejected.last_error_code.as_deref(),
        Some("project_inventory_missing_or_stale_generation")
    );

    let midway_pages = snapshot_pages("midway", 4, &projects);
    registry
        .apply_project_inventory_page(client_id, old_instance, midway_pages[0].clone())
        .await
        .unwrap();
    registry.reconcile_disconnect(client_id, old_instance).await;
    let new_instance = "inventory-new-instance";
    let restarted = registry
        .register(paged_registration(client_id, new_instance))
        .await
        .expect("new Runner instance should register after the old lease disconnects");
    assert!(restarted.connected);
    assert!(
        restarted.projects.is_empty(),
        "a new Runner instance must not inherit the previous process's routable project authority"
    );
    let restarted_inventory = restarted.project_inventory.as_ref().unwrap();
    assert_eq!(restarted_inventory.sync_state, "pending");
    assert_eq!(restarted_inventory.total_synced, 0);

    let stale_instance = registry
        .apply_project_inventory_page(client_id, old_instance, midway_pages[1].clone())
        .await
        .unwrap_err();
    assert!(stale_instance.contains("stale or replaced"));
    assert!(
        registry
            .list_client_projects(client_id)
            .await
            .unwrap()
            .is_empty(),
        "stale old-instance pages must not restore routable authority while the replacement is pending"
    );

    let replacement = synthetic_projects(161);
    let status = apply_snapshot(
        &registry,
        client_id,
        new_instance,
        "after-restart",
        1,
        &replacement,
    )
    .await;
    assert_eq!(status.sync_state, "complete");
    assert_eq!(status.total_synced, 161);
    assert_resolves_edges(
        &registry.list_client_projects(client_id).await.unwrap(),
        161,
    );
}

#[tokio::test]
async fn malformed_and_oversized_inventory_degrades_without_revoking_runner_liveness() {
    let registry = ShellClientRegistry::default();
    let client_id = "inventory-bounds";
    let instance_id = "inventory-bounds-instance";
    let trusted = synthetic_projects(3);
    registry
        .register(paged_registration(client_id, instance_id))
        .await
        .unwrap();
    let trusted_status =
        apply_snapshot(&registry, client_id, instance_id, "trusted", 1, &trusted).await;
    assert_eq!(trusted_status.sync_state, "complete");

    let malformed = ShellProjectInventoryPage {
        generation: "bad/generation".to_string(),
        snapshot_sequence: 1,
        page_index: 0,
        total_reported: 1,
        complete: true,
        projects: vec![project_summary("new", "/tmp/new")],
    };
    let malformed_status = registry
        .apply_project_inventory_page(client_id, instance_id, malformed)
        .await
        .unwrap();
    assert_eq!(malformed_status.sync_state, "degraded");
    assert_eq!(
        malformed_status.last_error_code.as_deref(),
        Some("project_inventory_invalid_generation")
    );

    let invalid_sequence_status = registry
        .apply_project_inventory_page(
            client_id,
            instance_id,
            ShellProjectInventoryPage {
                generation: "valid-generation".to_string(),
                snapshot_sequence: 0,
                page_index: 0,
                total_reported: 1,
                complete: true,
                projects: vec![project_summary("new", "/tmp/new")],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        invalid_sequence_status.last_error_code.as_deref(),
        Some("project_inventory_invalid_snapshot_sequence")
    );

    let mut invalid_summary = project_summary("invalid-description", "/tmp/invalid");
    invalid_summary.description = Some("d".repeat(501));
    let invalid_status = registry
        .apply_project_inventory_page(
            client_id,
            instance_id,
            ShellProjectInventoryPage {
                generation: "invalid-summary".to_string(),
                snapshot_sequence: 1,
                page_index: 0,
                total_reported: 1,
                complete: true,
                projects: vec![invalid_summary],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        invalid_status.last_error_code.as_deref(),
        Some("project_summary_invalid_description")
    );

    let max_path = format!("/{}", "x".repeat(4095));
    let oversized_projects = (0..PROJECT_INVENTORY_PAGE_MAX_SUMMARIES)
        .map(|index| project_summary(&format!("large-{index}"), &max_path))
        .collect::<Vec<_>>();
    let oversized_page = ShellProjectInventoryPage {
        generation: "oversized-page".to_string(),
        snapshot_sequence: 1,
        page_index: 0,
        total_reported: oversized_projects.len(),
        complete: true,
        projects: oversized_projects,
    };
    assert!(
        serde_json::to_vec(&oversized_page).unwrap().len()
            > PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES
    );
    let oversized_status = registry
        .apply_project_inventory_page(client_id, instance_id, oversized_page)
        .await
        .unwrap();
    assert_eq!(
        oversized_status.last_error_code.as_deref(),
        Some("project_inventory_page_too_large")
    );
    assert_eq!(
        registry
            .list_client_projects(client_id)
            .await
            .unwrap()
            .len(),
        trusted.len()
    );
    assert!(registry.get_client_view(client_id).await.unwrap().connected);
}

#[tokio::test]
async fn staging_capacity_and_timeout_are_bounded_without_taking_runners_offline() {
    let registry = ShellClientRegistry::default();
    for index in 0..=PROJECT_INVENTORY_MAX_CONCURRENT_SYNCS {
        let client_id = format!("staging-{index}");
        let instance_id = format!("staging-instance-{index}");
        registry
            .register(paged_registration(&client_id, &instance_id))
            .await
            .unwrap();
        let status = registry
            .apply_project_inventory_page(
                &client_id,
                &instance_id,
                ShellProjectInventoryPage {
                    generation: format!("generation-{index}"),
                    snapshot_sequence: 1,
                    page_index: 0,
                    total_reported: 2,
                    complete: false,
                    projects: vec![project_summary("first", "/tmp/first")],
                },
            )
            .await
            .unwrap();
        if index < PROJECT_INVENTORY_MAX_CONCURRENT_SYNCS {
            assert_eq!(status.sync_state, "in_progress");
        } else {
            assert_eq!(status.sync_state, "degraded");
            assert_eq!(
                status.last_error_code.as_deref(),
                Some("project_inventory_staging_capacity")
            );
            assert!(
                registry
                    .get_client_view(&client_id)
                    .await
                    .unwrap()
                    .connected
            );
        }
    }

    let client_id = "staging-0";
    {
        let mut inner = registry.inner.lock().await;
        let staging = inner
            .clients
            .get_mut(client_id)
            .and_then(|client| client.project_inventory.staging.as_mut())
            .expect("first client has staging");
        staging.started_at = now_ts() - PROJECT_INVENTORY_STAGING_TTL_SECS - 1;
    }

    // Capacity rejection happened before the denied page advanced the Server's
    // sequence fence. Once one staging slot expires, retry the exact same page
    // 0/generation/sequence and prove the snapshot can converge atomically.
    let denied_index = PROJECT_INVENTORY_MAX_CONCURRENT_SYNCS;
    let denied_client = format!("staging-{denied_index}");
    let denied_instance = format!("staging-instance-{denied_index}");
    let denied_page0 = ShellProjectInventoryPage {
        generation: format!("generation-{denied_index}"),
        snapshot_sequence: 1,
        page_index: 0,
        total_reported: 2,
        complete: false,
        projects: vec![project_summary("first", "/tmp/first")],
    };
    let retry = registry
        .apply_project_inventory_page(&denied_client, &denied_instance, denied_page0.clone())
        .await
        .unwrap();
    assert_eq!(retry.sync_state, "in_progress");
    assert_eq!(
        retry.generation.as_deref(),
        Some(denied_page0.generation.as_str())
    );
    assert_eq!(retry.total_synced, 1);
    let completed = registry
        .apply_project_inventory_page(
            &denied_client,
            &denied_instance,
            ShellProjectInventoryPage {
                generation: denied_page0.generation.clone(),
                snapshot_sequence: denied_page0.snapshot_sequence,
                page_index: 1,
                total_reported: 2,
                complete: true,
                projects: vec![project_summary("second", "/tmp/second")],
            },
        )
        .await
        .unwrap();
    assert_eq!(completed.sync_state, "complete");
    let published = registry.list_client_projects(&denied_client).await.unwrap();
    assert_eq!(published.len(), 2);
    assert!(published.iter().any(|project| project.id == "first"));
    assert!(published.iter().any(|project| project.id == "second"));

    registry
        .register(paged_registration(
            "staging-reclaimed",
            "staging-reclaimed-instance",
        ))
        .await
        .unwrap();
    let reclaimed = registry
        .apply_project_inventory_page(
            "staging-reclaimed",
            "staging-reclaimed-instance",
            ShellProjectInventoryPage {
                generation: "reclaimed-generation".to_string(),
                snapshot_sequence: 1,
                page_index: 0,
                total_reported: 2,
                complete: false,
                projects: vec![project_summary("first", "/tmp/reclaimed-first")],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        reclaimed.sync_state, "in_progress",
        "capacity admission must reclaim expired staging before rejecting a new sync"
    );

    let expired = registry.get_client_view(client_id).await.unwrap();
    let status = expired.project_inventory.unwrap();
    assert_eq!(status.sync_state, "degraded");
    assert_eq!(
        status.last_error_code.as_deref(),
        Some("project_inventory_sync_timeout")
    );
    assert!(expired.connected);
}
