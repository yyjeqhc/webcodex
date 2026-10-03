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
