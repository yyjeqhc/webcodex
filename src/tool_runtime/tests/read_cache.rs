use super::*;
use crate::auth::AuthKind;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn key() -> ReadKey {
    ReadKey {
        snapshot: SnapshotKey {
            scope: ReadScope::new(None, Some("session-a")),
            target: ReadRevisionTarget {
                project_id: "agent:r:p".into(),
                path: "a.rs".into(),
                client_id: "r".into(),
                runner_instance_id: "instance-a".into(),
                project_root: "/p".into(),
                root_fingerprint: Some("root-a".into()),
            },
        },
        start: 1,
        limit: 3,
        expected_sha256: None,
    }
}

fn output(content: &str) -> Value {
    let range = webcodex_workspace::file_read_range::read_range_from(
        content.as_bytes(),
        EffectiveRange::new(Some(1), Some(3)),
    )
    .unwrap();
    webcodex_workspace::file_read_normalize::success_output(&range, false)
}

fn cached_flight(
    cache: &ReadCache,
    key: ReadKey,
    work: BoxFuture<'static, ToolResult>,
) -> ReadFlight {
    let caller_deadline = Instant::now() + Duration::from_secs(30);
    cache.flight(
        key,
        caller_deadline,
        caller_deadline + PHYSICAL_READ_GRACE,
        work,
    )
}

#[test]
fn snapshot_contains_only_covered_ranges_and_invalidates_changed_sha() {
    let cache = ReadCache::default();
    let mut key = key();
    cache.remember(key.snapshot.clone(), output("a\nb\nc\nd\n"));
    key.start = 2;
    key.limit = 2;
    assert_eq!(cache.snapshot(&key).unwrap()["text"], "b\nc");
    key.limit = 3;
    assert!(cache.snapshot(&key).is_none());
    key.start = 1;
    key.limit = 3;
    cache.remember(key.snapshot.clone(), output("x\ny\nz\n"));
    assert_eq!(cache.snapshot(&key).unwrap()["text"], "x\ny\nz");
    assert_eq!(cache.state.lock().unwrap().snapshots.len(), 1);
    cache.invalidate(&key.snapshot);
    assert!(cache.snapshot(&key).is_none());
}

#[test]
fn snapshot_partitions_session_authority_project_and_runner_target() {
    let cache = ReadCache::default();
    let original = key();
    cache.remember(original.snapshot.clone(), output("a\nb\nc"));
    for field in 0..9 {
        let mut other = original.clone();
        match field {
            0 => other.snapshot.scope.session = Some("session-b".into()),
            1 => other.snapshot.scope.authority = "other".into(),
            2 => other.snapshot.target.project_id.push('x'),
            3 => other.snapshot.target.client_id.push('x'),
            4 => other.snapshot.target.runner_instance_id.push('x'),
            5 => other.snapshot.target.project_root.push('x'),
            6 => other.snapshot.target.root_fingerprint = Some("root-b".into()),
            7 => other.snapshot.target.path.push('x'),
            _ => other.snapshot.scope.session = None,
        }
        assert!(cache.snapshot(&other).is_none());
    }
    let mut stateless = original.snapshot.clone();
    stateless.scope.session = None;
    cache.remember(stateless, output("a\nb\nc"));
    assert_eq!(cache.state.lock().unwrap().snapshots.len(), 1);
}

#[test]
fn authority_partition_includes_credential_and_scope_changes() {
    let mut auth = AuthContext::new(AuthKind::ApiToken);
    auth.user_id = Some("user".into());
    let base = ReadScope::new(Some(&auth), None);
    auth.api_key_id = Some("key-a".into());
    assert!(base != ReadScope::new(Some(&auth), None));
    let credential = ReadScope::new(Some(&auth), None);
    auth.scopes.push("read".into());
    assert!(credential != ReadScope::new(Some(&auth), None));
}

