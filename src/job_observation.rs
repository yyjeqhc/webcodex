use std::fmt;

const LEGACY_TOKEN_PREFIX: &str = "wjob1";
const TOKEN_V2_AGENT_PREFIX: &str = "wj2a";
const TOKEN_V2_LOCAL_PREFIX: &str = "wj2l";
pub(crate) const MAX_JOB_OBSERVATION_TOKEN_LEN: usize = 192;
const MAX_JOB_ID_LEN: usize = 80;
const MAX_EPOCH_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobObservationExecutor {
    Agent,
    Local,
}

impl JobObservationExecutor {
    fn legacy_code(self) -> &'static str {
        match self {
            Self::Agent => "a",
            Self::Local => "l",
        }
    }

    fn v2_prefix(self) -> &'static str {
        match self {
            Self::Agent => TOKEN_V2_AGENT_PREFIX,
            Self::Local => TOKEN_V2_LOCAL_PREFIX,
        }
    }

    fn parse_legacy(value: &str) -> Result<Self, JobObservationTokenError> {
        match value {
            "a" => Ok(Self::Agent),
            "l" => Ok(Self::Local),
            _ => Err(JobObservationTokenError::Malformed),
        }
    }

    fn parse_v2_prefix(value: &str) -> Result<Self, JobObservationTokenError> {
        match value {
            TOKEN_V2_AGENT_PREFIX => Ok(Self::Agent),
            TOKEN_V2_LOCAL_PREFIX => Ok(Self::Local),
            _ => Err(JobObservationTokenError::Malformed),
        }
    }
}

/// Opaque, Job-bound model observation state.
///
/// Legacy `wjob1` tokens have no log cursor proof. Current `wj2*` tokens carry
/// the next absolute stdout/stderr line that an automatic delta observation
/// should inspect. These cursors are observation state only: they are not
/// execution identity, authority, idempotency, or Runner protocol sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobObservationToken {
    pub(crate) executor: JobObservationExecutor,
    pub(crate) job_id: String,
    pub(crate) epoch: String,
    pub(crate) revision: u64,
    pub(crate) stdout_cursor: Option<u64>,
    pub(crate) stderr_cursor: Option<u64>,
}

impl JobObservationToken {
    pub(crate) fn new(
        executor: JobObservationExecutor,
        job_id: impl Into<String>,
        epoch: impl Into<String>,
        revision: u64,
        stdout_cursor: u64,
        stderr_cursor: u64,
    ) -> Result<Self, JobObservationTokenError> {
        Self::build(
            executor,
            job_id.into(),
            epoch.into(),
            revision,
            Some(stdout_cursor),
            Some(stderr_cursor),
        )
    }

    pub(crate) fn new_legacy(
        executor: JobObservationExecutor,
        job_id: impl Into<String>,
        epoch: impl Into<String>,
        revision: u64,
    ) -> Result<Self, JobObservationTokenError> {
        Self::build(executor, job_id.into(), epoch.into(), revision, None, None)
    }

    fn build(
        executor: JobObservationExecutor,
        job_id: String,
        epoch: String,
        revision: u64,
        stdout_cursor: Option<u64>,
        stderr_cursor: Option<u64>,
    ) -> Result<Self, JobObservationTokenError> {
        if stdout_cursor.is_some() != stderr_cursor.is_some() {
            return Err(JobObservationTokenError::Malformed);
        }
        if stdout_cursor.is_some_and(|cursor| cursor == 0)
            || stderr_cursor.is_some_and(|cursor| cursor == 0)
        {
            return Err(JobObservationTokenError::Malformed);
        }
        let token = Self {
            executor,
            job_id,
            epoch,
            revision,
            stdout_cursor,
            stderr_cursor,
        };
        validate_component(&token.job_id, MAX_JOB_ID_LEN)?;
        validate_component(&token.epoch, MAX_EPOCH_LEN)?;
        if token.encode().len() > MAX_JOB_OBSERVATION_TOKEN_LEN {
            return Err(JobObservationTokenError::Oversized);
        }
        Ok(token)
    }

    pub(crate) fn parse(value: &str) -> Result<Self, JobObservationTokenError> {
        if value.is_empty() || !value.is_ascii() {
            return Err(JobObservationTokenError::Malformed);
        }
        if value.len() > MAX_JOB_OBSERVATION_TOKEN_LEN {
            return Err(JobObservationTokenError::Oversized);
        }
        if value.starts_with("wjob1:") {
            Self::parse_legacy(value)
        } else {
            Self::parse_v2(value)
        }
    }

