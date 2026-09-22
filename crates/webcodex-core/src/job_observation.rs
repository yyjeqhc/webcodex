use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;

const TOKEN_PREFIX: &str = "wj3_";
pub const MAX_JOB_OBSERVATION_TOKEN_LEN: usize = 62;

/// Prefix that distinguishes a compact observation ref from a `job_id` or bare
/// observation token. Short enough to be unambiguous; not a valid `job_id` or
/// `wj3_` token prefix.
const OBSERVATION_REF_PREFIX: &str = "~j";

/// Maximum number of (principal, ref) entries retained across all callers.
/// Refs are ephemeral convenience handles; eviction is LRU on capacity.
const OBSERVATION_REF_REGISTRY_CAPACITY: usize = 512;

/// Maximum observation-ref string length (prefix + up to 20-digit decimal
/// counter). Strict length check keeps deserialization predictable.
pub const MAX_OBSERVATION_REF_LEN: usize = 22;

/// Compact server-issued continuation selector that pins one exact Job
/// observation state (job_id + observation token) for a specific principal.
///
/// A ref is **observation authority only**: it never starts, retries, stops,
/// or redispatches a Job. Dereference re-authorizes visibility through the
/// same canonical path as supplying job_id + after_observation_token directly.
/// Stale or mismatched state fails closed — the underlying job_log call
/// returns the same canonical reset/recovery semantics as today.
///
/// Refs survive only while the process is running; a server restart
/// invalidates all refs fail-closed (the model falls back to job_id + token).
#[derive(Debug)]
pub struct ObservationRefRegistry {
    entries: Mutex<VecDeque<ObservationRefEntry>>,
    counter: Mutex<u64>,
}

#[derive(Debug, Clone)]
struct ObservationRefEntry {
    /// Opaque ref string, e.g. `~j42`.
    ref_str: String,
    /// Stable non-secret principal identifier scoped by the registry owner.
    /// Cross-principal substitution is rejected at dereference time.
    principal_id: String,
    /// Exact job_id bound at mint time.
    job_id: String,
    /// Observation token string bound at mint time (the `observation_token`
    /// field from the successful observe_jobs item output).
    observation_token: String,
}

impl Default for ObservationRefRegistry {
    fn default() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            counter: Mutex::new(0),
        }
    }
}

impl ObservationRefRegistry {
    /// Mint a new compact ref for `(principal_id, job_id, observation_token)`.
    /// Returns the opaque ref string (e.g. `~j42`).
    pub fn mint(
        &self,
        principal_id: impl Into<String>,
        job_id: impl Into<String>,
        observation_token: impl Into<String>,
    ) -> String {
        let mut counter = self.counter.lock().expect("observation ref counter lock");
        let index = *counter;
        *counter = counter.wrapping_add(1);
        drop(counter);

        let ref_str = format!("{OBSERVATION_REF_PREFIX}{index}");
        let entry = ObservationRefEntry {
            ref_str: ref_str.clone(),
            principal_id: principal_id.into(),
            job_id: job_id.into(),
            observation_token: observation_token.into(),
        };

        let mut entries = self.entries.lock().expect("observation ref registry lock");
        if entries.len() >= OBSERVATION_REF_REGISTRY_CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry);
        ref_str
    }

    /// Resolve a ref back to `(job_id, observation_token)` for the given
    /// principal.  Returns `None` when the ref is unknown, expired (evicted),
    /// or belongs to a different principal.
    pub fn resolve(&self, principal_id: &str, ref_str: &str) -> Option<(String, String)> {
        let mut entries = self.entries.lock().expect("observation ref registry lock");
        let index = entries
            .iter()
            .position(|entry| entry.ref_str == ref_str && entry.principal_id == principal_id)?;
        let entry = entries.remove(index)?;
        let resolved = (entry.job_id.clone(), entry.observation_token.clone());
        entries.push_back(entry);
        Some(resolved)
    }

    /// Return true iff the string looks like a well-formed observation ref
    /// (prefix + non-empty decimal suffix, within the length bound).
    /// This is a syntactic check only; it does not prove the ref is known.
    pub fn is_ref_syntax(value: &str) -> bool {
        if value.len() > MAX_OBSERVATION_REF_LEN {
            return false;
        }
        let Some(suffix) = value.strip_prefix(OBSERVATION_REF_PREFIX) else {
            return false;
        };
        !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
    }
}

