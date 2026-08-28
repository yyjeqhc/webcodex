use super::*;

#[test]
fn sink_submit_result_sends_result_envelope() {
    type SinkFactory = fn(&str) -> (RunnerSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, expected_client, expected_instance) in [
        ("ws", ws_sink as SinkFactory, "ws-client", "ws-inst"),
        ("quic", quic_sink as SinkFactory, "quic-client", "quic-inst"),
    ] {
        let (sink, mut rx) = make_sink(expected_client);
        let result = CommandResult {
            exit_code: Some(0),
            stdout: Some("hi".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(3),
            error: None,
        };
        assert_eq!(
            sink.submit_result("req-9".to_string(), result).unwrap(),
            webcodex_runner::ResultSubmission::Accepted,
            "{label}"
        );
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.client_id, expected_client, "{label}");
                assert_eq!(
                    payload.result.agent_instance_id, expected_instance,
                    "{label}"
                );
                assert_eq!(payload.result.request_id, "req-9");
                assert_eq!(payload.result.exit_code, Some(0));
                assert_eq!(payload.result.stdout.as_deref(), Some("hi"));
                assert_eq!(payload.command_execution_state, None);
            }
            other => panic!("{label}: expected result, got {:?}", other.kind()),
        }
    }
}

#[test]
fn sink_send_job_update_sends_job_update_envelope() {
    type SinkFactory = fn(&str) -> (RunnerSink, tokio::sync::mpsc::Receiver<AgentEnvelope>);
    for (label, make_sink, expected_client) in [
        ("ws", ws_sink as SinkFactory, "ws-client"),
        ("quic", quic_sink as SinkFactory, "quic-client"),
    ] {
        let (sink, mut rx) = make_sink(expected_client);
        let body = ShellAgentJobUpdateRequest {
            client_id: expected_client.to_string(),
            agent_instance_id: sink.agent_instance_id().to_string(),
            job_id: "job-1".to_string(),
            request_id: Some("req-1".to_string()),
            update_seq: None,
            status: "running".to_string(),
            stdout_chunk: Some(format!("{label}-chunk")),
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        };
        sink.send_job_update(&body).unwrap();
        let env = rx.try_recv().expect("envelope was sent");
        match env {
            AgentEnvelope::JobUpdate { payload } => {
                assert_eq!(payload.client_id, expected_client, "{label}");
                assert_eq!(
                    payload.agent_instance_id,
                    sink.agent_instance_id(),
                    "{label}"
                );
                assert_eq!(payload.job_id, "job-1", "{label}");
                assert_eq!(payload.status, "running", "{label}");
                assert_eq!(
                    payload.stdout_chunk.as_deref(),
                    Some(format!("{label}-chunk").as_str()),
                    "{label}"
                );
            }
            other => panic!("{label}: expected job_update, got {:?}", other.kind()),
        }
    }
}

#[test]
fn sink_try_send_job_update_preserves_full_ws_and_quic_queue_for_retry() {
    for label in ["ws", "quic"] {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        tx.try_send(AgentEnvelope::Ping { ts: 11 }).unwrap();
        let sink = match label {
            "ws" => RunnerSink::WebSocket {
                tx,
                client_id: "stream-client".to_string(),
                agent_instance_id: "stream-instance".to_string(),
            },
            "quic" => RunnerSink::Quic {
                tx,
                client_id: "stream-client".to_string(),
                agent_instance_id: "stream-instance".to_string(),
            },
            _ => unreachable!(),
        };
        let body = ShellAgentJobUpdateRequest {
            client_id: "stream-client".to_string(),
            agent_instance_id: "stream-instance".to_string(),
            job_id: "job-full".to_string(),
            request_id: Some("request-full".to_string()),
            update_seq: Some(2),
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        };

        assert_eq!(sink.try_send_job_update(&body), Ok(false), "{label}");
        assert!(matches!(rx.try_recv(), Ok(AgentEnvelope::Ping { ts: 11 })));
        assert_eq!(sink.try_send_job_update(&body), Ok(true), "{label}");
        assert!(matches!(
            rx.try_recv(),
            Ok(AgentEnvelope::JobUpdate { payload }) if payload.job_id == "job-full"
        ));
    }
}
