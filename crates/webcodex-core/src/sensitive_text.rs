//! Shared immutable vocabulary for detecting secret-like command/process text.

/// Stable non-secret token prefixes shared by redaction consumers.
pub const WEBCODEX_SECRET_PREFIXES: &[&str] = &[
    "wc_pat_",
    "wc_agent_",
    "wc_acct_",
    "wc_oat_",
    "wc_ort_",
    "wc_csec_",
    "wc_pair_",
    "wc_boot_",
];

/// Conservative detector used before emitting command/process previews.
pub fn secret_like_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("-----begin")
        || lower.contains("bearer ")
        || lower.contains("api_key")
        || lower.contains("token=")
        || lower.contains("id_rsa")
        || lower.contains("id_ed25519")
        || WEBCODEX_SECRET_PREFIXES
            .iter()
            .any(|prefix| lower.contains(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_shared_secret_detection_vocabulary() {
        for value in [
            "Bearer abc",
            "api_key=value",
            "token=value",
            "-----BEGIN PRIVATE KEY-----",
            "id_rsa",
            "id_ed25519",
            "wc_pat_demo",
            "wc_agent_demo",
            "wc_acct_demo",
            "wc_oat_demo",
            "wc_ort_demo",
            "wc_csec_demo",
            "wc_pair_demo",
            "wc_boot_demo",
        ] {
            assert!(secret_like_value(value), "{value}");
        }
        assert!(!secret_like_value("cargo check -p webcodex"));
    }
}