#[test]
fn snapshot_retention_has_entry_and_byte_bounds() {
    let cache = ReadCache::default();
    let original = key();
    for index in 0..MAX_SNAPSHOTS + 1 {
        let mut key = original.snapshot.clone();
        key.target.path = format!("{index}.rs");
        cache.remember(key, output("small"));
    }
    assert_eq!(cache.state.lock().unwrap().snapshots.len(), MAX_SNAPSHOTS);
    for index in 0..MAX_SNAPSHOTS {
        let mut key = original.snapshot.clone();
        key.target.path = format!("large-{index}.rs");
        cache.remember(key, output(&"x".repeat(180 * 1024)));
    }
    let state = cache.state.lock().unwrap();
    assert!(state.bytes <= MAX_SNAPSHOT_BYTES);
    assert!(state.snapshots.len() < MAX_SNAPSHOTS);
}

#[tokio::test]
async fn singleflight_shares_only_pending_work_and_never_retains_errors() {
    let cache = ReadCache::default();
    let count = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let calls = count.clone();
    let mut first = cached_flight(
        &cache,
        key(),
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            rx.await.unwrap();
            ToolResult::err("test failure")
        }
        .boxed(),
    );
    assert!(futures_util::poll!(&mut first).is_pending());
    let second = cached_flight(
        &cache,
        key(),
        async { panic!("duplicate work polled") }.boxed(),
    );
    let retained_completed_handle = second.clone();
    tx.send(()).unwrap();
    assert!(!first.await.success);
    assert!(!second.await.success);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let next = cached_flight(
        &cache,
        key(),
        async { ToolResult::ok(json!({"fresh": true})) }.boxed(),
    );
    assert_eq!(next.await.output["fresh"], true);
    assert!(retained_completed_handle.peek().is_some());
}

#[tokio::test]
async fn singleflight_partitions_exact_authority_session_target_and_range() {
    let cache = ReadCache::default();
    let original = key();
    let _pending = cached_flight(
        &cache,
        original.clone(),
        futures_util::future::pending().boxed(),
    );
    for field in 0..11 {
        let mut other = original.clone();
        match field {
            0 => other.snapshot.scope.authority.push('x'),
            1 => other.snapshot.scope.session = Some("session-b".into()),
            2 => other.snapshot.target.project_id.push('x'),
            3 => other.snapshot.target.client_id.push('x'),
            4 => other.snapshot.target.runner_instance_id.push('x'),
            5 => other.snapshot.target.project_root.push('x'),
            6 => other.snapshot.target.root_fingerprint = Some("root-b".into()),
            7 => other.snapshot.target.path.push('x'),
            8 => other.start += 1,
            9 => other.limit += 1,
            _ => other.expected_sha256 = Some("different-snapshot".into()),
        }
        let mut independent =
            cached_flight(&cache, other, async { ToolResult::ok(json!({})) }.boxed());
        assert!(
            futures_util::poll!(&mut independent).is_ready(),
            "field {field} shared work"
        );
    }
}

#[tokio::test]
async fn singleflight_drops_work_only_when_last_waiter_leaves_and_bounds_slots() {
    struct Guard(Arc<AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let cache = ReadCache::default();
    let drops = Arc::new(AtomicUsize::new(0));
    let guard = Guard(drops.clone());
    let mut first = cached_flight(
        &cache,
        key(),
        async move {
            let _guard = guard;
            futures_util::future::pending::<ToolResult>().await
        }
        .boxed(),
    );
    assert!(futures_util::poll!(&mut first).is_pending());
    let second = cached_flight(&cache, key(), async { panic!("duplicate") }.boxed());
    drop(first);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(second);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let mut flights = Vec::new();
    for start in 1..=MAX_FLIGHTS + 2 {
        let mut key = key();
        key.start = start;
        flights.push(cached_flight(
            &cache,
            key,
            futures_util::future::pending().boxed(),
        ));
    }
    assert_eq!(cache.state.lock().unwrap().flights.len(), MAX_FLIGHTS);
}
