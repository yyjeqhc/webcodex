use super::*;
use serde_json::{json, to_value};

#[test]
fn guidance_registry_composes_in_registration_order_without_profile_leakage() {
    static FIRST: GuidanceContributor = GuidanceContributor {
        target: GuidanceTarget::Core,
        items: &["core first"],
    };
    static DIRECT: GuidanceContributor = GuidanceContributor {
        target: GuidanceTarget::Strategy(CodingGuidanceProfile::Direct),
        items: &["direct only"],
    };
    static LAST: GuidanceContributor = GuidanceContributor {
        target: GuidanceTarget::Core,
        items: &["core second", "core third"],
    };
    static REGISTRY: GuidanceRegistry = GuidanceRegistry {
        contributors: &[&FIRST, &DIRECT, &LAST],
    };
    assert_eq!(
        to_value(REGISTRY.project(GuidanceTarget::Core)).unwrap(),
        json!(["core first", "core second", "core third"])
    );
    assert_eq!(
        to_value(REGISTRY.project(GuidanceTarget::Strategy(CodingGuidanceProfile::Direct)))
            .unwrap(),
        json!(["direct only"])
    );
    assert_eq!(
        to_value(REGISTRY.project(GuidanceTarget::Strategy(
            CodingGuidanceProfile::HostCodeMode
        )))
        .unwrap(),
        json!([])
    );
}

#[test]
fn guidance_registry_bundles_are_complete_unique_and_deterministic() {
    let targets = [
        GuidanceTarget::Core,
        GuidanceTarget::Coding,
        GuidanceTarget::IndependentReview,
        GuidanceTarget::Strategy(CodingGuidanceProfile::Direct),
        GuidanceTarget::Strategy(CodingGuidanceProfile::HostCodeMode),
        #[cfg(feature = "experimental-code-mode")]
        GuidanceTarget::Strategy(CodingGuidanceProfile::CodeMode),
    ];
    for target in targets {
        let first = to_value(BUILTIN_GUIDANCE.project(target)).unwrap();
        assert_eq!(first, to_value(BUILTIN_GUIDANCE.project(target)).unwrap());
        let items = first.as_array().unwrap();
        assert!(!items.is_empty());
        let mut seen = std::collections::HashSet::new();
        for item in items {
            let text = item.as_str().unwrap();
            assert!(!text.is_empty());
            assert!(seen.insert(text), "duplicate bundled guidance: {text}");
        }
    }
}

#[test]
fn coding_and_review_contributors_are_absent_from_shared_guidance() {
    let core = to_value(core_guidance()).unwrap();
    for contributor in [
        &contributors::CODING_IMPLEMENTATION,
        &contributors::INDEPENDENT_REVIEW,
    ] {
        assert_ne!(contributor.target, GuidanceTarget::Core);
        let selected = to_value(BUILTIN_GUIDANCE.project(contributor.target)).unwrap();
        for item in contributor.items {
            assert!(selected.as_array().unwrap().contains(&json!(item)));
            assert!(!core.as_array().unwrap().contains(&json!(item)));
        }
    }
}

#[test]
fn coding_chapter_contains_only_task_roles_and_no_strategy_or_authority_state() {
    let chapter = coding_guidance_projection();
    let keys: std::collections::BTreeSet<_> = chapter
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from(["authority", "role_selection", "roles"])
    );
    assert_eq!(chapter["authority"], "model_guidance_only");
    let roles = chapter["roles"].as_object().unwrap();
    assert_eq!(roles.len(), 2);
    for (name, target) in [
        ("coding", GuidanceTarget::Coding),
        ("independent_review", GuidanceTarget::IndependentReview),
    ] {
        assert_eq!(
            roles[name]["guidance"],
            to_value(BUILTIN_GUIDANCE.project(target)).unwrap()
        );
        assert_eq!(roles[name].as_object().unwrap().len(), 2);
        assert!(!roles[name]["purpose"].as_str().unwrap().is_empty());
    }
}
