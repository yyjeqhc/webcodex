use super::*;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use webcodex_core::runner_operation::RunnerOperation;

#[derive(Debug, Default)]
struct TimingTelemetry {
    dequeued: Mutex<Vec<(RunnerTransport, Duration)>>,
    round_trips: Mutex<Vec<(RunnerTransport, Duration)>>,
}

impl RunnerRegistryTelemetry for TimingTelemetry {
    fn request_enqueued(
        &self,
        _request: &RunnerRequest,
        _operation: &RunnerOperation,
        _request_id: &str,
        _client_id: &str,
        _job_id: Option<&str>,
        _runner_instance_id: Option<&str>,
        _runner_transport: Option<&str>,
        _runner_version: Option<&str>,
        _runner_git_commit: Option<&str>,
    ) {
    }

    fn runner_request_dequeued(
        &self,
        _request_id: &str,
        transport: RunnerTransport,
        queue_wait: Duration,
    ) {
        self.dequeued.lock().unwrap().push((transport, queue_wait));
    }

    fn runner_result_accepted(&self, _request_id: &str, _payload: &RunnerResultPayload) {}

    fn runner_request_round_trip(
        &self,
        _request_id: &str,
        transport: RunnerTransport,
        round_trip: Duration,
    ) {
        self.round_trips
            .lock()
            .unwrap()
            .push((transport, round_trip));
    }

    fn runner_result_finalized(&self, _request_id: &str) {}

    fn runner_job_update_accepted(
        &self,
        _request_id: Option<&str>,
        _job_id: &str,
        _payload: &RunnerJobUpdateRequest,
    ) {
    }

    fn runner_job_finalized(&self, _request_id: Option<&str>, _job_id: &str) {}
}

async fn registered_metric_runner(registry: &RunnerRegistry, client_id: &str) {
    registry
        .register(current_runner_registration(RunnerRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            runner_instance_id: "metric-inst".to_string(),
            runner_protocol_generation: RUNNER_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: crate::test_support::current_runner_capabilities(
                RunnerCapabilities::default(),
            ),
            policy: None,
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn queue_wait_and_round_trip_use_server_monotonic_time_and_dispatch_transport() {
    let telemetry = Arc::new(TimingTelemetry::default());
    let registry = RunnerRegistry::with_telemetry(telemetry.clone());
    registered_metric_runner(&registry, "metric-runner").await;
    registry
        .set_transport("metric-runner", RunnerTransport::WebSocket)
        .await
        .unwrap();

    let (request_id, waiter) = registry
        .enqueue_run(
            ShellRunRequest {
                login: false,
                client_id: "metric-runner".to_string(),
                cwd: None,
                command: "echo metric".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 10,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();

    {
        let mut inner = registry.inner.lock().await;
        let pending = inner.pending_by_id.get_mut(&request_id).unwrap();
        pending.enqueued_at = Instant::now() - Duration::from_millis(25);
    }

    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "metric-runner".to_string(),
            runner_instance_id: "metric-inst".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);

    // Changing the current transport after dispatch must not relabel the
    // already-dispatched request when its result later arrives.
    registry
        .set_transport("metric-runner", RunnerTransport::Quic)
        .await
        .unwrap();

    registry
        .complete(RunnerResultRequest {
            client_id: "metric-runner".to_string(),
            runner_instance_id: "metric-inst".to_string(),
            request_id,
            exit_code: Some(0),
            stdout: Some("metric\n".to_string()),
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(3),
            error: None,
        })
        .await
        .unwrap();
    assert!(waiter.await.unwrap().success);

    let dequeued = telemetry.dequeued.lock().unwrap();
    assert_eq!(dequeued.len(), 1);
    assert_eq!(dequeued[0].0, RunnerTransport::WebSocket);
    assert!(dequeued[0].1 >= Duration::from_millis(25));
    drop(dequeued);

    let round_trips = telemetry.round_trips.lock().unwrap();
    assert_eq!(round_trips.len(), 1);
    assert_eq!(round_trips[0].0, RunnerTransport::WebSocket);
    assert!(round_trips[0].1 >= Duration::from_millis(25));
}

#[tokio::test]
async fn undispatched_request_emits_no_latency_samples_instead_of_zero() {
    let telemetry = Arc::new(TimingTelemetry::default());
    let registry = RunnerRegistry::with_telemetry(telemetry.clone());
    registered_metric_runner(&registry, "metric-unavailable").await;

    let (request_id, _waiter) = registry
        .enqueue_run(
            ShellRunRequest {
                login: false,
                client_id: "metric-unavailable".to_string(),
                cwd: None,
                command: "echo unavailable".to_string(),
                stdin: None,
                timeout_secs: 10,
                wait_timeout_secs: 10,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    registry.cancel_request(&request_id).await;

    assert!(telemetry.dequeued.lock().unwrap().is_empty());
    assert!(telemetry.round_trips.lock().unwrap().is_empty());
}