    fn parse_legacy(value: &str) -> Result<Self, JobObservationTokenError> {
        let mut parts = value.split(':');
        if parts.next() != Some(LEGACY_TOKEN_PREFIX) {
            return Err(JobObservationTokenError::Malformed);
        }
        let executor = JobObservationExecutor::parse_legacy(
            parts.next().ok_or(JobObservationTokenError::Malformed)?,
        )?;
        let job_id = parts
            .next()
            .ok_or(JobObservationTokenError::Malformed)?
            .to_string();
        let epoch = parts
            .next()
            .ok_or(JobObservationTokenError::Malformed)?
            .to_string();
        let revision = parse_decimal(parts.next().ok_or(JobObservationTokenError::Malformed)?)?;
        if parts.next().is_some() {
            return Err(JobObservationTokenError::Malformed);
        }
        let token = Self::build(executor, job_id, epoch, revision, None, None)?;
        if token.encode() != value {
            return Err(JobObservationTokenError::Malformed);
        }
        Ok(token)
    }

    fn parse_v2(value: &str) -> Result<Self, JobObservationTokenError> {
        let mut parts = value.split(':');
        let executor = JobObservationExecutor::parse_v2_prefix(
            parts.next().ok_or(JobObservationTokenError::Malformed)?,
        )?;
        let job_id = parts
            .next()
            .ok_or(JobObservationTokenError::Malformed)?
            .to_string();
        let epoch = parts
            .next()
            .ok_or(JobObservationTokenError::Malformed)?
            .to_string();
        let revision = parse_base36(parts.next().ok_or(JobObservationTokenError::Malformed)?)?;
        let stdout_cursor = parse_base36(parts.next().ok_or(JobObservationTokenError::Malformed)?)?;
        let stderr_cursor = parse_base36(parts.next().ok_or(JobObservationTokenError::Malformed)?)?;
        if parts.next().is_some() {
            return Err(JobObservationTokenError::Malformed);
        }
        let token = Self::build(
            executor,
            job_id,
            epoch,
            revision,
            Some(stdout_cursor),
            Some(stderr_cursor),
        )?;
        if token.encode() != value {
            return Err(JobObservationTokenError::Malformed);
        }
        Ok(token)
    }

    pub(crate) fn parse_bound(
        value: &str,
        executor: JobObservationExecutor,
        job_id: &str,
    ) -> Result<Self, JobObservationTokenError> {
        let token = Self::parse(value)?;
        if token.executor != executor {
            return Err(JobObservationTokenError::WrongExecutor);
        }
        if token.job_id != job_id {
            return Err(JobObservationTokenError::WrongJob);
        }
        Ok(token)
    }

    pub(crate) fn is_legacy(&self) -> bool {
        self.stdout_cursor.is_none()
    }

    pub(crate) fn encode(&self) -> String {
        match (self.stdout_cursor, self.stderr_cursor) {
            (Some(stdout_cursor), Some(stderr_cursor)) => format!(
                "{}:{}:{}:{}:{}:{}",
                self.executor.v2_prefix(),
                self.job_id,
                self.epoch,
                encode_base36(self.revision),
                encode_base36(stdout_cursor),
                encode_base36(stderr_cursor),
            ),
            (None, None) => format!(
                "{LEGACY_TOKEN_PREFIX}:{}:{}:{}:{}",
                self.executor.legacy_code(),
                self.job_id,
                self.epoch,
                self.revision
            ),
            _ => unreachable!("Job observation cursors are both present or both absent"),
        }
    }
}

fn parse_decimal(value: &str) -> Result<u64, JobObservationTokenError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(JobObservationTokenError::Malformed);
    }
    value
        .parse::<u64>()
        .map_err(|_| JobObservationTokenError::Malformed)
}

fn encode_base36(mut value: u64) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut encoded = [0_u8; 13];
    let mut index = encoded.len();
    while value > 0 {
        index -= 1;
        let digit = (value % 36) as u8;
        encoded[index] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        value /= 36;
    }
    String::from_utf8(encoded[index..].to_vec()).expect("base36 is ASCII")
}

fn parse_base36(value: &str) -> Result<u64, JobObservationTokenError> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return Err(JobObservationTokenError::Malformed);
    }
    let parsed = value.bytes().try_fold(0_u64, |parsed, byte| {
        let digit = match byte {
            b'0'..=b'9' => u64::from(byte - b'0'),
            b'a'..=b'z' => u64::from(byte - b'a' + 10),
            _ => return None,
        };
        parsed.checked_mul(36)?.checked_add(digit)
    });
    parsed.ok_or(JobObservationTokenError::Malformed)
}

