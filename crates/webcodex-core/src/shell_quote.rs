//! Deterministic POSIX shell single-argument quoting.
//!
//! This helper is intentionally narrow: it preserves one string as one shell
//! word when legacy command-string adapters still need shell text. It does not
//! parse commands or provide an argv abstraction.

/// Quote one value as a POSIX shell word using single quotes.
///
/// Embedded single quotes use the standard close-quote, escaped-quote,
/// reopen-quote sequence. The exact representation is part of existing
/// WebCodex command-string compatibility.
pub fn shell_escape_simple(value: &str) -> String {
    let mut out = String::from("'");
    for character in value.chars() {
        if character == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(character);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::shell_escape_simple;

    #[test]
    fn shell_escape_simple_preserves_existing_single_quote_contract() {
        assert_eq!(shell_escape_simple(""), "''");
        assert_eq!(shell_escape_simple("plain value"), "'plain value'");
        assert_eq!(shell_escape_simple("a'b"), "'a'\\''b'");
    }
}
