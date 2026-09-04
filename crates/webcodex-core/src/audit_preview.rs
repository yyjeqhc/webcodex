//! Bounded, non-replayable presentation summaries shared by runtime audit and Runner activity.

pub const COMMAND_PREVIEW_MAX_CHARS: usize = 120;

pub fn command_preview(command: &str) -> String {
    let first_line = command.lines().next().unwrap_or_default().trim();
    if crate::sensitive_text::secret_like_value(first_line) {
        "[redacted]".to_string()
    } else if first_line.chars().count() <= COMMAND_PREVIEW_MAX_CHARS {
        first_line.to_string()
    } else {
        let preview = first_line
            .chars()
            .take(COMMAND_PREVIEW_MAX_CHARS)
            .collect::<String>();
        format!("{}…", preview)
    }
}

/// Bounded human-readable process summary. Argument boundaries are used only
/// for display; this string is never executable input or a retry source.
pub fn process_preview<'a>(executable: &'a str, args: impl IntoIterator<Item = &'a str>) -> String {
    let mut summary = String::new();
    let mut truncated = false;
    let push = |summary: &mut String, character: char| {
        if summary.chars().count() >= COMMAND_PREVIEW_MAX_CHARS {
            false
        } else {
            summary.push(character);
            true
        }
    };
    for value in std::iter::once(executable).chain(args) {
        if !summary.is_empty() && !push(&mut summary, ' ') {
            truncated = true;
            break;
        }
        let simple = !value.is_empty()
            && value.chars().all(|character| {
                character.is_alphanumeric() || matches!(character, '_' | '-' | '.' | '/' | '\\')
            });
        if !simple && !push(&mut summary, '"') {
            truncated = true;
            break;
        }
        for character in value.chars() {
            let escaped = match character {
                '"' => Some(['\\', '"']),
                '\\' if !simple => Some(['\\', '\\']),
                _ => None,
            };
            if let Some(escaped) = escaped {
                if escaped
                    .into_iter()
                    .any(|character| !push(&mut summary, character))
                {
                    truncated = true;
                    break;
                }
            } else if !push(
                &mut summary,
                if character.is_control() {
                    '�'
                } else {
                    character
                },
            ) {
                truncated = true;
                break;
            }
        }
        if truncated {
            break;
        }
        if !simple && !push(&mut summary, '"') {
            truncated = true;
            break;
        }
    }
    if crate::sensitive_text::secret_like_value(&summary) {
        return "[redacted]".to_string();
    }
    if truncated {
        summary.push('…');
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_bounded_and_secret_safe() {
        assert_eq!(
            command_preview("curl -H 'Authorization: Bearer example' https://example.invalid"),
            "[redacted]"
        );
        assert_eq!(command_preview("cargo test focused"), "cargo test focused");

        let preview = process_preview(
            "git",
            ["status", "two words", "$(literal)", &"x".repeat(200)],
        );
        assert!(preview.starts_with("git status \"two words\" \"$(literal)\""));
        assert!(preview.chars().count() <= COMMAND_PREVIEW_MAX_CHARS + 1);
        assert!(preview.ends_with('…'));
        assert_eq!(
            process_preview("tool", ["Authorization: Bearer example"].into_iter()),
            "[redacted]"
        );
    }
}
