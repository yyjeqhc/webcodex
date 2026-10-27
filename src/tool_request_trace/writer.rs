//! Writer responsibility of the existing trace subsystem.
use super::*;

pub(super) const TRACE_WRITER_QUEUE_CAPACITY: usize = 64;
const TRACE_PENDING_LOSS_LIMIT: usize = 128;

#[derive(Debug, Default)]
struct PendingTraceLosses(Mutex<HashMap<String, (TraceStoreConfig, u64)>>);
impl PendingTraceLosses {
    fn record(&self, trace_id: &str, config: &TraceStoreConfig) {
        // Never replace business backpressure with a telemetry mutex wait.
        let Ok(mut losses) = self.0.try_lock() else {
            TRACE_LOSS_MARKER_DROPS.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if trace_id.len() > 64
            || (!losses.contains_key(trace_id) && losses.len() >= TRACE_PENDING_LOSS_LIMIT)
        {
            TRACE_LOSS_MARKER_DROPS.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let (_, count) = losses
            .entry(trace_id.to_owned())
            .or_insert_with(|| (config.clone(), 0));
        *count = count.saturating_add(1);
    }

    fn persist(&self) {
        let pending = { std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner())) };
        for (trace_id, (config, count)) in pending {
            let mut event = base_event(&trace_id, "tool_trace_capture_gap");
            merge_event_fields(
                &mut event,
                json!({"known_dropped_events":count,"coverage":"incomplete"}),
            );
            // Best effort even for this marker: disk failures cannot be made
            // durable by another write. Never recurse or claim completeness.
            if !matches!(
                persist_metadata_event_with_config(&trace_id, event, &config),
                Ok(true)
            ) {
                TRACE_LOSS_MARKER_DROPS.fetch_add(count, Ordering::Relaxed);
            }
        }
    }
}

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
    losses: Arc<PendingTraceLosses>,
}

impl TraceWriter {
    pub(super) fn start() -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(TRACE_WRITER_QUEUE_CAPACITY);
        let losses = Arc::new(PendingTraceLosses::default());
        let worker_losses = Arc::clone(&losses);
        thread::Builder::new()
            .name("webcodex-tool-trace-writer".to_string())
            .spawn(move || trace_writer_loop(receiver, worker_losses))?;
        Ok(Self { sender, losses })
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

fn trace_writer_loop(receiver: mpsc::Receiver<TraceWrite>, losses: Arc<PendingTraceLosses>) {
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
                        losses.record(&trace_id, &config);
                    }
                })
                .inspect_err(|_| {
                    TRACE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
                    losses.record(&trace_id, &config);
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
                        losses.record(&trace_id, &config);
                    }
                })
                .inspect_err(|_| {
                    TRACE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
                    losses.record(&trace_id, &config);
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
                losses.persist();
                let _ = done.send(());
            }
        }
        losses.persist();
    }
    losses.persist();
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
    enqueue_to_writer(writer, write, trace_id, phase);
}

fn enqueue_to_writer(writer: &TraceWriter, write: TraceWrite, trace_id: &str, phase: &str) {
    match writer.sender.try_send(write) {
        Ok(()) => {}
        Err(error) => {
            let (write, reason) = match error {
                mpsc::TrySendError::Full(write) => {
                    TRACE_QUEUE_DROPS.fetch_add(1, Ordering::Relaxed);
                    (write, "trace_writer_queue_full")
                }
                mpsc::TrySendError::Disconnected(write) => {
                    TRACE_WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
                    (write, "trace_writer_disconnected")
                }
            };
            if let TraceWrite::Metadata { config, .. } | TraceWrite::Payload { config, .. } = &write
            {
                writer.losses.record(trace_id, config);
            }
            tracing::warn!(event="tool_trace_capture_omitted", server_trace_id=%trace_id, phase, reason, "tool_trace_capture_omitted");
        }
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

#[cfg(test)]
#[path = "tests/writer_saturation.rs"]
mod saturation_tests;
