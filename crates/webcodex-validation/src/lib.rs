//! Canonical WebCodex validation-domain ownership.
//!
//! This crate owns project-aware read-only validation planning, structured
//! validation adapters, and Workflow Session ledger-to-evidence semantics. It
//! never authorizes callers, starts Jobs, executes commands, or mutates a
//! Workflow Session store.
//!
//! Recipes and execution adapters are available by default. Server-side ledger
//! projection requires `session-evidence`; Runner builds do not need that graph.

mod adapters;
#[cfg(any(feature = "session-evidence", test))]
mod evidence;
mod recipe;

#[cfg(test)]
mod evidence_tests;
#[cfg(test)]
mod profile_tests;
#[cfg(test)]
mod recipe_tests;

pub use adapters::{
    execution_purpose_for_validation_kind, project_validation_operation,
    validation_adapter_for_recipe, validation_adapter_for_tool,
    validation_evidence_profile_for_recipe, validation_evidence_profile_for_tool,
    CargoCheckOptions, CargoReadOnlyValidationOperation, CargoTestOptions, GoCheckOptions,
    GoReadOnlyValidationOperation, GoTestOptions, PythonTestOptions, ReadOnlyValidationOperation,
    ReadOnlyValidationPlan, ValidationAdapter, ValidationCommandOptions,
    ValidationCompatibilityProfile, ValidationEvidenceProfile, ValidationFailureEvidence,
};
#[cfg(any(feature = "session-evidence", test))]
pub use evidence::{
    current_validation_evidence_for_session, event_is_job_acceptance_only,
    event_observes_validation_activity, extract_validation_events, skipped_validation_summary,
    validation_kind_for_tool, validation_summary_for_session_events,
    validation_summary_from_events, CurrentValidationEvidenceProjection, ValidationEvent,
};
pub use recipe::{
    detect_validation_recipe, resolve_node_native_project_check, resolve_node_native_project_test,
    resolve_project_validation_recipe, resolve_validation_recipe,
    resolve_validation_recipe_with_packages, resolve_validation_recipe_with_project_policy,
    validate_node_project_markers, RecipeError, RecipeId, ResolvedValidationRecipe, SemanticCheck,
};
pub use webcodex_core::cargo_test_count::{
    parse_cargo_test_run_metadata, CargoTestRunMetadata, CargoTestRunMetadataAccumulator,
};
