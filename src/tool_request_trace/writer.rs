//! Writer responsibility of the existing trace subsystem.
use super::*;

pub(super) const TRACE_WRITER_QUEUE_CAPACITY: usize = 64;

pub(super) static TRACE_WRITER: OnceLock<Option<TraceWriter>> = OnceLock::new();

#[derive(Debug)]
pub(super) enum TraceWrite {
    Metadata {
        trace_id: String,
        phase: String,
        event: Value,
        config: TraceStoreConfig,
    },
    Payload {
        trace_id: String,
        phase: String,
        value: Value,
        event: Value,
        config: TraceStoreConfig,
    },
    Flush(mpsc::Sender<()>),
}

#[derive(Debug)]
pub(super) struct TraceWriter {
    pub(super) sender: mpsc::SyncSender<TraceWrite>,
}

impl TraceWriter {
    pub(super) fn start() -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(TRACE_WRITER_QUEUE_CAPACITY);
        thread::Builder::new()
            .name("webcodex-tool-trace-writer".to_string())
            .spawn(move || trace_writer_loop(receiver))?;
        Ok(Self { sender })
    }
}

pub(super) fn trace_writer() -> Option<&'static TraceWriter> {
    TRACE_WRITER
        .get_or_init(|| match TraceWriter::start() {
            Ok(writer) => Some(writer),
            Err(error) => {
                tracing::warn!(
                    event = "tool_trace_writer_unavailable",
                    error = %error,
                    "tool_trace_writer_unavailable"
                );
                None
            }
        })
        .as_ref()
}

pub(super) fn trace_writer_loop(receiver: mpsc::Receiver<TraceWrite>) {
    while let Ok(write) = receiver.recv() {
        match write {
            TraceWrite::Metadata {
                trace_id,
                phase,
                event,
                config,
            } => match persist_metadata_event_with_config(&trace_id, event, &config)
                .inspect(|accepted| {
                    if !accepted {
                        TRACE_BUDGET_DROPS.fetch_add(1, Ordering::Relaxed);
                    }
                })
                .inspect_err(|_| {
                    TRACE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
                }) {
                Ok(true) => {}
                Ok(false) => tracing::warn!(
                    event = "tool_trace_capture_omitted",
                    server_trace_id = %trace_id,
                    phase = %phase,
                    reason = "trace_disk_budget_exceeded",
                    "tool_trace_capture_omitted"
                ),
                Err(error) => tracing::warn!(
                    event = "tool_trace_capture_failed",
                    server_trace_id = %trace_id,
                    phase = %phase,
                    error = %error,
                    "tool_trace_capture_failed"
                ),
            },
            TraceWrite::Payload {
                trace_id,
                phase,
                value,
                event,
                config,
            } => match persist_payload_with_config(&trace_id, &phase, &value, event, &config)
                .inspect(|accepted| {
                    if accepted.is_none() {
                        TRACE_BUDGET_DROPS.fetch_add(1, Ordering::Relaxed);
                    }
                })
                .inspect_err(|_| {
                    TRACE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
                }) {
                Ok(Some((payload_bytes, compressed_bytes, digest, path))) => tracing::info!(
                    event = "tool_trace_payload_persisted",
                    server_trace_id = %trace_id,
                    phase = %phase,
                    payload_bytes = payload_bytes as u64,
                    compressed_bytes = compressed_bytes as u64,
                    payload_sha256 = %digest,
                    payload_path = %path,
                    "tool_trace_payload_persisted"
                ),
                Ok(None) => tracing::warn!(
                    event = "tool_trace_capture_omitted",
                    server_trace_id = %trace_id,
                    phase = %phase,
                    reason = "trace_disk_budget_exceeded",
                    "tool_trace_capture_omitted"
                ),
                Err(error) => tracing::warn!(
                    event = "tool_trace_capture_failed",
                    server_trace_id = %trace_id,
                    phase = %phase,
                    error = %error,
                    "tool_trace_capture_failed"
                ),
            },
            TraceWrite::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}

pub(super) fn flush_trace_writer_for_read() -> Result<(), TraceReadError> {
    let Some(writer) = trace_writer() else {
        return Err(TraceReadError::new(
            "trace_store_unavailable",
            "full trace writer is unavailable",
        ));
    };
    let (done_tx, done_rx) = mpsc::channel();
    match writer.sender.try_send(TraceWrite::Flush(done_tx)) {
        Ok(()) => done_rx.recv_timeout(Duration::from_secs(2)).map_err(|_| {
            TraceReadError::new("trace_store_busy", "full trace writer flush timed out")
        }),
        Err(mpsc::TrySendError::Full(_)) => Err(TraceReadError::new(
            "trace_store_busy",
            "full trace writer queue is busy; retry the diagnostic read",
        )),
        Err(mpsc::TrySendError::Disconnected(_)) => Err(TraceReadError::new(
            "trace_store_unavailable",
            "full trace writer is disconnected",
        )),
    }
}

pub(super) fn enqueue_trace_write(write: TraceWrite, trace_id: &str, phase: &str) {
    let Some(writer) = trace_writer() else {
        TRACE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(
            event = "tool_trace_capture_failed",
            server_trace_id = %trace_id,
            phase = phase,
            error = "trace writer unavailable",
            "tool_trace_capture_failed"
        );
        return;
    };
    match writer
        .sender
        .try_send(write)
        .inspect_err(|error| match error {
            mpsc::TrySendError::Full(_) => {
                TRACE_QUEUE_DROPS.fetch_add(1, Ordering::Relaxed);
            }
            mpsc::TrySendError::Disconnected(_) => {
                TRACE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
            }
        }) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full(_)) => tracing::warn!(
            event = "tool_trace_capture_omitted",
            server_trace_id = %trace_id,
            phase = phase,
            reason = "trace_writer_queue_full",
            "tool_trace_capture_omitted"
        ),
        Err(mpsc::TrySendError::Disconnected(_)) => tracing::warn!(
            event = "tool_trace_capture_failed",
            server_trace_id = %trace_id,
            phase = phase,
            error = "trace writer disconnected",
            "tool_trace_capture_failed"
        ),
    }
}

pub(super) fn enqueue_metadata_event(trace_id: &str, phase: &str, event: Value) {
    enqueue_trace_write(
        TraceWrite::Metadata {
            trace_id: trace_id.to_string(),
            phase: phase.to_string(),
            event,
            config: trace_store_config(),
        },
        trace_id,
        phase,
    );
}

#[cfg(test)]
pub(crate) fn flush_full_trace_writer() {
    let Some(writer) = trace_writer() else {
        panic!("full trace writer unavailable");
    };
    let (done_tx, done_rx) = mpsc::channel();
    writer
        .sender
        .send(TraceWrite::Flush(done_tx))
        .expect("full trace writer disconnected before flush");
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("full trace writer flush timed out");
}

pub(super) fn capture_owned_payload_for_trace(trace_id: &str, phase: &str, value: Value) {
    if !full_trace_enabled() {
        return;
    }
    enqueue_trace_write(
        TraceWrite::Payload {
            trace_id: trace_id.to_string(),
            phase: phase.to_string(),
            value,
            event: base_event(trace_id, "tool_trace_payload_captured"),
            config: trace_store_config(),
        },
        trace_id,
        phase,
    );
}

pub(super) fn capture_payload_for_trace(trace_id: &str, phase: &str, value: &Value) {
    if !full_trace_enabled() {
        return;
    }
    capture_owned_payload_for_trace(trace_id, phase, value.clone());
}