/// Observation state, never execution identity or authority. A 96-bit digest
/// binds the exact Job and registry generation without repeating either ID.
/// Paired zero cursors mean no log receipt (lifecycle/explicit-page views).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobObservationToken {
    pub binding: String,
    pub revision: u64,
    pub stdout_cursor: Option<u64>,
    pub stderr_cursor: Option<u64>,
}

fn observation_binding(job_id: &str, epoch: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"webcodex/job-observation-binding/v3\0");
    hash.update((epoch.len() as u64).to_be_bytes());
    hash.update(epoch.as_bytes());
    hash.update(job_id.as_bytes());
    crate::compact::encode(&hash.finalize()[..12])
}

impl JobObservationToken {
    pub fn new(
        job_id: impl AsRef<str>,
        epoch: impl AsRef<str>,
        revision: u64,
        stdout_cursor: u64,
        stderr_cursor: u64,
    ) -> Result<Self, JobObservationTokenError> {
        if stdout_cursor == 0 || stderr_cursor == 0 {
            return Err(JobObservationTokenError::Malformed);
        }
        Ok(Self {
            binding: observation_binding(job_id.as_ref(), epoch.as_ref()),
            revision,
            stdout_cursor: Some(stdout_cursor),
            stderr_cursor: Some(stderr_cursor),
        })
    }

    pub fn new_baseline(
        job_id: impl AsRef<str>,
        epoch: impl AsRef<str>,
        revision: u64,
    ) -> Result<Self, JobObservationTokenError> {
        Ok(Self {
            binding: observation_binding(job_id.as_ref(), epoch.as_ref()),
            revision,
            stdout_cursor: None,
            stderr_cursor: None,
        })
    }

    pub fn matches_parent(&self, job_id: &str, epoch: &str) -> bool {
        self.binding == observation_binding(job_id, epoch)
    }

    pub fn requires_baseline(&self) -> bool {
        self.stdout_cursor.is_none()
    }

    pub fn parse(value: &str) -> Result<Self, JobObservationTokenError> {
        if value.len() > MAX_JOB_OBSERVATION_TOKEN_LEN {
            return Err(JobObservationTokenError::Oversized);
        }
        let mut parts = value
            .strip_prefix(TOKEN_PREFIX)
            .ok_or(JobObservationTokenError::Malformed)?
            .split('.');
        let binding = parts.next().ok_or(JobObservationTokenError::Malformed)?;
        crate::compact::decode::<12>(binding).ok_or(JobObservationTokenError::Malformed)?;
        let revision = parse_base36(parts.next().ok_or(JobObservationTokenError::Malformed)?)?;
        let stdout = parse_base36(parts.next().ok_or(JobObservationTokenError::Malformed)?)?;
        let stderr = parse_base36(parts.next().ok_or(JobObservationTokenError::Malformed)?)?;
        if parts.next().is_some() || (stdout == 0) != (stderr == 0) {
            return Err(JobObservationTokenError::Malformed);
        }
        Ok(Self {
            binding: binding.to_string(),
            revision,
            stdout_cursor: (stdout != 0).then_some(stdout),
            stderr_cursor: (stderr != 0).then_some(stderr),
        })
    }

