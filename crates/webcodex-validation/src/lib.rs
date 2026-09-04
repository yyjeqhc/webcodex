//! Canonical WebCodex validation-domain ownership.
//!
//! This crate owns project-aware read-only validation planning, structured
//! validation adapters, and Workflow Session ledger-to-evidence semantics. It
//! never authorizes callers, starts Jobs, executes commands, or mutates a
//! Workflow Session store.

mod adapters;
mod cargo_test;
mod evidence;
mod recipe;

#[cfg(test)]
mod evidence_tests;
#[cfg(test)]
mod profile_tests;
#[cfg(test)]
mod recipe_tests;

pub use adapters::{
    validation_adapter_for_tool, ValidationAdapter, ValidationCommandOptions,
    ValidationFailureEvidence,
};
pub use cargo_test::{parse_cargo_test_run_metadata, CargoTestRunMetadata};
pub use evidence::{
    current_validation_evidence_for_session, event_is_job_acceptance_only,
    event_observes_validation_activity, extract_validation_events, skipped_validation_summary,
    validation_kind_for_tool, validation_summary_for_session_events,
    validation_summary_from_events, CurrentValidationEvidenceProjection, ValidationEvent,
};
pub use recipe::{
    resolve_validation_recipe, RecipeError, RecipeId, ResolvedValidationRecipe, SemanticCheck,
};
