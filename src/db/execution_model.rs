use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectorExecution {
    pub execution_id: String,
    pub task_id: String,
    pub run_id: String,
    pub state: String,
    pub submitted_at: i64,
    pub queued_at: Option<i64>,
    pub queue_deadline: i64,
    pub started_at: Option<i64>,
    pub last_output_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub stdout_cursor: usize,
    pub stderr_cursor: usize,
    pub exit_code: Option<i32>,
    pub failure_source: Option<String>,
    pub failure_code: Option<String>,
    pub terminal_reason: Option<String>,
    pub operation_id: String,
    pub request_sha256: String,
    pub executor_reference: Option<String>,
    pub first_status_failure_at: Option<i64>,
    pub last_successful_observation_at: Option<i64>,
    pub status_failure_code: Option<String>,
}

impl ConnectorExecution {
    pub(crate) fn state_is_active(state: &str) -> bool {
        matches!(
            state,
            "accepted" | "queued" | "starting" | "running" | "cancel_requested"
        )
    }

    pub(crate) fn is_active(&self) -> bool {
        Self::state_is_active(&self.state)
    }

    pub(crate) fn is_terminal(&self) -> bool {
        !self.is_active()
    }

    pub(crate) fn blocks_finish(&self) -> bool {
        self.is_active() || self.state == "unknown"
    }

    pub(crate) fn executor_status_recognized(status: &str) -> bool {
        matches!(
            status,
            "queued"
                | "agent_queued"
                | "running"
                | "started"
                | "stop_requested"
                | "completed"
                | "stopped"
                | "cancelled"
                | "timeout"
                | "timed_out"
                | "lost"
                | "failed"
        )
    }
}

pub(crate) enum ConnectorExecutionReservation {
    Created(ConnectorExecution),
    Existing(ConnectorExecution),
}

pub(crate) enum ConnectorExecutionFailure {
    Submission(&'static str),
    Unknown(&'static str),
}

pub(crate) struct ConnectorExecutionObservation<'a> {
    pub executor_status: &'a str,
    pub stdout_cursor: usize,
    pub stderr_cursor: usize,
    pub exit_code: Option<i32>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub now: i64,
}

type Fact = Option<&'static str>;
type StateOutcome = (&'static str, Fact, Fact, Fact);
const EXECUTOR: Fact = Some("executor");
const UNKNOWN_REASON: &str = "executor_terminal_unknown";

fn active_state(execution: &ConnectorExecution, state: &'static str) -> StateOutcome {
    let state = (execution.state == "cancel_requested")
        .then_some("cancel_requested")
        .unwrap_or(state);
    (state, None, None, None)
}

fn failed(source: &'static str, code: &'static str, reason: &'static str) -> StateOutcome {
    ("failed", Some(source), Some(code), Some(reason))
}

fn unknown(code: &'static str) -> StateOutcome {
    ("unknown", EXECUTOR, Some(code), Some(UNKNOWN_REASON))
}

pub(super) fn observed_state(
    execution: &ConnectorExecution,
    observation: &ConnectorExecutionObservation<'_>,
) -> StateOutcome {
    match observation.executor_status {
        "queued" | "agent_queued" => active_state(execution, "queued"),
        "running" | "started" => active_state(execution, "running"),
        "stop_requested" => active_state(execution, "running"),
        "completed" if observation.exit_code == Some(0) => {
            ("succeeded", None, None, Some("exit_zero"))
        }
        "completed" if observation.exit_code.is_none() => unknown("executor_exit_code_missing"),
        "stopped" | "cancelled"
            if execution.state == "cancel_requested"
                && execution.failure_code.as_deref() == Some("queue_deadline") =>
        {
            failed("queue", "queue_deadline", "queue_timeout")
        }
        "stopped" | "cancelled" if execution.state == "cancel_requested" => {
            ("cancelled", None, None, Some("user_cancelled"))
        }
        "timeout" | "timed_out" => failed("executor", "command_timeout", "timeout"),
        "lost" => unknown("executor_lost"),
        "failed" | "completed" => failed("command", "nonzero_exit", "nonzero_exit"),
        _ => active_state(execution, "running"),
    }
}

fn query_execution<P: rusqlite::Params>(
    conn: &rusqlite::Connection,
    sql: &str,
    params: P,
) -> rusqlite::Result<Option<ConnectorExecution>> {
    conn.query_row(sql, params, map_execution).optional()
}

pub(super) fn latest_execution(
    conn: &rusqlite::Connection,
    task_id: &str,
) -> rusqlite::Result<Option<ConnectorExecution>> {
    query_execution(
        conn,
        &format!(
            "SELECT {EXECUTION_COLUMNS} FROM wc_executions
             WHERE task_id = ?1 ORDER BY submitted_at DESC, rowid DESC LIMIT 1"
        ),
        params![task_id],
    )
}

pub(super) fn load_execution_by_operation(
    conn: &rusqlite::Connection,
    task_id: &str,
    run_id: &str,
    operation_id: &str,
) -> rusqlite::Result<Option<ConnectorExecution>> {
    query_execution(
        conn,
        &format!(
            "SELECT {EXECUTION_COLUMNS} FROM wc_executions
             WHERE task_id = ?1 AND run_id = ?2 AND operation_id = ?3"
        ),
        params![task_id, run_id, operation_id],
    )
}

pub(super) fn load_execution(
    conn: &rusqlite::Connection,
    execution_id: &str,
) -> rusqlite::Result<Option<ConnectorExecution>> {
    query_execution(
        conn,
        &format!("SELECT {EXECUTION_COLUMNS} FROM wc_executions WHERE id = ?1"),
        params![execution_id],
    )
}

pub(super) const EXECUTION_COLUMNS: &str = "id, task_id, run_id, state, submitted_at, queued_at, \
    queue_deadline, started_at, last_output_at, finished_at, stdout_cursor, stderr_cursor, \
    exit_code, failure_source, failure_code, terminal_reason, operation_id, request_sha256, \
    executor_reference, first_status_failure_at, last_successful_observation_at, \
    status_failure_code";

pub(super) fn map_execution(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectorExecution> {
    let cursor = |index| {
        let value = row.get::<_, i64>(index)?;
        usize::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
    };
    Ok(ConnectorExecution {
        execution_id: row.get(0)?,
        task_id: row.get(1)?,
        run_id: row.get(2)?,
        state: row.get(3)?,
        submitted_at: row.get(4)?,
        queued_at: row.get(5)?,
        queue_deadline: row.get(6)?,
        started_at: row.get(7)?,
        last_output_at: row.get(8)?,
        finished_at: row.get(9)?,
        stdout_cursor: cursor(10)?,
        stderr_cursor: cursor(11)?,
        exit_code: row.get(12)?,
        failure_source: row.get(13)?,
        failure_code: row.get(14)?,
        terminal_reason: row.get(15)?,
        operation_id: row.get(16)?,
        request_sha256: row.get(17)?,
        executor_reference: row.get(18)?,
        first_status_failure_at: row.get(19)?,
        last_successful_observation_at: row.get(20)?,
        status_failure_code: row.get(21)?,
    })
}

pub(super) fn execution_event_kind(state: &str) -> &'static str {
    match state {
        "accepted" => "execution_accepted",
        "queued" => "execution_queued",
        "starting" | "running" => "execution_started",
        "cancel_requested" => "execution_cancel_requested",
        "succeeded" => "execution_succeeded",
        "cancelled" => "execution_cancelled",
        "interrupted" => "execution_interrupted",
        "unknown" => "execution_unknown",
        _ => "execution_failed",
    }
}
