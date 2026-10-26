//! Pure, bundled guidance composition. Selection is request-local model advice,
//! never admission, a Session mode, or a mutable runtime service.

mod contributors;

use super::tool_inputs::CodingGuidanceProfile;
use serde::{Serialize, Serializer};
use serde_json::{json, Value};

pub(crate) const CODING_GUIDANCE_CONTEXT_KEY: &str = "webcodex.coding";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuidanceTarget {
    Core,
    Coding,
    IndependentReview,
    Strategy(CodingGuidanceProfile),
}

struct GuidanceContributor {
    target: GuidanceTarget,
    items: &'static [&'static str],
}

struct GuidanceRegistry {
    contributors: &'static [&'static GuidanceContributor],
}

impl GuidanceRegistry {
    fn project(&self, target: GuidanceTarget) -> GuidanceProjection<'_> {
        GuidanceProjection {
            registry: self,
            target,
        }
    }
}

/// Serialize the selected contributors directly, without an intermediate Vec,
/// sorting, deduplication, or any observable reordering of guidance.
pub(super) struct GuidanceProjection<'a> {
    registry: &'a GuidanceRegistry,
    target: GuidanceTarget,
}

impl Serialize for GuidanceProjection<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(
            self.registry
                .contributors
                .iter()
                .filter(|contributor| contributor.target == self.target)
                .flat_map(|contributor| contributor.items.iter().copied()),
        )
    }
}

static BUILTIN_GUIDANCE: GuidanceRegistry = GuidanceRegistry {
    contributors: &[
        &contributors::CORE_SAFETY,
        &contributors::CORE_WORKFLOW,
        &contributors::CORE_VALIDATION,
        &contributors::CORE_JOBS,
        &contributors::CODING_IMPLEMENTATION,
        &contributors::INDEPENDENT_REVIEW,
        &contributors::DIRECT,
        &contributors::HOST_BATCHING,
        &contributors::HOST_CONTINUATION,
        #[cfg(feature = "experimental-code-mode")]
        &contributors::CODE_MODE,
    ],
};

pub(super) fn core_guidance() -> GuidanceProjection<'static> {
    BUILTIN_GUIDANCE.project(GuidanceTarget::Core)
}

pub(super) fn tool_strategy_guidance(
    profile: CodingGuidanceProfile,
) -> GuidanceProjection<'static> {
    BUILTIN_GUIDANCE.project(GuidanceTarget::Strategy(profile))
}

/// Optional task guidance, selected explicitly through the existing context
/// material mechanism. Neither role changes tool strategy or execution authority.
pub(crate) fn coding_guidance_projection() -> Value {
    json!({
        "authority": "model_guidance_only",
        "role_selection": "Use coding for implementation and independent_review only for an explicitly requested code review. Roles never grant authority; project instructions own language, formatting, testing and Git conventions.",
        "roles": {
            "coding": {
                "purpose": "Implement code within the authorized task and project conventions.",
                "guidance": BUILTIN_GUIDANCE.project(GuidanceTarget::Coding),
            },
            "independent_review": {
                "purpose": "Review code independently within task scope.",
                "guidance": BUILTIN_GUIDANCE.project(GuidanceTarget::IndependentReview),
            },
        },
    })
}

#[cfg(test)]
#[path = "tests/guidance_registry.rs"]
mod tests;
