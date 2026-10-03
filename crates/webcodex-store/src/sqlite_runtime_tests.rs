use crate::Database;
use std::sync::{Arc, Barrier};

#[test]
fn bundled_sqlite_contains_wal_reset_fix() {
    // https://sqlite.org/wal.html#the_wal_reset_bug -- fixed on this release line
    // in 3.51.3. This is a dependency fence, not a reproduction of the rare race.
    assert!(
        rusqlite::version_number() >= 3_051_003,
        "WAL fix missing: {}",
        rusqlite::version()
    );
    eprintln!(
        "SQLITE_RUNTIME version={} version_number={}",
        rusqlite::version(),
        rusqlite::version_number()
    );
}

#[test]
fn concurrent_wal_checkpoint_and_writer_preserve_all_committed_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("wal.db");
    let db = Database::open(&path).unwrap();
    db.conn_for_tests()
        .execute_batch("CREATE TABLE checkpoint_probe(id INTEGER PRIMARY KEY);")
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let writer_path = path.clone();
    let writer_barrier = barrier.clone();
    let writer = std::thread::spawn(move || {
        let mut conn = rusqlite::Connection::open(writer_path).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        writer_barrier.wait();
        for batch in 0..64 {
            let tx = conn.transaction().unwrap();
            for item in 0..8 {
                tx.execute(
                    "INSERT INTO checkpoint_probe VALUES (?1)",
                    [batch * 8 + item],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
    });
    let checkpoint = rusqlite::Connection::open(&path).unwrap();
    barrier.wait();
    for _ in 0..64 {
        let _: (i64, i64, i64) = checkpoint
            .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        std::thread::yield_now();
    }
    writer.join().unwrap();
    assert_eq!(
        checkpoint
            .query_row("SELECT count(*) FROM checkpoint_probe", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        512
    );
    assert_eq!(
        checkpoint
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
