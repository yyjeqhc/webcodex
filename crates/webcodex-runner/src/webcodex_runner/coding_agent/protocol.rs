//! Bounded ACP framing, sole outbound writer, and normalized observations.

use super::*;

struct OutboundWriteRequest {
    frame: Vec<u8>,
    completion: mpsc::SyncSender<Result<(), AcpFailure>>,
}

pub(super) struct PendingOutboundWrite {
    completion: Receiver<Result<(), AcpFailure>>,
}

pub(super) struct AcpOutboundWriter {
    requests: Option<mpsc::SyncSender<OutboundWriteRequest>>,
    finished: Arc<(Mutex<bool>, Condvar)>,
}

struct OutboundWriterFinished(Arc<(Mutex<bool>, Condvar)>);

impl Drop for OutboundWriterFinished {
    fn drop(&mut self) {
        let (finished, changed) = &*self.0;
        let mut finished = finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *finished = true;
        changed.notify_all();
    }
}

impl AcpOutboundWriter {
    pub(super) fn spawn<W: Write + Send + 'static>(
        mut sink: W,
        threads: &BackgroundThreads,
    ) -> std::io::Result<Self> {
        let (requests, receiver) = mpsc::sync_channel::<OutboundWriteRequest>(1);
        let finished = Arc::new((Mutex::new(false), Condvar::new()));
        let thread_finished = Arc::clone(&finished);
        let handle = thread::Builder::new()
            .name("wc-acp-stdin".to_string())
            .spawn(move || {
                let _finished = OutboundWriterFinished(thread_finished);
                while let Ok(request) = receiver.recv() {
                    let result = sink
                        .write_all(&request.frame)
                        .and_then(|_| sink.flush())
                        .map_err(|error| AcpFailure::WriteIo(error.kind()));
                    let failed = result.is_err();
                    let _ = request.completion.send(result);
                    if failed {
                        break;
                    }
                }
            })?;
        threads.register(handle);
        Ok(Self {
            requests: Some(requests),
            finished,
        })
    }

    pub(super) fn start_frame(&self, frame: Vec<u8>) -> Result<PendingOutboundWrite, AcpFailure> {
        let requests = self
            .requests
            .as_ref()
            .ok_or(AcpFailure::WriterUnavailable)?;
        let (completion, receiver) = mpsc::sync_channel(1);
        requests
            .try_send(OutboundWriteRequest { frame, completion })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => AcpFailure::WriterBusy,
                mpsc::TrySendError::Disconnected(_) => AcpFailure::WriterUnavailable,
            })?;
        Ok(PendingOutboundWrite {
            completion: receiver,
        })
    }

    pub(super) fn close(&mut self) {
        self.requests.take();
    }

    pub(super) fn wait_finished_until(&self, deadline: Instant) -> bool {
        let (finished, changed) = &*self.finished;
        let mut finished = finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*finished {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, timed_out) = changed
                .wait_timeout(finished, remaining.min(ACP_POLL))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            finished = next;
            if timed_out.timed_out() && Instant::now() >= deadline {
                return false;
            }
        }
        true
    }
}

impl Drop for AcpOutboundWriter {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutboundInterruption {
    Cancelled,
    Shutdown,
    Deadline,
}

pub(super) enum OutboundWriteOutcome {
    Written,
    Failed(AcpFailure),
    Interrupted(OutboundInterruption),
}

impl OutboundWriteOutcome {
    pub(super) fn failure(self) -> Option<AcpFailure> {
        match self {
            Self::Written => None,
            Self::Failed(failure) => Some(failure),
            Self::Interrupted(OutboundInterruption::Deadline) => Some(AcpFailure::TimedOut),
            Self::Interrupted(_) => Some(AcpFailure::Interrupted),
        }
    }
}

#[derive(Debug)]
pub(super) enum ReaderEvent {
    Message(Value),
    Eof,
    Malformed,
    TooLarge,
    Io(std::io::ErrorKind),
}

/// Runner-private diagnostic causes. Provider text and raw OS errors never
/// become terminal messages, durable records, or Server observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcpFailure {
    ProcessExited,
    OutputClosed,
    Malformed,
    TooLarge,
    ReadIo(std::io::ErrorKind),
    WriteIo(std::io::ErrorKind),
    SpawnIo(std::io::ErrorKind),
    WorkerIo(std::io::ErrorKind),
    WriterUnavailable,
    WriterBusy,
    TimedOut,
    Interrupted,
    Disconnected,
    RpcRejected(Option<i64>),
    MissingResult,
    InvalidFrame,
}

