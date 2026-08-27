use serde::{Deserialize, Serialize};

pub const MAX_SKILL_NAME_CHARS: usize = 96;
pub const MAX_SKILL_DESCRIPTION_CHARS: usize = 512;
pub const MAX_SKILL_DEFINITION_BYTES: usize = 64 * 1024;
pub const MAX_SKILL_FRONTMATTER_LINES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

/// Parse the canonical bounded Agent Skill frontmatter used by both Control
/// project discovery and the Runner operator-store installer. Only explicit
/// scalar `name` and `description` metadata are accepted; bodies are never
/// searched for inferred metadata.
pub fn parse_skill_metadata(text: &str) -> Result<SkillMetadata, &'static str> {
    if text.len() > MAX_SKILL_DEFINITION_BYTES {
        return Err("skill_definition_too_large");
    }
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err("skill_frontmatter_missing");
    }
    let mut name = None::<String>;
    let mut description = None::<String>;
    let mut closed = false;
    for line in lines.take(MAX_SKILL_FRONTMATTER_LINES) {
        let trimmed = line.trim();
        if trimmed == "---" {
            closed = true;
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if !matches!(key, "name" | "description") {
            continue;
        }
        let value = parse_frontmatter_scalar(raw_value.trim())?;
        match key {
            "name" if name.is_none() => name = Some(value),
            "description" if description.is_none() => description = Some(value),
            _ => return Err("skill_frontmatter_duplicate_field"),
        }
    }
    if !closed {
        return Err("skill_frontmatter_unclosed");
    }
    let name = name.ok_or("skill_name_missing")?;
    let description = description.ok_or("skill_description_missing")?;
    if name.is_empty()
        || name.chars().count() > MAX_SKILL_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err("skill_name_invalid");
    }
    if description.is_empty()
        || description.chars().count() > MAX_SKILL_DESCRIPTION_CHARS
        || description.chars().any(char::is_control)
    {
        return Err("skill_description_invalid");
    }
    Ok(SkillMetadata { name, description })
}

fn parse_frontmatter_scalar(raw: &str) -> Result<String, &'static str> {
    if raw.is_empty()
        || matches!(
            raw.as_bytes().first(),
            Some(b'|' | b'>' | b'[' | b'{' | b'&' | b'*' | b'!')
        )
    {
        return Err("skill_frontmatter_scalar_invalid");
    }
    let value = if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        let inner = &raw[1..raw.len() - 1];
        if inner.contains('"') || inner.contains('\\') {
            return Err("skill_frontmatter_scalar_invalid");
        }
        inner
    } else if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        let inner = &raw[1..raw.len() - 1];
        if inner.contains('\'') {
            return Err("skill_frontmatter_scalar_invalid");
        }
        inner
    } else {
        raw.split(" #").next().unwrap_or(raw).trim()
    };
    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_parser_requires_explicit_simple_metadata() {
        let parsed = parse_skill_metadata(
            "---\nname: demo\ndescription: 'Use demo safely'\nlicense: MIT\n---\nPRIVATE_BODY",
        )
        .unwrap();
        assert_eq!(parsed.name, "demo");
        assert_eq!(parsed.description, "Use demo safely");
        for invalid in [
            "# no frontmatter\nname: guessed",
            "---\ndescription: only desc\n---\nname in body",
            "---\nname: x\n---\nbody description",
            "---\nname: x\ndescription: |\n  block\n---",
        ] {
            assert!(parse_skill_metadata(invalid).is_err(), "{invalid}");
        }
    }
}
