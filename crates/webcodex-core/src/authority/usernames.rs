/// Validate a username against the shared account rules: non-empty, bounded length,
/// only lowercase ASCII letters, digits, `_`, and `-`. No slash, no `..`, no
/// whitespace, no uppercase. Returns the trimmed username on success.
pub fn validate_username(username: &str) -> Result<String, String> {
    let username = username.trim();
    if username.is_empty() {
        return Err("username cannot be empty".to_string());
    }
    if username.chars().count() > 64 {
        return Err("username is too long; maximum is 64 characters".to_string());
    }
    if username.contains("..") {
        return Err("username cannot contain '..'".to_string());
    }
    for ch in username.chars() {
        let ok = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if !ok {
            return Err(format!(
                "username contains an invalid character '{}'; only lowercase letters, digits, '_' and '-' are allowed",
                ch
            ));
        }
    }
    Ok(username.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_username_preserves_the_server_contract() {
        for valid in ["alice", "bob_123", "user-name", "123", &"a".repeat(64)] {
            assert_eq!(validate_username(valid).unwrap(), valid);
        }
        assert_eq!(validate_username(" alice ").unwrap(), "alice");
        for invalid in [
            "",
            "Alice",
            "alice.smith",
            "../etc",
            "ali ce",
            "用户",
            &"a".repeat(65),
        ] {
            assert!(validate_username(invalid).is_err(), "{invalid:?}");
        }
    }
}
