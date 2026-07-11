//! Agent-side multi-language validation adapters.
//!
//! Structured validators (pyright, ruff, eslint) emit absolute paths that only
//! the agent can relativize against the canonical project root, so validation
//! — like LSP navigation — runs and normalizes here, returning the typed
//! `validation_bridge` result to the server. See
//! docs/MULTI_LANGUAGE_VALIDATION.md.

mod pyright;

#[allow(unused_imports)] // Consumed by the runner/dispatch wiring (in progress).
pub(crate) use pyright::{parse_pyright_output, PyrightDiagnostics};