impl AcpFailure {
    pub(super) fn from_reader(event: ReaderEvent) -> Self {
        match event {
            ReaderEvent::Eof => Self::OutputClosed,
            ReaderEvent::Malformed => Self::Malformed,
            ReaderEvent::TooLarge => Self::TooLarge,
            ReaderEvent::Io(kind) => Self::ReadIo(kind),
            ReaderEvent::Message(_) => unreachable!("messages are handled before transport faults"),
        }
    }

    pub(super) fn rpc_rejected(error: &Value) -> Self {
        Self::RpcRejected(error.get("code").and_then(Value::as_i64))
    }

    pub(super) fn safe_message(self) -> &'static str {
        match self {
            Self::ProcessExited => "ACP process exited before responding; check the executable and explicit Runner environment mappings",
            Self::OutputClosed => "ACP stdout closed before responding; check the executable and explicit Runner environment mappings",
            Self::Malformed => "ACP returned malformed JSON",
            Self::TooLarge => "ACP message exceeded the size limit",
            Self::ReadIo(_) => "ACP stdout read failed",
            Self::WriteIo(_) => "ACP stdin write failed",
            Self::SpawnIo(_) => "ACP process could not start; check the executable and explicit Runner environment mappings",
            Self::WorkerIo(_) => "ACP I/O worker could not start",
            Self::WriterUnavailable => "ACP stdin writer is unavailable",
            Self::WriterBusy => "ACP stdin writer is busy",
            Self::TimedOut => "ACP request timed out",
            Self::Interrupted => "ACP setup interrupted",
            Self::Disconnected => "ACP transport reader disconnected",
            Self::RpcRejected(_) => "ACP provider rejected the request",
            Self::MissingResult => "ACP response is missing its result",
            Self::InvalidFrame => "ACP request could not be encoded within the message limit",
        }
    }

    /// Record only bounded, controlled facts on the Runner, before cleanup can
    /// create an exit status of its own. EOF alone does not prove process exit.
    pub(super) fn diagnose(
        self,
        run_id: &str,
        stage: &str,
        status: Option<std::process::ExitStatus>,
    ) -> Self {
        let failure = if status.is_some()
            && matches!(
                self,
                Self::OutputClosed | Self::WriterUnavailable | Self::WriteIo(_)
            ) {
            Self::ProcessExited
        } else {
            self
        };
        let io_kind = match self {
            Self::ReadIo(kind)
            | Self::WriteIo(kind)
            | Self::SpawnIo(kind)
            | Self::WorkerIo(kind) => Some(kind),
            _ => None,
        };
        let rpc_code = match self {
            Self::RpcRejected(code) => code,
            _ => None,
        };
        #[cfg(unix)]
        let exit_signal = {
            use std::os::unix::process::ExitStatusExt;
            status.and_then(|status| status.signal())
        };
        #[cfg(not(unix))]
        let exit_signal: Option<i32> = None;
        tracing::warn!(run_id, stage, reason = failure.safe_message(), ?io_kind, ?rpc_code,
            exit_code = ?status.and_then(|status| status.code()), ?exit_signal, "ACP request failed");
        failure
    }
}

pub(super) fn remaining_run_budget(run_deadline: Instant) -> Option<Duration> {
    let remaining = run_deadline.saturating_duration_since(Instant::now());
    (!remaining.is_zero()).then_some(remaining)
}

pub(super) fn bounded_setup_wait(run_deadline: Instant) -> Option<Duration> {
    remaining_run_budget(run_deadline).map(|remaining| remaining.min(ACP_SETUP_TIMEOUT))
}

pub(super) fn wait_outbound_write(
    pending: PendingOutboundWrite,
    deadline: Instant,
    cancelled: Option<&AtomicBool>,
    accepting: Option<&AtomicBool>,
) -> OutboundWriteOutcome {
    loop {
        match pending.completion.try_recv() {
            Ok(Ok(())) => return OutboundWriteOutcome::Written,
            Ok(Err(error)) => return OutboundWriteOutcome::Failed(error),
            Err(mpsc::TryRecvError::Disconnected) => {
                return OutboundWriteOutcome::Failed(AcpFailure::WriterUnavailable);
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return OutboundWriteOutcome::Interrupted(OutboundInterruption::Cancelled);
        }
        if accepting.is_some_and(|flag| !flag.load(Ordering::Acquire)) {
            return OutboundWriteOutcome::Interrupted(OutboundInterruption::Shutdown);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return OutboundWriteOutcome::Interrupted(OutboundInterruption::Deadline);
        }
        match pending.completion.recv_timeout(remaining.min(ACP_POLL)) {
            Ok(Ok(())) => return OutboundWriteOutcome::Written,
            Ok(Err(error)) => return OutboundWriteOutcome::Failed(error),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return OutboundWriteOutcome::Failed(AcpFailure::WriterUnavailable);
            }
        }
    }
}

