//! Stable validation identity parsing used across audit and Workflow Session layers.

use sha2::{Digest, Sha256};

const STRUCTURED_VALIDATION_TARGET_PREFIX: &str = "target:";
const VALIDATION_IDENTITY_HEX_LEN: usize = 24;
const GENERIC_VALIDATION_IDENTITY_PREFIX: &str = "command:";
const ASSERTION_VALIDATION_IDENTITY_PREFIX: &str = "assertion:";

pub fn is_structured_validation_target_identity(value: &str) -> bool {
    let Some(hex) = value.strip_prefix(STRUCTURED_VALIDATION_TARGET_PREFIX) else {
        return false;
    };
    hex.len() == VALIDATION_IDENTITY_HEX_LEN && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn is_validation_execution_identity(value: &str) -> bool {
    if is_structured_validation_target_identity(value) {
        return true;
    }
    [
        GENERIC_VALIDATION_IDENTITY_PREFIX,
        ASSERTION_VALIDATION_IDENTITY_PREFIX,
    ]
    .into_iter()
    .any(|prefix| {
        value.strip_prefix(prefix).is_some_and(|suffix| {
            suffix.len() == VALIDATION_IDENTITY_HEX_LEN
                && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    })
}

pub fn assertion_validation_identity(assertion_name: &str) -> String {
    let assertion_name = assertion_name.trim();
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-validation-assertion-v1\0");
    hasher.update((assertion_name.len() as u64).to_le_bytes());
    hasher.update(assertion_name.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!(
        "{ASSERTION_VALIDATION_IDENTITY_PREFIX}{}",
        &digest[..VALIDATION_IDENTITY_HEX_LEN]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_identity_shapes_are_stable() {
        let assertion = assertion_validation_identity("cargo check");
        assert!(assertion.starts_with("assertion:"));
        assert!(is_validation_execution_identity(&assertion));
        assert!(is_structured_validation_target_identity(
            "target:0123456789abcdef01234567"
        ));
        assert!(!is_validation_execution_identity("target:short"));
    }
}