fn validate_component(value: &str, max_len: usize) -> Result<(), JobObservationTokenError> {
    if value.is_empty() || value.len() > max_len {
        return Err(JobObservationTokenError::Malformed);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(JobObservationTokenError::Malformed);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobLogDeltaStatus {
    Baseline,
    Delta,
    Unchanged,
    Reset,
}

impl JobLogDeltaStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Delta => "delta",
            Self::Unchanged => "unchanged",
            Self::Reset => "reset",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobLogSelectionMode {
    Baseline,
    Delta { cursor: u64 },
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobLogStreamProjection {
    pub(crate) text: String,
    pub(crate) next_line: usize,
    pub(crate) total_lines: usize,
    pub(crate) returned_lines: usize,
    pub(crate) first_retained_line: usize,
    pub(crate) truncated: bool,
    pub(crate) delta_reset: bool,
}

/// Select one bounded model-facing stream from an already frozen retained
/// snapshot. Storage remains executor-specific; only baseline/delta/reset
/// range semantics are shared.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_log_stream(
    retained_text: &str,
    first_retained_line: usize,
    next_line: usize,
    storage_truncated: bool,
    tail_lines: Option<usize>,
    mode: JobLogSelectionMode,
    preserve_trailing_newline: bool,
) -> JobLogStreamProjection {
    let lines = retained_text.lines().collect::<Vec<_>>();
    let first_retained_line = first_retained_line.max(1);
    let expected_next = first_retained_line.saturating_add(lines.len());
    let next_line = next_line.max(1);
    let snapshot_consistent = next_line == expected_next;
    let tail_bound = tail_lines.filter(|lines| *lines > 0);

    let baseline_start = || {
        tail_bound
            .map(|tail| lines.len().saturating_sub(tail))
            .unwrap_or(0)
    };
    let (start, delta_reset) = match mode {
        JobLogSelectionMode::Baseline => (baseline_start(), false),
        JobLogSelectionMode::Reset => (baseline_start(), true),
        JobLogSelectionMode::Delta { cursor } => {
            let cursor = usize::try_from(cursor).ok();
            let continuous = snapshot_consistent
                && cursor
                    .is_some_and(|cursor| cursor >= first_retained_line && cursor <= next_line);
            if !continuous {
                (baseline_start(), true)
            } else {
                let delta_start = cursor
                    .expect("continuous cursor is present")
                    .saturating_sub(first_retained_line)
                    .min(lines.len());
                let bounded_start = tail_bound
                    .map(|tail| lines.len().saturating_sub(tail).max(delta_start))
                    .unwrap_or(delta_start);
                (bounded_start, bounded_start > delta_start)
            }
        }
    };
    let selected = lines[start..].join("\n");
    let text = if preserve_trailing_newline && !selected.is_empty() {
        format!("{selected}\n")
    } else {
        selected
    };
    let baseline_or_reset = matches!(
        mode,
        JobLogSelectionMode::Baseline | JobLogSelectionMode::Reset
    );
    let truncated = baseline_or_reset
        && (storage_truncated || first_retained_line > 1 || start > 0)
        || delta_reset && matches!(mode, JobLogSelectionMode::Delta { .. }) && start > 0;
    JobLogStreamProjection {
        text,
        next_line,
        total_lines: next_line.saturating_sub(1),
        returned_lines: lines.len().saturating_sub(start),
        first_retained_line,
        truncated,
        delta_reset,
    }
}

pub(crate) fn combined_delta_status(
    mode: JobLogSelectionMode,
    stdout: &JobLogStreamProjection,
    stderr: &JobLogStreamProjection,
) -> JobLogDeltaStatus {
    match mode {
        JobLogSelectionMode::Baseline => JobLogDeltaStatus::Baseline,
        JobLogSelectionMode::Reset => JobLogDeltaStatus::Reset,
        JobLogSelectionMode::Delta { .. } if stdout.delta_reset || stderr.delta_reset => {
            JobLogDeltaStatus::Reset
        }
        JobLogSelectionMode::Delta { .. } if stdout.text.is_empty() && stderr.text.is_empty() => {
            JobLogDeltaStatus::Unchanged
        }
        JobLogSelectionMode::Delta { .. } => JobLogDeltaStatus::Delta,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobObservationTokenError {
    Malformed,
    Oversized,
    WrongExecutor,
    WrongJob,
}

impl fmt::Display for JobObservationTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Malformed => "invalid after_observation_token: malformed opaque Job token",
            Self::Oversized => "invalid after_observation_token: token exceeds 192 bytes",
            Self::WrongExecutor => {
                "invalid after_observation_token: token belongs to a different executor"
            }
            Self::WrongJob => "invalid after_observation_token: token belongs to a different Job",
        })
    }
}

