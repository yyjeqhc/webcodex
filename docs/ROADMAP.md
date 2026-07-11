# Roadmap

This roadmap is intentionally short. WebCodex 0.2.x is about making online AI coding safer to try and easier to review, not promising a full IDE or autonomous operations platform.

## 0.2.x: Productize The Online Coding Loop

- Simpler first-run setup.
- ChatGPT MCP and GPT Actions acceptance tasks.
- More fixture dogfood tasks.
- Clearer review and rollback workflow.

## LSP Phase 1: Completed

- Agent-side supervisor.
- Constrained rust-analyzer profile.
- Read-only symbol navigation.
- Startup capability awareness.

## LSP Phase 2 Read-Only MVP: Completed

- Disk-backed full-text document refresh (`didOpen` / `didChange`), without editor-style incremental sync.
- Bounded `document_diagnostics` with explicit freshness and timeout semantics.
- Normalized, bounded hover.
- Workspace-only symbol search with project-relative results.
- ToolRuntime, MCP, generic GPT Action, OAuth, schema, and startup capability synchronization.

## Future LSP Work

- Broader diagnostics fidelity beyond the constrained rust-analyzer profile.
- Explicitly designed write capabilities such as rename and code actions (not part of the read-only MVP).
- More languages.

## Validation Intelligence MVP: Completed

- Deterministic structured extraction from bounded safe validation metadata.
- Bounded cargo-check diagnostics and cargo-test failed-test evidence.
- Conservative validation failure-kind classification without root-cause inference.
- Shared finish/handoff evidence plus the read-only `validation_summary` session query.
- ToolRuntime, MCP, generic GPT Action, OAuth, and schema synchronization (75 runtime tools, 25 GPT Action operations).

## Future Coding Intelligence

- Richer multi-language validation adapters.
- Optional machine-readable Cargo JSON integration.
- Review and rollback UX.

## Later

- Dashboard.
- Ops packs.
- Browser/computer-use evidence.

## Non-Goals

- Full IDE replacement.
- Autonomous DevOps by default.
- Arbitrary computer use as a core promise.
- Guaranteed compatibility with every AI client.
