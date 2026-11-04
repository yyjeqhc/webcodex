//! Opt-in, deterministic-shape Server benchmark. No live Server or Runner I/O.
//! Run with: cargo test -p webcodex --lib project_selector_load_profile -- --ignored --nocapture
use super::*;
use std::time::Instant;

fn percentile_ms(samples: &mut [f64], percentile: usize) -> f64 {
    samples.sort_by(f64::total_cmp);
    let index = (samples.len() * percentile).div_ceil(100).saturating_sub(1);
    samples[index.min(samples.len() - 1)]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "controlled selector latency profile; no CI timing threshold"]
async fn project_selector_load_profile() {
    for count in [500, 2_000, 5_000] {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::Database::open(&temp.path().join("selector.db")).unwrap());
        let runtime = Arc::new(
            ToolRuntime::new(
                Arc::new(crate::RunnerRegistry::default()),
                Arc::new(RuntimeInfo::default()),
            )
            .with_project_reference_database(db),
        );
        let auth = crate::auth::shared_key_context("selector-profile");
        // Cached polling Runner inventories; no fixture serves Runner RPCs.
        // The query must complete entirely from Server-owned observations.
        for runner_index in 0..5 {
            let client = format!("profile-{runner_index}");
            register_project(&runtime, &client, "seed", "/fixture/seed", Some(&auth)).await;
            let project_rows = (0..count / 5)
                .map(|index| {
                    let mut p = project(
                        &format!("p{index:05}"),
                        &format!("/fixture/{client}/p{index:05}"),
                    );
                    p.root_fingerprint = Some(format!("wc_projroot_{index:064x}"));
                    p.description = Some("fixture project metadata ".repeat(16));
                    p.hooks = vec!["fixture hook metadata ".repeat(4)];
                    p
                })
                .collect();
            crate::test_support::apply_project_inventory_snapshot(
                &runtime.runner_registry,
                &client,
                &format!("inst-{client}"),
                project_rows,
            )
            .await;
        }
        for limit in [24, 2_000] {
            // Limit 24 runs first against this database, so limit 2000 has
            // those first refs cached. This is a first pass, not a cold DB.
            let first_pass_started = Instant::now();
            let first_page = projects_for_filters_auth(&runtime, &auth, None, None, Some(limit))
                .await
                .unwrap();
            let first_pass_ms = first_pass_started.elapsed().as_secs_f64() * 1_000.0;
            let payload_bytes = serde_json::to_vec(&first_page).unwrap().len();
            assert_eq!(first_page.total, count);
            assert_eq!(first_page.projects.len(), count.min(limit));
            for consumers in [1, 5, 10] {
                let rounds = if consumers == 1 { 20 } else { 5 };
                let mut samples = Vec::with_capacity(consumers * rounds);
                let wall = Instant::now();
                for _ in 0..rounds {
                    let mut requests = Vec::with_capacity(consumers);
                    for _ in 0..consumers {
                        let runtime = runtime.clone();
                        let auth = auth.clone();
                        requests.push(tokio::spawn(async move {
                            let started = Instant::now();
                            let page =
                                projects_for_filters_auth(&runtime, &auth, None, None, Some(limit))
                                    .await
                                    .unwrap();
                            let bytes = serde_json::to_vec(&page).unwrap();
                            assert_eq!(page.total, count);
                            assert_eq!(page.projects.len(), count.min(limit));
                            (started.elapsed().as_secs_f64() * 1_000.0, bytes.len())
                        }));
                    }
                    for request in requests {
                        let (ms, bytes) = request.await.unwrap();
                        assert_eq!(bytes, payload_bytes);
                        samples.push(ms);
                    }
                }
                let wall_ms = wall.elapsed().as_secs_f64() * 1_000.0;
                let p50 = percentile_ms(&mut samples, 50);
                let p95 = percentile_ms(&mut samples, 95);
                eprintln!(
                    "PROJECT_SELECTOR_PROFILE projects={count} limit={limit} consumers={consumers} requests={} returned={} payload_bytes={payload_bytes} first_pass_ms={first_pass_ms:.3} p50_ms={p50:.3} p95_ms={p95:.3} batch_wall_ms={wall_ms:.3}",
                    samples.len(), count.min(limit),
                );
            }
        }
    }
}
