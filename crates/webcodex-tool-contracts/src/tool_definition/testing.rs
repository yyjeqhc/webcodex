use super::RunnerCapabilityRequirement::{OwnerOnly, Shell};
use super::ToolVisibility::ModelVisible;
use super::{
    adaptive_runtime_direct, captures_validation_output, model_spec, ToolDeclaration,
    ToolDefinition, TOOL_CATEGORY_VALIDATION,
};
use crate::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::JobRun, JOB_RUN, TOOL_PROVIDER_RUNNER,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    captures_validation_output(model_spec(
        ToolDeclaration {
            name: "cargo_fmt",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL
                .execution(super::ToolAuditExecutionPolicy::TEXT),
            visibility: ModelVisible,
            category: TOOL_CATEGORY_VALIDATION,
            runner_capability: Some(Shell),
            provider_id: TOOL_PROVIDER_RUNNER,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Execute,
                risk: JobRun,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(JOB_RUN),
            requires_project: true,
            path_hint: NoPath,
            destructive: true,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE.validation_identity(super::ToolValidationIdentityKind::CargoFmt),
        }.define(),
        "Use check=false (default) for intentional final formatting after relevant Rust source stabilizes: precheck first, mutate only for a proven rustfmt diff, and use changed/state_changed instead of reproducing rustfmt diffs with edit tools. Do not use cargo_fmt as a per-edit ritual. Use check=true for read-only formatting validation; only check mode may hand off the same execution as a Job. If check mode returns execution_state=pending, keep its continuation as fallback, continue independent work, and let ordinary same-Window/Project/Session results surface sparse terminal attention; do not poll. Observe only for details/recovery. If covered Rust source changes, check=true evidence is stale. For final formatting proof, freeze covered source or rerun after any required source edit. Server timing policy controls same-execution Job handoff grace.",
    )
    .with_execution(super::ToolExecutionContract::new(
        super::ToolExecutionForm::StructuredValidation,
        super::ToolExecutionLifetime::Runner,
        super::ToolExecutionStart::SyncFirst,
        super::ToolExecutionContinuation::ObserveJobs,
    ))),
    captures_validation_output(model_spec(
        ToolDeclaration {
            name: "cargo_check",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL
                .execution(super::ToolAuditExecutionPolicy::TEXT),
            visibility: ModelVisible,
            category: TOOL_CATEGORY_VALIDATION,
            runner_capability: Some(Shell),
            provider_id: TOOL_PROVIDER_RUNNER,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Execute,
                risk: JobRun,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(JOB_RUN),
            requires_project: true,
            path_hint: NoPath,
            destructive: false,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE.validation_identity(super::ToolValidationIdentityKind::CargoCheck),
        }.define()
            .with_composition_policy(super::ToolCompositionPolicy::Sequential)
            .with_host_orchestration_hint(
                super::ToolHostOrchestrationHint::sequential()
                    .with_native_batch_field("packages"),
            ),
            "Check Rust projects. Structured cargo check (default --all-targets) for common supported validation with parsed diagnostics, validation identity, and same execution Job handoff. Use package for one workspace package or packages for a known set; packages runs one Cargo invocation with repeated -p after deterministic sort/dedup. If pending, keep its continuation and continue independent work; same-scope results may surface terminal attention. Observe only for logs/details/recovery; do not poll. Development validation can overlap independent work, but edits to covered source make it stale. For final evidence freeze covered source; if it changes, rerun the appropriate check. Server timing policy controls same-execution Job handoff grace and never changes timeout or retry semantics. Complete synchronous success omits zero counts and empty parser bookkeeping; positive warnings and diagnostics remain. source_state never certifies current workspace source.",
        ).with_discovery_keywords(&["rust", "compile", "typecheck", "compilation", "check", "编译", "检查", "类型"])
        .with_execution(super::ToolExecutionContract::new(
            super::ToolExecutionForm::StructuredValidation,
            super::ToolExecutionLifetime::Runner,
            super::ToolExecutionStart::SyncFirst,
            super::ToolExecutionContinuation::ObserveJobs,
        ))),
    captures_validation_output(model_spec(
        ToolDeclaration {
            name: "cargo_test",
            audit: super::ToolAuditPolicy::TYPED_CANONICAL
                .execution(super::ToolAuditExecutionPolicy::TEST_ASSERTIONS),
            visibility: ModelVisible,
            category: TOOL_CATEGORY_VALIDATION,
            runner_capability: Some(Shell),
            provider_id: TOOL_PROVIDER_RUNNER,
            semantic: super::ToolSemanticContract {
                effect: super::ToolEffect::Execute,
                risk: JobRun,
                approval: super::ToolApprovalPolicy::Standard,
                idempotency: super::ToolIdempotency::NonIdempotent,
            },
            required_scope: Some(JOB_RUN),
            requires_project: true,
            path_hint: NoPath,
            destructive: false,
            shell_like: false,
            session_evidence: super::ToolSessionEvidencePolicy::NONE.validation_identity(super::ToolValidationIdentityKind::CargoTest),
        }.define()
            .with_composition_policy(super::ToolCompositionPolicy::Sequential)
            .with_host_orchestration_hint(super::ToolHostOrchestrationHint::sequential()),
            "Run Cargo tests. Structured cargo test for common supported validation with bounded output, executed-test evidence, min_tests/require_tests, validation identity, and same execution Job handoff. lib=true selects Cargo --lib; filter is one Rust substring, not flags such as --exact or --nocapture. Exit 0 alone is not test proof: default requires positive counts; require_tests=false accepts zero only without min_tests; require_tests=true/min_tests enforce a proven minimum. no_run=true is compile-only. Sparse synchronous proof retains tests_run_count and explicit minimum_tests, require_tests=false for accepted zero, or no_run=true for compile-only. Rich success may lack test proof. source_state never certifies current workspace source. If pending, keep its continuation and continue independent work; same-scope results may surface terminal attention. Observe only for logs/details/recovery; do not poll. Edits to covered source stale evidence. For final evidence freeze covered source; if it changes, rerun the appropriate test.",
        ).with_discovery_keywords(&["rust", "unit", "integration", "tests", "test", "suite", "测试", "单元", "集成", "用例"])
        .with_execution(super::ToolExecutionContract::new(
            super::ToolExecutionForm::StructuredValidation,
            super::ToolExecutionLifetime::Runner,
            super::ToolExecutionStart::SyncFirst,
            super::ToolExecutionContinuation::ObserveJobs,
        ))),
    adaptive_runtime_direct(
        captures_validation_output(model_spec(
            ToolDeclaration {
                name: "project_validate",
                audit: super::ToolAuditPolicy::TYPED_CANONICAL
                    .execution(super::ToolAuditExecutionPolicy::TEST_ASSERTIONS),
                visibility: ModelVisible,
                category: TOOL_CATEGORY_VALIDATION,
                runner_capability: Some(OwnerOnly),
                provider_id: TOOL_PROVIDER_RUNNER,
                semantic: super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                required_scope: Some(JOB_RUN),
                requires_project: true,
                path_hint: NoPath,
                destructive: false,
                shell_like: false,
                session_evidence: super::ToolSessionEvidencePolicy::NONE.validation_identity(super::ToolValidationIdentityKind::Project),
            }.define()
            .with_composition_policy(super::ToolCompositionPolicy::Sequential)
            .with_host_orchestration_hint(super::ToolHostOrchestrationHint::sequential()),
            "Project validation. Project scripts may modify files or access network. Rust: format_check/check/lint/test; Go: check/test. Rust lint: cargo clippy --all-targets -- -D clippy::correctness; cargo_clippy evidence, project_validation_rust_clippy_v1. Clippy missing: no install/fallback. Node >=22.3: check>typecheck>lint; lint selects only scripts.lint (check identity); node --run without pre/post. Node TAP test only for exact scripts.test=node --test or node --test --test-reporter=tap. Vitest/Jest: fixed installed CLI, project_validation_node_json_v1 JSON counts; other scripts unproven. Scope: packages 1..8/all_packages=true; Rust Cargo workspace proven, Go single module. Rust exclude_packages (all_packages): 1..8 exact Cargo members; unknown/all excluded reject; project_workspace_exclusions_v1 planning/admission; not Go/Node/Python/format. test.filter: Rust substring, Go -run, pytest -k; not for Node. require_tests/min_tests need proven counts. Python/Ruff preinstalled; Ruff needs pyproject.toml [tool.ruff] target-version, no extend/fixes/cache/install. locked: Cargo --locked/Go -mod=readonly; not Python/Node/format. dependency_resolution_policy=offline: Rust check/lint/test Cargo --offline; Clippy requires project_validation_rust_clippy_offline_v1; cache miss fails, no online fallback. build scripts/tests may use network; not OS sandbox. Go GO111MODULE=on/GOWORK=off. No arbitrary executable, argv or shell. Same Runner Job; recheck provenance at queue; observe pending, never retry.",    ).with_discovery_keywords(&["project", "validation", "build", "tests", "lint", "verify", "项目", "验证", "构建", "测试", "检查"])
        .with_execution(super::ToolExecutionContract::new(
            super::ToolExecutionForm::StructuredValidation,
            super::ToolExecutionLifetime::Runner,
            super::ToolExecutionStart::SyncFirst,
            super::ToolExecutionContinuation::ObserveJobs,
        ))),
        95,
        super::ToolDirectReason::CoreWorkflow,
    ),
    captures_validation_output(model_spec(
            ToolDeclaration {
                name: "go_test",
                audit: super::ToolAuditPolicy::TYPED_CANONICAL
                    .execution(super::ToolAuditExecutionPolicy::TEST_COUNTS),
                visibility: ModelVisible,
                category: TOOL_CATEGORY_VALIDATION,
                runner_capability: Some(OwnerOnly),
                provider_id: TOOL_PROVIDER_RUNNER,
                semantic: super::ToolSemanticContract {
                    effect: super::ToolEffect::Execute,
                    risk: JobRun,
                    approval: super::ToolApprovalPolicy::Standard,
                    idempotency: super::ToolIdempotency::NonIdempotent,
                },
                required_scope: Some(JOB_RUN),
                requires_project: true,
                path_hint: NoPath,
                destructive: false,
                shell_like: false,
                session_evidence: super::ToolSessionEvidencePolicy::NONE.validation_identity(super::ToolValidationIdentityKind::GoTest),
            }.define(),
            "Structured option for common supported go test -json (default ./...) when bounded package scopes, Go JSON test-count evidence, validation identity, or same-execution pending handoff are useful. If execution_state=pending, keep its continuation as fallback and continue independent work; ordinary same-Window/Project/Session results may surface sparse terminal validation truth. Observe only when logs/details/recovery are needed. Validation intent is intrinsic to this validator and Runtime-derived; do not pass generic execution purpose. Requires Runner Go JSON validation support; Server timing policy controls only synchronous grace before the same execution is returned pending and never changes total timeout or retry semantics. Sparse synchronous success retains tests_run_count only when complete evidence proves all counted tests passed; skipped or incomplete evidence stays rich. source_state independently describes source coverage.",
    ).with_discovery_keywords(&["golang", "unit", "integration", "tests", "test", "suite", "测试", "单元", "集成", "用例"])
    .with_execution(super::ToolExecutionContract::new(
        super::ToolExecutionForm::StructuredValidation,
        super::ToolExecutionLifetime::Runner,
        super::ToolExecutionStart::SyncFirst,
        super::ToolExecutionContinuation::ObserveJobs,
    ))),
];
