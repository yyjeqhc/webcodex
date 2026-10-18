use super::super::delivery::run_job_heartbeat_worker;
use super::*;
use std::cell::Cell;

const JOB_ID: &str = "heartbeat-worker";
const INTERVAL: Duration = Duration::from_millis(300);

struct HeartbeatFixture {
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
    pending: Arc<Mutex<HashMap<String, JobUpdateDeliveryQueue>>>,
    delivery_order: Arc<Mutex<()>>,
    shutting_down: Arc<AtomicBool>,
    signal: Arc<JobUpdateDeliverySignal>,
}

impl HeartbeatFixture {
    fn new() -> Self {
        let job = RunningJob {
            client_id: "test-agent".to_string(),
            runner_instance_id: "test-instance".to_string(),
            snapshot: test_job_snapshot(JOB_ID),
            input: None,
            child: None,
            stop_requested: Arc::new(AtomicBool::new(false)),
            slot_reserved: true,
        };
        Self {
            jobs: Arc::new(Mutex::new(HashMap::from([(JOB_ID.to_string(), job)]))),
            pending: Arc::default(),
            delivery_order: Arc::default(),
            shutting_down: Arc::default(),
            signal: Arc::default(),
        }
    }

    fn sequence(&self) -> u64 {
        lock_unpoison(&self.jobs)[JOB_ID].snapshot.update_seq
    }

    fn run(
        &self,
        now: impl Fn() -> Instant,
        wait: impl FnMut(&JobUpdateDeliverySignal, u64, Duration) -> Option<u64>,
    ) {
        run_job_heartbeat_worker(
            Arc::downgrade(&self.jobs),
            Arc::downgrade(&self.pending),
            Arc::downgrade(&self.delivery_order),
            Arc::downgrade(&self.shutting_down),
            Arc::downgrade(&self.signal),
            INTERVAL,
            now,
            wait,
        );
    }
}

#[test]
fn heartbeat_worker_keeps_fixed_cadence_across_signals_and_exits_on_shutdown() {
    let fixture = HeartbeatFixture::new();
    let start = Instant::now();
    let now = Cell::new(start);
    // (wake time, requested wait, current sequence, ordinary notification).
    // The fourth wake consumes the notification emitted by the first heartbeat.
    let wakeups = [
        (180, 300, 1, true),
        (299, 120, 1, true),
        (300, 1, 1, false),
        (300, 300, 2, false),
        (480, 300, 2, true),
        (600, 120, 2, false),
    ];
    let mut waits = 0;
    fixture.run(
        || now.get(),
        |signal, observed, remaining| {
            if waits == wakeups.len() {
                assert_eq!(remaining, INTERVAL);
                assert_eq!(fixture.sequence(), 3);
                signal.close();
                // Closing wins even if the next heartbeat would now be due.
                now.set(start + INTERVAL * 3);
                waits += 1;
                return signal.wait_for_change(observed, Duration::ZERO);
            }
            let (elapsed_ms, remaining_ms, sequence, notify) = wakeups[waits];
            assert_eq!(remaining, Duration::from_millis(remaining_ms));
            assert_eq!(fixture.sequence(), sequence);
            now.set(start + Duration::from_millis(elapsed_ms));
            if notify {
                signal.notify();
            }
            waits += 1;
            Some(signal.generation())
        },
    );
    assert_eq!(waits, wakeups.len() + 1);
    assert_eq!(fixture.sequence(), 3);
    let pending = lock_unpoison(&fixture.pending);
    let heartbeat = pending[JOB_ID].output_only.as_ref().unwrap();
    assert_eq!(heartbeat.update_seq, 3);
    assert!(heartbeat.liveness_only);
}

#[test]
fn heartbeat_worker_shutdown_prevents_a_due_batch() {
    let fixture = HeartbeatFixture::new();
    let start = Instant::now();
    let now = Cell::new(start);
    let mut waits = 0;
    fixture.run(
        || now.get(),
        |signal, _, remaining| {
            assert_eq!(waits, 0);
            assert_eq!(remaining, INTERVAL);
            now.set(start + INTERVAL);
            fixture.shutting_down.store(true, Ordering::SeqCst);
            signal.notify();
            waits += 1;
            Some(signal.generation())
        },
    );
    assert_eq!(waits, 1);
    assert_eq!(fixture.sequence(), 1);
    assert!(lock_unpoison(&fixture.pending).is_empty());
}
