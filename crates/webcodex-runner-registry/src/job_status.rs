/// Canonical Runner-registry definition of a broadly active Agent Job.
///
/// `stop_requested` remains active until authoritative terminal truth arrives.
pub fn job_status_is_active(status: &str) -> bool {
    matches!(
        status,
        "running" | "queued" | "started" | "agent_queued" | "stop_requested" | "recovering"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_status_vocabulary_is_stable() {
        for status in [
            "running",
            "queued",
            "started",
            "agent_queued",
            "stop_requested",
            "recovering",
        ] {
            assert!(job_status_is_active(status), "{status}");
        }
        for status in [
            "completed",
            "failed",
            "stopped",
            "lost",
            "timeout",
            "timed_out",
            "cancelled",
        ] {
            assert!(!job_status_is_active(status), "{status}");
        }
    }
}
