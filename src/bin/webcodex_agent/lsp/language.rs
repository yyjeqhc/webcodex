//! Language registry for agent-side LSP navigation.
//!
//! Every language-specific fact lives in one `LanguageProfile`: routed file
//! extensions, the LSP `languageId`, manifest detection markers, executable
//! resolution inputs, and the constrained read-only initialization profile.
//! Supervisor and navigation code consult the registry instead of hardcoding
//! language knowledge, so adding a language means adding one profile here
//! (plus its server-specific read-only safety profile and tests) — not
//! touching process management or request routing.

use super::supervisor::{compact_stderr, is_unusable_rustup_proxy, LspServerKind};
use serde_json::{json, Value};
use std::path::Path;

pub(crate) struct LanguageProfile {
    /// Supervisor process-key discriminant for this language's server.
    pub(crate) kind: LspServerKind,
    /// Primary LSP `languageId` — the public language label in tool results
    /// and the id for project-scoped operations that carry no file path
    /// (e.g. `workspace_symbols`). Per-file operations use the extension's
    /// mapped id (see `extensions`).
    pub(crate) language_id: &'static str,
    /// Public server name reported by `lsp_status`. Never a path.
    pub(crate) server_name: &'static str,
    /// Routed file extensions paired with the LSP `languageId` sent in
    /// `textDocument/didOpen` for that extension. Extensions are lower-case
    /// with no leading dot. One server can own several ids (e.g. `.tsx`
    /// maps to `typescriptreact`).
    pub(crate) extensions: &'static [(&'static str, &'static str)],
    /// Project-root marker files that mark the language as detected.
    pub(crate) manifest_markers: &'static [&'static str],
    /// Environment variable that overrides the executable path.
    pub(crate) env_override: &'static str,
    /// Executable name resolved on `PATH` when nothing is configured.
    pub(crate) executable: &'static str,
    /// Default CLI arguments appended when the command is resolved from the
    /// env override or `PATH` (e.g. `--stdio` for stdio-only servers).
    /// Explicitly configured commands are used verbatim and never get these.
    pub(crate) default_args: &'static [&'static str],
    /// Constrained read-only `initializationOptions` for this server. This is
    /// a per-language security boundary: starting the server must not execute
    /// repository code or fetch dependencies.
    pub(crate) initialization_options: fn() -> Value,
    /// Extra probe marking a resolved executable as unusable even though it
    /// is present and executable (e.g. rustup PATH shims without the
    /// component installed).
    pub(crate) unusable_command_probe: Option<fn(&Path) -> bool>,
    /// Stable classification of known startup-failure stderr into a short,
    /// path-free operator message (e.g. "component not installed"). Returns
    /// `None` to defer to the generic first-line summary. The generic
    /// supervisor holds no per-language stderr knowledge.
    pub(crate) startup_stderr_classifier: Option<fn(&str) -> Option<String>>,
}

impl LanguageProfile {
    /// True when this profile routes `extension` (case-insensitive, no dot).
    pub(crate) fn handles_extension(&self, extension: &str) -> bool {
        self.extensions
            .iter()
            .any(|(candidate, _)| extension.eq_ignore_ascii_case(candidate))
    }
}