    pub fn encode(&self) -> String {
        format!(
            "{TOKEN_PREFIX}{}.{}.{}.{}",
            self.binding,
            encode_base36(self.revision),
            encode_base36(self.stdout_cursor.unwrap_or(0)),
            encode_base36(self.stderr_cursor.unwrap_or(0))
        )
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobLogDeltaStatus {
    Baseline,
    Delta,
    Unchanged,
    Reset,
}

impl JobLogDeltaStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Delta => "delta",
            Self::Unchanged => "unchanged",
            Self::Reset => "reset",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobLogSelectionMode {
    Baseline,
    Delta { cursor: u64 },
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLogStreamProjection {
    pub text: String,
    pub next_line: usize,
    pub total_lines: usize,
    pub returned_lines: usize,
    pub first_retained_line: usize,
    pub truncated: bool,
    pub delta_reset: bool,
}

/// Select one bounded model-facing stream from an already frozen retained
/// snapshot. Storage remains executor-specific; only baseline/delta/reset
/// range semantics are shared.
#[allow(clippy::too_many_arguments)]
pub fn project_log_stream(
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
    let observed_next_line = next_line.max(1);
    let snapshot_consistent = observed_next_line == expected_next;
    // A visible final partial line is not conclusively consumed: later bytes
    // may extend the same logical line without changing its absolute line count.
    let next_line =
        if snapshot_consistent && !retained_text.is_empty() && !retained_text.ends_with('\n') {
            observed_next_line
                .saturating_sub(1)
                .max(first_retained_line)
        } else {
            observed_next_line
        };
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
                && cursor.is_some_and(|cursor| {
                    cursor >= first_retained_line && cursor <= observed_next_line
                });
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
    let truncated = (baseline_or_reset || delta_reset)
        && (storage_truncated || first_retained_line > 1 || start > 0);
    JobLogStreamProjection {
        text,
        next_line,
        total_lines: observed_next_line.saturating_sub(1),
        returned_lines: lines.len().saturating_sub(start),
        first_retained_line,
        truncated,
        delta_reset,
    }
}

pub fn combined_delta_status(
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
pub enum JobObservationTokenError {
    Malformed,
    Oversized,
}

impl fmt::Display for JobObservationTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Malformed => "invalid after_observation_token: malformed opaque Job token",
            Self::Oversized => "invalid after_observation_token: token exceeds 62 bytes",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_ref_registry_is_principal_scoped_and_lru_bounded() {
        let registry = ObservationRefRegistry::default();
        let keep = registry.mint("principal:a", "job-keep", "token-keep");
        assert_eq!(
            registry.resolve("principal:a", &keep),
            Some(("job-keep".to_string(), "token-keep".to_string()))
        );
        assert_eq!(registry.resolve("principal:b", &keep), None);

        let mut first_other = None;
        for index in 0..(OBSERVATION_REF_REGISTRY_CAPACITY - 1) {
            let reference = registry.mint(
                "principal:a",
                format!("job-{index}"),
                format!("token-{index}"),
            );
            first_other.get_or_insert(reference);
        }

        // Refreshing the oldest live ref must move it to the MRU end.
        assert!(registry.resolve("principal:a", &keep).is_some());
        registry.mint("principal:a", "job-extra", "token-extra");

        assert!(
            registry.resolve("principal:a", &keep).is_some(),
            "recently resolved ref must survive LRU eviction"
        );
        assert_eq!(
            registry.resolve("principal:a", first_other.as_deref().unwrap()),
            None,
            "least-recently-used ref must be evicted at capacity"
        );
    }

    #[test]
    fn observation_ref_syntax_is_small_and_unambiguous() {
        for valid in ["~j0", "~j4", "~j18446744073709551615"] {
            assert!(ObservationRefRegistry::is_ref_syntax(valid), "{valid}");
        }
        for invalid in ["", "~j", "j4", "~j-1", "~j1x", "wj3_abc"] {
            assert!(!ObservationRefRegistry::is_ref_syntax(invalid), "{invalid}");
        }
        assert!(!ObservationRefRegistry::is_ref_syntax(&format!(
            "~j{}",
            "1".repeat(MAX_OBSERVATION_REF_LEN)
        )));
    }

    #[test]
    fn compact_cursor_binding_and_length() {
        let token =
            JobObservationToken::new("wc_job_abcdefghijklmnop", "registry", 42, 101, 21).unwrap();
        assert_eq!(token.encode().len(), 28);
        assert_eq!(JobObservationToken::parse(&token.encode()).unwrap(), token);
        assert!(token.matches_parent("wc_job_abcdefghijklmnop", "registry"));
        assert!(!token.matches_parent("wc_job_other_parent_id", "registry"));
        assert!(!token.matches_parent("wc_job_abcdefghijklmnop", "restart"));
        assert_eq!(token.stdout_cursor, Some(101));
        assert_eq!(token.stderr_cursor, Some(21));
        let max = JobObservationToken::new("job", "epoch", u64::MAX, u64::MAX, u64::MAX).unwrap();
        assert_eq!(max.encode().len(), MAX_JOB_OBSERVATION_TOKEN_LEN);
        assert_eq!(JobObservationToken::parse(&max.encode()).unwrap(), max);
        let baseline = JobObservationToken::new_baseline("job", "epoch", 1).unwrap();
        assert!(JobObservationToken::parse(&baseline.encode())
            .unwrap()
            .requires_baseline());
    }

    #[test]
    fn cursor_rejects_noncanonical_or_malformed_state() {
        for bad in [
            "01.1.1",
            "1.01.1",
            "1.1.01",
            "A.1.1",
            "1.1",
            "1.1.1.extra",
            "1.0.1",
            "1.1.0",
            "1.-1.1",
        ] {
            assert_eq!(
                JobObservationToken::parse(&format!("wj3_abcdefghijklmnop.{bad}")),
                Err(JobObservationTokenError::Malformed)
            );
        }
        for bad in [
            "",
            "wj3_invalid.1.1.1",
            "wj3_abcdefghijklmn+p.1.1.1",
            "wj3_abcdefghijklmnop=.1.1.1",
        ] {
            assert!(JobObservationToken::parse(bad).is_err());
        }
        assert_eq!(
            JobObservationToken::parse(&"x".repeat(63)),
            Err(JobObservationTokenError::Oversized)
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

    #[test]
    fn partial_final_line_remains_the_next_delta_cursor_until_completed() {
        let baseline = project_log_stream(
            "abc",
            1,
            2,
            false,
            Some(10),
            JobLogSelectionMode::Baseline,
            false,
        );
        assert_eq!(baseline.text, "abc");
        assert_eq!(baseline.next_line, 1);
        assert_eq!(baseline.total_lines, 1);

        let first_growth = project_log_stream(
            "abcdef",
            1,
            2,
            false,
            Some(10),
            JobLogSelectionMode::Delta {
                cursor: baseline.next_line as u64,
            },
            false,
        );
        assert_eq!(first_growth.text, "abcdef");
        assert_eq!(first_growth.next_line, 1);
        assert!(!first_growth.delta_reset);

        let second_growth = project_log_stream(
            "abcdefgh",
            1,
            2,
            false,
            Some(10),
            JobLogSelectionMode::Delta {
                cursor: first_growth.next_line as u64,
            },
            false,
        );
        assert_eq!(second_growth.text, "abcdefgh");
        assert_eq!(second_growth.next_line, 1);

        let completed = project_log_stream(
            "abcdefgh\n",
            1,
            2,
            false,
            Some(10),
            JobLogSelectionMode::Delta {
                cursor: second_growth.next_line as u64,
            },
            false,
        );
        assert_eq!(completed.text, "abcdefgh");
        assert_eq!(completed.next_line, 2);
        assert_eq!(completed.total_lines, 1);

        let unchanged = project_log_stream(
            "abcdefgh\n",
            1,
            2,
            false,
            Some(10),
            JobLogSelectionMode::Delta {
                cursor: completed.next_line as u64,
            },
            false,
        );
        assert_eq!(unchanged.text, "");
        assert_eq!(unchanged.next_line, 2);
    }

    #[test]
    fn partial_line_cursors_remain_independent_between_streams() {
        let stdout = project_log_stream(
            "stdout partial",
            1,
            2,
            false,
            Some(10),
            JobLogSelectionMode::Delta { cursor: 1 },
            false,
        );
        let stderr = project_log_stream(
            "stderr complete\n",
            1,
            2,
            false,
            Some(10),
            JobLogSelectionMode::Delta { cursor: 2 },
            false,
        );
        assert_eq!(stdout.text, "stdout partial");
        assert_eq!(stdout.next_line, 1);
        assert_eq!(stderr.text, "");
        assert_eq!(stderr.next_line, 2);
        assert_eq!(
            combined_delta_status(JobLogSelectionMode::Delta { cursor: 1 }, &stdout, &stderr),
            JobLogDeltaStatus::Delta
        );
    }

    #[test]
    fn retention_gap_reset_reports_truncation_even_when_all_retained_lines_fit() {
        let reset = project_log_stream(
            "l8\nl9\nl10\n",
            8,
            11,
            true,
            Some(10),
            JobLogSelectionMode::Delta { cursor: 3 },
            false,
        );
        assert_eq!(reset.text, "l8\nl9\nl10");
        assert!(reset.delta_reset);
        assert!(reset.truncated);
        assert_eq!(reset.first_retained_line, 8);
        assert_eq!(reset.returned_lines, 3);
    }
}
