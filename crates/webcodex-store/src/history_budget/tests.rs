use super::*;

fn fixture() -> (tempfile::TempDir, Arc<Database>) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&tmp.path().join("history.db")).unwrap());
    (tmp, db)
}

#[test]
fn history_sql_deadline_restores_policy_and_does_not_interrupt_the_next_reader() {
    let (_tmp, db) = fixture();
    let at = Instant::now();
    let budget = HistoryReadBudget::until(at + Duration::from_millis(10));
    assert!(db.with_history_read_budget(budget, |db| {
        let mut conn = db.lock_history_connection(StoreDomain::WindowActivity)?;
        let tx = conn.transaction()?;
        tx.query_row("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT SUM(x) FROM n",[],|r|r.get::<_,i64>(0))?;
        tx.commit()?;
        Ok(())
    }).is_err());
    let conn = db
        .lock_history_connection(StoreDomain::WindowActivity)
        .unwrap();
    assert!(conn.is_autocommit());
    assert_eq!(
        conn.query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5000
    );
    assert_eq!(
        conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    eprintln!(
        "HISTORY_SQL budget_ms=10 interrupted=true next_reader_healthy=true elapsed_ms={:.3}",
        at.elapsed().as_secs_f64() * 1000.0
    );
}

#[test]
fn cancellation_stops_history_lock_wait_without_releasing_another_owner() {
    let (_tmp, db) = fixture();
    let held = db
        .lock_history_connection(StoreDomain::WindowActivity)
        .unwrap();
    let budget = HistoryReadBudget::until(Instant::now() + Duration::from_secs(5));
    let worker_budget = budget.clone();
    let (done, finished) = std::sync::mpsc::channel();
    let worker = db.clone();
    let join = std::thread::spawn(move || {
        done.send(
            worker
                .with_history_read_budget(worker_budget, |db| {
                    db.count_window_activity_summaries(None)
                })
                .is_err(),
        )
        .unwrap();
    });
    budget.cancel();
    let stopped = finished.recv_timeout(Duration::from_secs(2));
    drop(held);
    join.join().unwrap();
    assert!(stopped.unwrap());
    assert_eq!(db.count_window_activity_summaries(None).unwrap(), 0);
}

#[test]
fn cancellation_interrupts_executing_sql_and_restores_the_connection() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    let (_tmp, db) = fixture();
    let (started, observed) = std::sync::mpsc::channel();
    let mut first = true;
    db.lock_history_connection(StoreDomain::WindowActivity)
        .unwrap()
        .authorizer(Some(move |context: AuthContext<'_>| {
            if first && matches!(context.action, AuthAction::Select) {
                first = false;
                let _ = started.send(());
            }
            Authorization::Allow
        }))
        .unwrap();
    let budget = HistoryReadBudget::until(Instant::now() + Duration::from_secs(10));
    let worker_budget = budget.clone();
    let worker = db.clone();
    let (done, finished) = std::sync::mpsc::channel();
    let join = std::thread::spawn(move || {
        let result=worker.with_history_read_budget(worker_budget, |db| {
            let conn=db.lock_history_connection(StoreDomain::WindowActivity)?;
            conn.query_row("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT SUM(x) FROM n",[],|r|r.get::<_,i64>(0))?;
            Ok(())
        });
        done.send(result.is_err()).unwrap();
    });
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    budget.cancel();
    let stopped = finished.recv_timeout(Duration::from_secs(3));
    join.join().unwrap();
    assert!(stopped.unwrap());
    let conn = db
        .lock_history_connection(StoreDomain::WindowActivity)
        .unwrap();
    conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
        .unwrap();
    assert_eq!(
        conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(conn.is_autocommit());
}

#[test]
fn budgeted_repair_interruption_rolls_back_before_releasing_writer() {
    let (_tmp, db) = fixture();
    db.conn_for_tests()
        .execute_batch("CREATE TABLE repair_probe(value INTEGER)")
        .unwrap();
    let budget = HistoryReadBudget::until(Instant::now() + Duration::from_millis(10));
    let result=db.with_history_read_budget(budget,|db| {
        let mut conn=db.lock_history_repair_connection(StoreDomain::WindowActivity)?;
        let tx=conn.transaction()?;
        tx.execute("INSERT INTO repair_probe VALUES (1)",[])?;
        tx.query_row("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT SUM(x) FROM n",[],|r|r.get::<_,i64>(0))?;
        tx.commit()?;
        Ok(())
    });
    assert!(result.is_err());
    let conn = db.conn_for_tests();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM repair_probe", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5000
    );
    assert!(conn.is_autocommit());
}

#[test]
fn expired_history_budget_does_not_change_canonical_writer_policy() {
    let (_tmp, db) = fixture();
    let before = db
        .conn_for_tests()
        .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
        .unwrap();
    let budget = HistoryReadBudget::until(Instant::now());
    assert!(db
        .with_history_read_budget(budget, |db| -> anyhow::Result<()> {
            let _writer = db.lock_history_repair_connection(StoreDomain::WindowActivity)?;
            panic!("expired repair cannot run")
        })
        .is_err());
    assert_eq!(
        db.conn_for_tests()
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        before
    );
}
