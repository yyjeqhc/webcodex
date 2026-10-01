//! Bounded, observed file evidence for general task closeout and result cards.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_TASK_OUTPUTS: usize = 16;
pub const MAX_TASK_OUTPUT_PATH_BYTES: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutputStatus {
    Verified,
    Missing,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskOutput {
    pub path: String,
    pub status: TaskOutputStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskOutputs {
    pub items: Vec<TaskOutput>,
    pub verified_count: usize,
    pub missing_count: usize,
    pub unavailable_count: usize,
    pub observed_at: i64,
}

pub fn valid_task_output_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_TASK_OUTPUT_PATH_BYTES
        && !path.starts_with(['/', '\\'])
        && !path.contains(':')
        && !path.chars().any(char::is_control)
        && !path
            .split(['/', '\\'])
            .any(|part| matches!(part, "" | "." | ".."))
        && !crate::sensitive_paths::is_bulk_skipped_path(path)
        && !crate::sensitive_text::secret_like_value(path)
}

impl TaskOutputs {
    /// Revalidate retained evidence before persistence/presentation. No arbitrary
    /// stdout, error, command, URL or model-written report is retained here.
    pub fn valid(&self) -> bool {
        let mut paths = HashSet::new();
        !self.items.is_empty()
            && self.items.len() <= MAX_TASK_OUTPUTS
            && self.observed_at >= 0
            && self.verified_count
                == self
                    .items
                    .iter()
                    .filter(|item| item.status == TaskOutputStatus::Verified)
                    .count()
            && self.missing_count
                == self
                    .items
                    .iter()
                    .filter(|item| item.status == TaskOutputStatus::Missing)
                    .count()
            && self.unavailable_count
                == self
                    .items
                    .iter()
                    .filter(|item| item.status == TaskOutputStatus::Unavailable)
                    .count()
            && self.items.iter().all(|item| {
                valid_task_output_path(&item.path)
                    && paths.insert(item.path.as_str())
                    && if item.status == TaskOutputStatus::Verified {
                        item.file_bytes.is_some()
                            && item.sha256.as_ref().is_some_and(|hash| {
                                hash.len() == 64
                                    && hash.bytes().all(|byte| {
                                        byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                                    })
                            })
                            && item.mime_type.as_ref().is_some_and(|mime| {
                                !mime.is_empty()
                                    && mime.len() <= 128
                                    && mime.bytes().all(|byte| byte.is_ascii_graphic())
                            })
                    } else {
                        item.file_bytes.is_none()
                            && item.sha256.is_none()
                            && item.mime_type.is_none()
                    }
            })
    }
}