pub(crate) fn new_epoch() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_token_v2_round_trips_canonically_with_independent_cursors() {
        let token = JobObservationToken::new(
            JobObservationExecutor::Agent,
            "11111111-2222-3333-4444-555555555555",
            "0123456789abcdef0123456789abcdef",
            42,
            101,
            21,
        )
        .unwrap();
        let encoded = token.encode();
        assert!(encoded.starts_with("wj2a:"));
        assert!(encoded.len() <= MAX_JOB_OBSERVATION_TOKEN_LEN);
        assert_eq!(JobObservationToken::parse(&encoded).unwrap(), token);
        assert_eq!(
            JobObservationToken::parse(&encoded).unwrap().encode(),
            encoded
        );
        assert_eq!(token.stdout_cursor, Some(101));
        assert_eq!(token.stderr_cursor, Some(21));
    }

    #[test]
    fn observation_token_v2_maximum_components_fit_exact_existing_bound() {
        let token = JobObservationToken::new(
            JobObservationExecutor::Local,
            "j".repeat(MAX_JOB_ID_LEN),
            "e".repeat(MAX_EPOCH_LEN),
            u64::MAX,
            u64::MAX,
            u64::MAX,
        )
        .unwrap();
        assert_eq!(token.encode().len(), MAX_JOB_OBSERVATION_TOKEN_LEN);
        assert_eq!(JobObservationToken::parse(&token.encode()).unwrap(), token);
    }

    #[test]
    fn observation_token_v2_rejects_wrong_binding_and_noncanonical_integers() {
        let encoded = JobObservationToken::new(
            JobObservationExecutor::Local,
            "job-one",
            "0123456789abcdef",
            36,
            101,
            21,
        )
        .unwrap()
        .encode();
        assert_eq!(
            JobObservationToken::parse_bound(&encoded, JobObservationExecutor::Agent, "job-one"),
            Err(JobObservationTokenError::WrongExecutor)
        );
        assert_eq!(
            JobObservationToken::parse_bound(&encoded, JobObservationExecutor::Local, "job-two"),
            Err(JobObservationTokenError::WrongJob)
        );
        for malformed in [
            "wj2l:job:epoch:01:1:1",
            "wj2l:job:epoch:1:01:1",
            "wj2l:job:epoch:1:1:01",
            "wj2l:job:epoch:A:1:1",
            "wj2l:job:epoch:1:1",
            "wj2l:job:epoch:1:1:1:extra",
            "wj2x:job:epoch:1:1:1",
            "wj2l:job:epoch:1:0:1",
            "wj2l:job:epoch:1:1:0",
        ] {
            assert_eq!(
                JobObservationToken::parse(malformed),
                Err(JobObservationTokenError::Malformed),
                "{malformed}"
            );
        }
        assert_eq!(
            JobObservationToken::parse(&"x".repeat(MAX_JOB_OBSERVATION_TOKEN_LEN + 1)),
            Err(JobObservationTokenError::Oversized)
        );
    }

    #[test]
    fn legacy_observation_token_remains_canonical_and_has_no_cursor_proof() {
        let legacy = JobObservationToken::new_legacy(
            JobObservationExecutor::Local,
            "job-one",
            "0123456789abcdef",
            1,
        )
        .unwrap();
        let encoded = legacy.encode();
        assert_eq!(encoded, "wjob1:l:job-one:0123456789abcdef:1");
        let parsed = JobObservationToken::parse(&encoded).unwrap();
        assert!(parsed.is_legacy());
        assert_eq!(parsed.stdout_cursor, None);
        assert_eq!(parsed.stderr_cursor, None);
        assert_eq!(parsed.encode(), encoded);
        assert_eq!(
            JobObservationToken::parse("wjob1:l:job:epoch:01"),
            Err(JobObservationTokenError::Malformed)
        );
    }

    #[test]
    fn delta_projection_distinguishes_unchanged_delta_and_reset() {
        let unchanged = project_log_stream(
            "l8\nl9\nl10\n",
            8,
            11,
            true,
            Some(3),
            JobLogSelectionMode::Delta { cursor: 11 },
            true,
        );
        assert_eq!(unchanged.text, "");
        assert!(!unchanged.delta_reset);
        assert!(!unchanged.truncated);

        let delta = project_log_stream(
            "l8\nl9\nl10\nl11\nl12\n",
            8,
            13,
            true,
            Some(3),
            JobLogSelectionMode::Delta { cursor: 11 },
            true,
        );
        assert_eq!(delta.text, "l11\nl12\n");
        assert!(!delta.delta_reset);

        let reset = project_log_stream(
            "l8\nl9\nl10\n",
            8,
            11,
            true,
            Some(2),
            JobLogSelectionMode::Delta { cursor: 3 },
            false,
        );
        assert_eq!(reset.text, "l9\nl10");
        assert!(reset.delta_reset);
        assert!(reset.truncated);

        let inconsistent_snapshot = project_log_stream(
            "l8\nl9\nl10\n",
            8,
            99,
            false,
            Some(2),
            JobLogSelectionMode::Delta { cursor: 11 },
            false,
        );
        assert_eq!(inconsistent_snapshot.text, "l9\nl10");
        assert_eq!(inconsistent_snapshot.next_line, 99);
        assert!(inconsistent_snapshot.delta_reset);
    }
}
