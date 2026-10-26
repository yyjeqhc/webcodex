//! Explicit synthetic Console benchmark. No production data or timing gate.
use super::*;
use std::time::{Duration, Instant};

fn percentiles(mut samples: Vec<f64>) -> (f64, f64) {
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).ceil() as usize];
    (at(0.5), at(0.95))
}

fn report(label: &str, samples: Vec<f64>, failures: usize, counts: (usize, usize)) {
    let n = samples.len();
    let (p50, p95) = percentiles(samples);
    eprintln!(
        "WINDOW_CONSOLE_BENCH case={label} samples={n} p50_ms={p50:.3} p95_ms={p95:.3} errors={failures} history_entries={} inventory_entries={}",
        counts.0, counts.1
    );
}

fn count_delta(db: &crate::Database, before: (usize, usize)) -> (usize, usize) {
    let after = db.window_read_counts_for_test();
    (after.0 - before.0, after.1 - before.1)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "manual synthetic Console load benchmark; no wall-clock threshold"]
async fn window_console_load_benchmark() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = test_bootstrap_auth();
    for p in 0..8 {
        register_project(
            &runtime,
            &format!("bench-{p}"),
            "repo",
            "/synthetic/bench",
            Some(&auth),
        )
        .await;
    }
    let client_window = crate::client_window::ClientWindow::for_test("console-load-active");
    let key = client_window.key().to_string();
    record_window_event(&db, &auth, &key, Some("agent:bench-0:repo"), None, 1);
    {
        let conn = db.conn_for_tests();
        let session: String = conn
            .query_row("SELECT session_id FROM action_events LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        conn.execute(
            "WITH RECURSIVE w(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM w WHERE n+1<400),
             h(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM h WHERE n+1<250)
             INSERT INTO action_events(event_id,session_id,started_at,ended_at,duration_ms,endpoint,action_name,operation,project,status,changed_files_json,ids_json,summary_json,client_window_key,client_window_source,principal_correlation_kind,principal_correlation_id,window_started_at_ms,window_ended_at_ms,window_meaningful)
             SELECT printf('load-%d-%d',w.n,h.n),?1,1,1,1,'/mcp','toolsCall','read_files',printf('agent:bench-%d:repo',h.n%8),'success','[]','{}','{}',CASE WHEN w.n=0 THEN ?2 ELSE printf('%064x',w.n) END,'openai-session','username',printf('benchmark-%d',h.n%4),1000000+w.n*250+h.n,1000001+w.n*250+h.n,1 FROM w CROSS JOIN h",
            [session.as_str(), key.as_str()],
        ).unwrap();
        conn.execute(
            "WITH RECURSIVE h(n) AS (SELECT 250 UNION ALL SELECT n+1 FROM h WHERE n+1<2000)
             INSERT INTO action_events(event_id,session_id,started_at,ended_at,duration_ms,endpoint,action_name,operation,project,status,changed_files_json,ids_json,summary_json,client_window_key,client_window_source,principal_correlation_kind,principal_correlation_id,window_started_at_ms,window_ended_at_ms,window_meaningful)
             SELECT printf('extra-%d',h.n),?1,1,1,1,'/mcp','toolsCall','read_files',printf('agent:bench-%d:repo',h.n%8),'success','[]','{}','{}',?2,'openai-session','username',printf('benchmark-%d',h.n%4),2000000+h.n,2000001+h.n,1 FROM h",
            [session.as_str(), key.as_str()],
        ).unwrap();
        let events: i64 = conn
            .query_row("SELECT count(*) FROM action_events", [], |row| row.get(0))
            .unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        eprintln!("WINDOW_CONSOLE_FIXTURE windows=400 projects=8 synthetic_principals=4 bootstrap_seed=1 events={events} journal_mode={mode}");
        assert_eq!(mode, "wal");
    }
    // Repair is measured separately; warm samples exclude fixture setup.
    let start = Instant::now();
    assert_eq!(
        windows_for_auth(&runtime, &auth, Some(50), None)
            .await
            .unwrap()
            .total,
        400
    );
    eprintln!(
        "WINDOW_CONSOLE_REPAIR initial_inventory_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let _active =
        runtime
            .window_activity
            .start(&client_window, "benchmark-active", "tools/call", None);
    for (label, detail_level, limit) in [
        ("primary_active", WindowDetailLevel::Primary, 20),
        ("full_2000_active", WindowDetailLevel::Full, 2000),
    ] {
        let before = db.window_read_counts_for_test();
        let mut samples = Vec::new();
        for _ in 0..31 {
            let start = Instant::now();
            let detail = window_for_auth(
                &runtime,
                &auth,
                WindowInput {
                    client_window_key: key.clone(),
                    activity_limit: Some(limit),
                    session_limit: Some(20),
                    detail_level,
                },
            )
            .await
            .unwrap();
            assert_eq!(detail.activity.len(), limit);
            assert_eq!(detail.active_count, 1);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        report(label, samples, 0, count_delta(&db, before));
    }
    let before = db.window_read_counts_for_test();
    let mut samples = Vec::new();
    for _ in 0..31 {
        let start = Instant::now();
        let page = windows_for_auth(&runtime, &auth, Some(50), None)
            .await
            .unwrap();
        assert_eq!((page.total, page.returned), (400, 50));
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    report("inventory_50", samples, 0, count_delta(&db, before));

    // Four synchronized rounds of 16 tabs; both inventory and primary compete
    // for the actual global history lane. No artificial per-query delay.
    let before = db.window_read_counts_for_test();
    let mut samples = Vec::new();
    let mut failures = 0;
    for _ in 0..4 {
        let barrier = Arc::new(tokio::sync::Barrier::new(16));
        let mut tasks = Vec::new();
        for tab in 0..16 {
            let (runtime, auth, key, barrier) =
                (runtime.clone(), auth.clone(), key.clone(), barrier.clone());
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                let start = Instant::now();
                let ok = if tab % 2 == 0 {
                    windows_for_auth(&runtime, &auth, Some(50), None)
                        .await
                        .is_ok()
                } else {
                    window_for_auth(
                        &runtime,
                        &auth,
                        WindowInput {
                            client_window_key: key,
                            activity_limit: Some(20),
                            session_limit: Some(20),
                            detail_level: WindowDetailLevel::Primary,
                        },
                    )
                    .await
                    .is_ok()
                };
                (start.elapsed().as_secs_f64() * 1000.0, ok)
            }));
        }
        for task in tasks {
            let (elapsed, ok) = task.await.unwrap();
            samples.push(elapsed);
            failures += usize::from(!ok);
        }
    }
    report(
        "16_tabs_4_rounds",
        samples,
        failures,
        count_delta(&db, before),
    );

    // Deliberately saturate the existing 5s lane to observe 503 distribution.
    // A cooperative release keeps the owned blocking task alive until all
    // queued readers have expired; no abandoned production work is touched.
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let blocking_db = db.clone();
    let blocker = tokio::spawn(async move {
        super::super::store_read::run(&blocking_db, move |_| {
            let _ = started.send(());
            released.recv_timeout(Duration::from_secs(8))?;
            Ok(())
        })
        .await
    });
    ready.await.unwrap();
    let before = db.window_read_counts_for_test();
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let (runtime, auth) = (runtime.clone(), auth.clone());
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let result = windows_for_auth(&runtime, &auth, Some(50), None).await;
            let unavailable = matches!(
                result,
                Err(RuntimeConsoleError::Request { status: 503, .. })
            );
            (start.elapsed().as_secs_f64() * 1000.0, unavailable)
        }));
    }
    let mut samples = Vec::new();
    let mut unavailable = 0;
    for task in tasks {
        let (elapsed, failed) = task.await.unwrap();
        samples.push(elapsed);
        unavailable += usize::from(failed);
    }
    release.send(()).unwrap();
    let _ = blocker.await.unwrap();
    report(
        "saturated_lane",
        samples,
        unavailable,
        count_delta(&db, before),
    );
    assert_eq!(
        unavailable, 16,
        "overloaded history must explicitly report 503"
    );
    // Recovery must use the same lane and remain available after worker exit.
    assert_eq!(
        windows_for_auth(&runtime, &auth, Some(50), None)
            .await
            .unwrap()
            .total,
        400
    );
}