/// All languages supported by LSP navigation. Order matters: the first
/// detected profile is the primary one for project-scoped operations.
pub(crate) static LANGUAGES: &[LanguageProfile] = &[
    LanguageProfile {
        kind: LspServerKind::RustAnalyzer,
        language_id: "rust",
        server_name: "rust-analyzer",
        extensions: &[("rs", "rust")],
        manifest_markers: &["Cargo.toml"],
        env_override: "WEBCODEX_RUST_ANALYZER",
        executable: "rust-analyzer",
        // rust-analyzer speaks LSP over stdio with no arguments.
        default_args: &[],
        initialization_options: rust_analyzer_read_only_initialization_options,
        unusable_command_probe: Some(is_unusable_rustup_proxy),
        startup_stderr_classifier: Some(rust_analyzer_startup_stderr_classifier),
    },
    LanguageProfile {
        kind: LspServerKind::Pyright,
        language_id: "python",
        server_name: "pyright",
        extensions: &[("py", "python"), ("pyi", "python")],
        manifest_markers: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "Pipfile",
            "pyrightconfig.json",
        ],
        env_override: "WEBCODEX_PYRIGHT",
        // The `pyright` npm package ships `pyright-langserver`, which speaks
        // LSP only under `--stdio`.
        executable: "pyright-langserver",
        default_args: &["--stdio"],
        initialization_options: pyright_read_only_initialization_options,
        // Pyright is a Node script on PATH; no rustup-style shim to detect.
        unusable_command_probe: None,
        startup_stderr_classifier: None,
    },
    LanguageProfile {
        kind: LspServerKind::TypeScriptLanguageServer,
        language_id: "typescript",
        server_name: "typescript-language-server",
        // typescript-language-server sends a different languageId per
        // extension; `.tsx`/`.jsx` are the React dialects.
        extensions: &[
            ("ts", "typescript"),
            ("mts", "typescript"),
            ("cts", "typescript"),
            ("tsx", "typescriptreact"),
            ("js", "javascript"),
            ("mjs", "javascript"),
            ("cjs", "javascript"),
            ("jsx", "javascriptreact"),
        ],
        manifest_markers: &["tsconfig.json", "jsconfig.json", "package.json"],
        env_override: "WEBCODEX_TYPESCRIPT_LANGUAGE_SERVER",
        executable: "typescript-language-server",
        default_args: &["--stdio"],
        initialization_options: typescript_read_only_initialization_options,
        unusable_command_probe: None,
        startup_stderr_classifier: None,
    },
];

pub(crate) fn profile_for_kind(kind: LspServerKind) -> &'static LanguageProfile {
    LANGUAGES
        .iter()
        .find(|profile| profile.kind == kind)
        .expect("every LspServerKind has exactly one LanguageProfile")
}

/// Route a file extension to its owning profile and the LSP `languageId` to
/// announce for that extension.
pub(crate) fn route_extension(extension: &str) -> Option<(&'static LanguageProfile, &'static str)> {
    LANGUAGES.iter().find_map(|profile| {
        profile
            .extensions
            .iter()
            .find(|(candidate, _)| extension.eq_ignore_ascii_case(candidate))
            .map(|(_, language_id)| (profile, *language_id))
    })
}

/// Profiles whose manifest markers exist at the project root, in registry
/// order.
pub(crate) fn detected_profiles(project_root: &Path) -> Vec<&'static LanguageProfile> {
    LANGUAGES
        .iter()
        .filter(|profile| {
            profile
                .manifest_markers
                .iter()
                .any(|marker| project_root.join(marker).is_file())
        })
        .collect()
}

/// Primary language for project-scoped operations that carry no file path
/// (e.g. `workspace_symbols`): the first detected profile, falling back to
/// the first registered one so single-language behavior does not depend on
/// manifest presence. Multi-server fan-out is an explicit follow-up decision.
pub(crate) fn primary_profile(project_root: &Path) -> &'static LanguageProfile {
    detected_profiles(project_root)
        .first()
        .copied()
        .unwrap_or(&LANGUAGES[0])
}

/// Human-readable supported-extension list for error messages, e.g.
/// `.cjs, .cts, .js, .jsx, .mjs, .mts, .py, .pyi, .rs, .ts, .tsx`.
pub(crate) fn supported_extensions_label() -> String {
    let mut extensions = LANGUAGES
        .iter()
        .flat_map(|profile| profile.extensions.iter())
        .map(|(extension, _)| format!(".{extension}"))
        .collect::<Vec<_>>();
    extensions.sort();
    extensions.dedup();
    extensions.join(", ")
}