pub(super) fn wait_response(
    rx: &Receiver<ReaderEvent>,
    id: u64,
    timeout: Duration,
    interrupted: Option<&AtomicBool>,
) -> Result<Value, AcpFailure> {
    let deadline = Instant::now() + timeout;
    loop {
        if interrupted.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err(AcpFailure::Interrupted);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AcpFailure::TimedOut);
        }
        match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(ReaderEvent::Message(message)) => {
                if message.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if let Some(error) = message.get("error") {
                    return Err(AcpFailure::rpc_rejected(error));
                }
                return message
                    .get("result")
                    .cloned()
                    .ok_or(AcpFailure::MissingResult);
            }
            Ok(event) => return Err(AcpFailure::from_reader(event)),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Err(AcpFailure::Disconnected),
        }
    }
}

pub(super) fn request_frame(id: u64, method: &str, params: Value) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
}

pub(super) fn notification_frame(method: &str, params: Value) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","method":method,"params":params}))
}

pub(super) fn result_frame(id: u64, result: Value) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","id":id,"result":result}))
}

pub(super) fn error_frame(id: u64, code: i64, message: &str) -> std::io::Result<Vec<u8>> {
    frame_message(&json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
}

fn frame_message(value: &Value) -> std::io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    if bytes.len() > ACP_MESSAGE_MAX_BYTES {
        return Err(std::io::Error::other("ACP message too large"));
    }
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn normalize_update(message: &Value) -> Option<CodingAgentEvent> {
    let update = message.get("params")?.get("update")?;
    let kind = update.get("sessionUpdate")?.as_str()?;
    let text = update
        .get("content")
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
        .map(bounded_text);
    match kind {
        "agent_message_chunk" => Some(event(CodingAgentEventKind::AgentMessage, text, None, None)),
        "agent_thought_chunk" => Some(event(CodingAgentEventKind::Reasoning, text, None, None)),
        "plan" => Some(event(
            CodingAgentEventKind::Plan,
            None,
            Some("plan".to_string()),
            Some("updated".to_string()),
        )),
        "tool_call" | "tool_call_update" => {
            let label = update
                .get("title")
                .and_then(Value::as_str)
                .map(bounded_text)
                .or_else(|| update.get("kind").and_then(Value::as_str).map(bounded_text));
            let status = update
                .get("status")
                .and_then(Value::as_str)
                .map(bounded_text);
            let event_kind = match update.get("kind").and_then(Value::as_str) {
                Some("edit") | Some("delete") | Some("move") => CodingAgentEventKind::FileChange,
                Some("execute") => CodingAgentEventKind::TerminalActivity,
                _ => CodingAgentEventKind::ToolActivity,
            };
            Some(event(event_kind, None, label, status))
        }
        "usage_update" => {
            let usage = CodingAgentUsage {
                used_tokens: update.get("used").and_then(Value::as_u64),
                context_window_tokens: update.get("size").and_then(Value::as_u64),
                cost_amount: update
                    .pointer("/cost/amount")
                    .and_then(Value::as_f64)
                    .map(|amount| amount.to_string()),
                cost_currency: update
                    .pointer("/cost/currency")
                    .and_then(Value::as_str)
                    .map(bounded_text),
            };
            Some(CodingAgentEvent {
                sequence: 0,
                kind: CodingAgentEventKind::Usage,
                text: None,
                label: None,
                status: None,
                usage: Some(usage),
            })
        }
        _ => None,
    }
}

pub(super) fn permission_event(params: &Value) -> CodingAgentEvent {
    let label = params
        .pointer("/toolCall/title")
        .and_then(Value::as_str)
        .map(bounded_text);
    let count = params
        .get("options")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    CodingAgentEvent {
        sequence: 0,
        kind: CodingAgentEventKind::PermissionRequest,
        text: None,
        label,
        status: Some(format!("pending:{count}_options")),
        usage: None,
    }
}

fn event(
    kind: CodingAgentEventKind,
    text: Option<String>,
    label: Option<String>,
    status: Option<String>,
) -> CodingAgentEvent {
    CodingAgentEvent {
        sequence: 0,
        kind,
        text,
        label,
        status,
        usage: None,
    }
}

pub(super) fn bounded_text(value: &str) -> String {
    const MAX: usize = webcodex_core::coding_agent::CODING_AGENT_MAX_EVENT_TEXT_BYTES;
    const SUFFIX: &str = "…";
    if value.len() <= MAX {
        return value.to_string();
    }
    let mut end = MAX.saturating_sub(SUFFIX.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut bounded = String::with_capacity(MAX);
    bounded.push_str(&value[..end]);
    bounded.push_str(SUFFIX);
    debug_assert!(bounded.len() <= MAX);
    bounded
}
