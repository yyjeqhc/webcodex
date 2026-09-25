//! Typed, model-facing short-reference syntax.
//!
//! Short refs are selectors only. Persistence, authority, lifecycle and
//! canonical identity remain owned by the target domain.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelReferenceKind {
    Project,
    Session,
}

impl ModelReferenceKind {
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Project => "~p",
            Self::Session => "~s",
        }
    }
}

pub fn parse_model_reference(raw: &str, kind: ModelReferenceKind) -> Option<Result<u64, ()>> {
    let digits = raw.strip_prefix(kind.prefix())?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(Err(()));
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return Some(Err(()));
    }
    Some(
        digits
            .parse::<u64>()
            .ok()
            .filter(|index| *index > 0)
            .ok_or(()),
    )
}

pub fn format_model_reference(kind: ModelReferenceKind, ref_index: u64) -> String {
    debug_assert!(ref_index > 0);
    format!("{}{ref_index}", kind.prefix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_refs_have_disjoint_strict_syntax() {
        assert_eq!(
            parse_model_reference("~p12", ModelReferenceKind::Project),
            Some(Ok(12))
        );
        assert_eq!(
            parse_model_reference("~s7", ModelReferenceKind::Session),
            Some(Ok(7))
        );
        assert_eq!(
            parse_model_reference("~s7", ModelReferenceKind::Project),
            None
        );
        for invalid in ["~s", "~s0", "~s01", "~s-1", "~s1x"] {
            assert_eq!(
                parse_model_reference(invalid, ModelReferenceKind::Session),
                Some(Err(())),
                "{invalid}"
            );
        }
        assert_eq!(
            format_model_reference(ModelReferenceKind::Project, 3),
            "~p3"
        );
        assert_eq!(
            format_model_reference(ModelReferenceKind::Session, 3),
            "~s3"
        );
    }
}