/// Constrained read-only rust-analyzer profile for WebCodex semantic
/// navigation.
///
/// WebCodex LSP tools are read-only semantic navigation (document symbols,
/// goto definition, find references). Starting the language server must not
/// implicitly execute repository `build.rs` scripts, load or execute proc
/// macros, or run Cargo check. This is **not** a full OS sandbox; it is a
/// constrained read-only rust-analyzer profile.
///
/// Safety choices encoded here:
/// - `cargo.buildScripts.enable=false` / `procMacro.enable=false` / `checkOnSave=false`
///   prevent code execution and write-side Cargo check during analysis.
/// - `cargo.noDeps=true` is a safety and network boundary: do not fetch or
///   analyze external dependencies automatically.
/// - `files.watcher=server` because the client does not yet implement
///   watched-files registration or change notifications.
/// - `cachePriming.enable=false` avoids unnecessary background priming work.
///
/// When changing these options, update the security regression test
/// `lsp_initialize_uses_constrained_rust_analyzer_profile` in lockstep. Do not
/// allow environment variables to override these safety fields.
fn rust_analyzer_read_only_initialization_options() -> Value {
    json!({
        "cargo": {
            "buildScripts": {
                "enable": false
            },
            "noDeps": true
        },
        "procMacro": {
            "enable": false
        },
        "checkOnSave": false,
        "files": {
            "watcher": "server"
        },
        "cachePriming": {
            "enable": false
        }
    })
}

/// Constrained read-only pyright profile.
///
/// Pyright is a pure type checker: it never executes project code (there is
/// no build-script or proc-macro analog), so the code-execution boundary is
/// satisfied intrinsically. These options keep it light and read-only:
/// - `diagnosticMode=openFilesOnly` bounds analysis to opened files instead
///   of the whole workspace.
/// - `typeCheckingMode=basic` yields useful diagnostics without strict noise.
/// - `useLibraryCodeForTypes=true` lets hover read library sources (reading,
///   not executing) for better types.
/// - `autoImportCompletions=false` — navigation is read-only; no completion
///   index work.
///
/// When changing these options, update the security regression test
/// `lsp_initialize_uses_constrained_pyright_profile` in lockstep.
fn pyright_read_only_initialization_options() -> Value {
    json!({
        "python": {
            "analysis": {
                "diagnosticMode": "openFilesOnly",
                "typeCheckingMode": "basic",
                "useLibraryCodeForTypes": true,
                "autoImportCompletions": false
            }
        }
    })
}

/// Constrained read-only typescript-language-server profile.
///
/// tsserver does not execute project code for navigation. The network/external
/// boundary — the analog to rust's `cargo.noDeps=true` — is
/// `disableAutomaticTypingAcquisition=true`, which stops tsserver from
/// downloading `@types/*` packages from npm. `includePackageJsonAutoImports=off`
/// avoids building an import index for read-only navigation.
///
/// When changing these options, update the security regression test
/// `lsp_initialize_uses_constrained_typescript_profile` in lockstep.
fn typescript_read_only_initialization_options() -> Value {
    json!({
        "hostInfo": "webcodex-agent",
        "disableAutomaticTypingAcquisition": true,
        "preferences": {
            "includePackageJsonAutoImports": "off"
        }
    })
}

