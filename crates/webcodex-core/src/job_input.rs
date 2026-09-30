//! Bounded input to an existing interactive Job; never a new execution identity.
use serde::{Deserialize, Serialize};

pub const CAPABILITY: &str = crate::runner_protocol::RUNNER_CAPABILITY_JOB_PROCESS_INPUT;
pub const MAX_BYTES: usize = 65_536;
pub const MAX_WRITES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobInputRequest {
    pub job_id: String,
    /// The instance which owns this exact process. A replacement cannot receive it.
    pub runner_instance_id: String,
    pub project: String,
    pub input_id: String,
    pub data: String,
    #[serde(default)]
    pub close: bool,
}

impl JobInputRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.job_id.is_empty()
            || self.job_id.len() > 160
            || self.runner_instance_id.is_empty()
            || self.runner_instance_id.len() > 128
            || self.project.is_empty()
            || self.project.len() > 512
        {
            return Err("job_input_target_invalid".into());
        }
        validate_input(&self.input_id, &self.data, self.close)
    }
}

pub fn validate_input(id: &str, data: &str, close: bool) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
    {
        return Err("job_input_id_invalid".into());
    }
    if data.len() > MAX_BYTES {
        return Err("job_input_too_large".into());
    }
    if data.is_empty() && !close {
        return Err("job_input_empty".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputWriteState {
    Pending,
    Written,
    Closed,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobInputReceipt {
    pub input_id: String,
    pub state: InputWriteState,
    /// Bytes accepted by the OS pipe, not proof of consumption by the program.
    pub bytes_written: usize,
    pub stdin_closed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_capability_is_not_a_generation_two_baseline_requirement() {
        assert!(!crate::runner_protocol::RunnerCapabilityId::JobProcessInput.is_v2_baseline());
    }

    #[test]
    fn typed_input_wire_preserves_literal_bytes_and_rejects_mixed_start_fields() {
        use crate::runner_operation::{RunnerInvocationMetadata, RunnerOperation};
        use crate::runner_protocol::RunnerRequest;
        let input = JobInputRequest {
            job_id: "exact-job".into(),
            runner_instance_id: "exact-instance".into(),
            project: "agent:runner:project".into(),
            input_id: "write-1".into(),
            data: "hello\0世界\n".into(),
            close: true,
        };
        let operation = RunnerOperation::JobInput(input.clone());
        let wire = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: "exact-request".into(),
                client_id: "runner".into(),
                requested_by: "test".into(),
                created_at: 1,
            },
            operation,
        )
        .unwrap();
        let decoded: RunnerRequest =
            serde_json::from_slice(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert!(
            matches!(decoded.decode_operation().unwrap(),RunnerOperation::JobInput(value) if value==input)
        );
        for field in ["command", "cwd", "stdin", "job_id"] {
            let mut value = serde_json::to_value(&wire).unwrap();
            value[field] = serde_json::json!("mixed-start");
            let invalid: RunnerRequest = serde_json::from_value(value).unwrap();
            assert!(invalid.decode_operation().is_err(), "{field}");
        }
        let mut invalid = wire;
        invalid.content = Some(" ".repeat(MAX_BYTES * 6 + 2049));
        assert!(invalid.decode_operation().is_err());
    }

    #[test]
    fn bounded_write_requires_stable_identity_and_explicit_effect() {
        assert!(validate_input("input-1", "hello\n", false).is_ok());
        assert!(validate_input("eof", "", true).is_ok());
        assert!(validate_input("empty", "", false).is_err());
        assert!(validate_input("bad id", "x", false).is_err());
        assert!(validate_input("id", &"x".repeat(MAX_BYTES + 1), false).is_err());
    }
}
