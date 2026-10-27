use super::*;

thread_local! { static OBSERVED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
fn record(id: &str, domain: &'static str, _: Duration, _: Duration) {
    assert_eq!(domain, "audit");
    OBSERVED.with(|events| events.borrow_mut().push(id.into()));
    // Prove callback re-entry does not retain a borrow of the context slot.
    with_connection_trace(None, record, || {});
}
fn slow_wait() {
    observe_slow_connection(StoreDomain::Audit, SLOW_CONNECTION, Duration::ZERO);
}

#[test]
fn slow_connection_identity_is_bounded_nested_and_unwind_safe() {
    OBSERVED.with(|events| events.borrow_mut().clear());
    with_connection_trace(Some("outer"), record, || {
        observe_slow_connection(StoreDomain::Audit, Duration::ZERO, Duration::ZERO);
        slow_wait();
        with_connection_trace(Some("inner"), record, || {
            observe_slow_connection(StoreDomain::Audit, Duration::ZERO, SLOW_CONNECTION);
        });
        for invalid in ["", "secret with spaces", "sql;", &"a".repeat(65)] {
            with_connection_trace(Some(invalid), record, slow_wait);
        }
        let result =
            catch_unwind(|| with_connection_trace(Some("panics"), record, || panic!("fixture")));
        assert!(result.is_err());
        slow_wait();
    });
    slow_wait(); // No stale context after leaving the synchronous scope.
    OBSERVED.with(|events| assert_eq!(*events.borrow(), ["outer", "inner", "outer"]));
}

#[test]
fn slow_callback_failure_cannot_poison_or_hold_the_store_mutex() {
    fn fail(_: &str, _: &'static str, _: Duration, _: Duration) {
        panic!("fixture callback");
    }
    let conn = Mutex::new(Connection::open_in_memory().unwrap());
    with_connection_trace(Some("trace"), fail, || {
        let mut guard = lock_connection(&conn, &TracingStoreConnectionObserver, StoreDomain::Audit);
        guard.set_wait_duration(SLOW_CONNECTION);
        drop(guard);
        assert!(conn.try_lock().is_ok());
    });
}