/// Stable classification of known rust-analyzer startup failures so operators
/// do not need raw process stderr. Today: the rustup component being absent
/// for the active toolchain. Returns `None` for anything else, deferring to
/// the generic first-line summary.
fn rust_analyzer_startup_stderr_classifier(raw: &str) -> Option<String> {
    let compact = compact_stderr(raw)?;
    let lower = compact.to_ascii_lowercase();
    if (lower.contains("unknown binary") && lower.contains("rust-analyzer"))
        || lower.contains("does not have the binary rust-analyzer")
        || lower.contains("does not have the binary `rust-analyzer`")
        || lower.contains("rustup component add rust-analyzer")
    {
        return Some(
            "rust-analyzer component is not installed for the active rustup toolchain".to_string(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_analyzer_classifier_flags_missing_component_else_defers() {
        let summary = rust_analyzer_startup_stderr_classifier(
            "error: Unknown binary 'rust-analyzer' in official toolchain 'stable-x86_64-unknown-linux-gnu'.\n",
        )
        .unwrap();
        assert_eq!(
            summary,
            "rust-analyzer component is not installed for the active rustup toolchain"
        );
        // Unrelated stderr and whitespace-only input defer to the generic path.
        assert!(rust_analyzer_startup_stderr_classifier("thread 'main' panicked").is_none());
        assert!(rust_analyzer_startup_stderr_classifier("   \n\t  ").is_none());
    }

    #[test]
    fn every_server_kind_has_exactly_one_profile() {
        // Grow this list together with `LspServerKind`.
        const ALL_KINDS: &[LspServerKind] = &[
            LspServerKind::RustAnalyzer,
            LspServerKind::Pyright,
            LspServerKind::TypeScriptLanguageServer,
        ];
        for kind in ALL_KINDS {
            let matching = LANGUAGES
                .iter()
                .filter(|profile| profile.kind == *kind)
                .count();
            assert_eq!(matching, 1, "kind {kind:?} must have exactly one profile");
        }
        assert_eq!(LANGUAGES.len(), ALL_KINDS.len());
    }

    #[test]
    fn profiles_are_internally_consistent() {
        let mut seen_extensions = std::collections::HashSet::new();
        for profile in LANGUAGES {
            assert!(!profile.language_id.is_empty());
            assert!(!profile.server_name.is_empty());
            assert!(!profile.executable.is_empty());
            assert!(!profile.env_override.is_empty());
            assert!(!profile.extensions.is_empty());
            assert!(!profile.manifest_markers.is_empty());
            for (extension, language_id) in profile.extensions {
                assert_eq!(
                    extension.to_ascii_lowercase().as_str(),
                    *extension,
                    "extensions must be registered lower-case"
                );
                assert!(!extension.starts_with('.'));
                assert!(!language_id.is_empty());
                // No extension may be claimed by two profiles, or routing
                // would depend on registry order.
                assert!(
                    seen_extensions.insert(*extension),
                    "extension .{extension} is claimed by more than one profile"
                );
                // Routing returns this profile and this extension's languageId.
                let (routed, routed_id) = route_extension(extension).unwrap();
                assert_eq!(routed.kind, profile.kind);
                assert_eq!(routed_id, *language_id);
            }
        }
    }

    #[test]
    fn extension_routing_is_case_insensitive_and_closed() {
        assert_eq!(route_extension("rs").unwrap().1, "rust");
        assert_eq!(route_extension("RS").unwrap().1, "rust");
        assert_eq!(route_extension("py").unwrap().1, "python");
        assert_eq!(route_extension("pyi").unwrap().1, "python");
        // TypeScript sends dialect-specific ids per extension.
        assert_eq!(route_extension("ts").unwrap().1, "typescript");
        assert_eq!(route_extension("tsx").unwrap().1, "typescriptreact");
        assert_eq!(route_extension("jsx").unwrap().1, "javascriptreact");
        assert_eq!(route_extension("js").unwrap().1, "javascript");
        assert!(route_extension("toml").is_none());
        assert!(route_extension("").is_none());
    }

    #[test]
    fn extension_routing_selects_the_right_server_kind() {
        assert_eq!(
            route_extension("py").unwrap().0.kind,
            LspServerKind::Pyright
        );
        assert_eq!(
            route_extension("tsx").unwrap().0.kind,
            LspServerKind::TypeScriptLanguageServer
        );
        assert_eq!(
            route_extension("rs").unwrap().0.kind,
            LspServerKind::RustAnalyzer
        );
    }

    #[test]
    fn supported_extensions_label_lists_all_registered_extensions() {
        assert_eq!(
            supported_extensions_label(),
            ".cjs, .cts, .js, .jsx, .mjs, .mts, .py, .pyi, .rs, .ts, .tsx"
        );
    }

    #[test]
    fn manifest_detection_is_per_language() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[project]\n").unwrap();
        let detected = detected_profiles(dir.path());
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].kind, LspServerKind::Pyright);
        assert_eq!(primary_profile(dir.path()).kind, LspServerKind::Pyright);
    }
}
