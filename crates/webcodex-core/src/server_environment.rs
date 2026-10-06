//! Pure parsing shared by Server startup and read-only configuration projections.
//! This deliberately preserves the established runtime grammar and does not
//! load files, expand variables, or modify process environment.
pub fn parse_env_file_line(line: &str) -> Option<Result<(String, String), String>> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    let Some((key, value)) = line.split_once('=') else {
        return Some(Err("missing '='".to_string()));
    };
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Some(Err(format!("invalid env key '{}'", key)));
    }
    let value = value.trim();
    let value = if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    };
    Some(Ok((key.to_string(), value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_runtime_grammar_without_expansion_or_comment_reinterpretation() {
        assert_eq!(
            parse_env_file_line("export WEBCODEX_DATA='relative data'"),
            Some(Ok(("WEBCODEX_DATA".into(), "relative data".into())))
        );
        assert_eq!(
            parse_env_file_line("WEBCODEX_DATA=$HOME/data # literal"),
            Some(Ok(("WEBCODEX_DATA".into(), "$HOME/data # literal".into())))
        );
        assert!(parse_env_file_line("lowercase=value").unwrap().is_err());
        assert_eq!(parse_env_file_line("# comment"), None);
        assert_eq!(
            parse_env_file_line("WEBCODEX_DATA='"),
            Some(Ok(("WEBCODEX_DATA".into(), "'".into())))
        );
    }
}
