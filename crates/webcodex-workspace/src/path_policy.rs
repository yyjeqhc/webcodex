//! Shared workspace path policy used by always-on inspection and optional snapshots.

/// Return true for project-relative paths that must be excluded from broad
/// workspace inspection/snapshot surfaces. This remains broader than the core
/// secret-path classifier because checkpoints also skip bulk/private components.
pub fn sensitive_path(path: &str) -> bool {
    webcodex_core::sensitive_paths::is_secret_path(path)
        || path_components(path)
            .any(|part| is_workspace_private_component(&part) || is_workspace_bulk_component(&part))
}

/// Return true for paths that an explicit project-overview scope must never
/// expose. Generated/high-volume directories are intentionally not classified
/// as protected here; the overview walker applies a separate bounded noise policy.
pub fn project_overview_protected_path(path: &str) -> bool {
    webcodex_core::sensitive_paths::is_secret_path(path)
        || path_components(path).any(|part| {
            is_workspace_private_component(&part)
                || matches!(part.as_str(), ".pypirc" | ".ssh" | ".aws")
                || part.ends_with(".p12")
                || part.ends_with(".pfx")
        })
}

fn is_workspace_private_component(part: &str) -> bool {
    matches!(
        part,
        ".npmrc"
            | ".netrc"
            | "secret"
            | "token"
            | "credentials"
            | "credential"
            | "passwords"
            | "password"
            | "id_rsa"
            | "id_ed25519"
    )
}

fn is_workspace_bulk_component(part: &str) -> bool {
    matches!(part, "target" | "node_modules")
}

fn path_components(path: &str) -> impl Iterator<Item = String> + '_ {
    path.split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::{project_overview_protected_path, sensitive_path};

    #[test]
    fn preserves_workspace_private_and_bulk_path_policy() {
        for path in [
            ".git/config",
            "target/debug/app",
            "node_modules/pkg/index.js",
            "project-registry/demo.toml",
            "projects.d/demo.toml",
            "runner.toml",
            "nested/.env.local",
            "keys/id_ed25519",
            "certs/client.pem",
            "secrets/token.txt",
        ] {
            assert!(sensitive_path(path), "{path}");
        }
        for protected in [
            ".git/config",
            "project-registry/demo.toml",
            "runner.toml",
            "nested/.env.local",
            "keys/id_ed25519",
            "certs/client.pem",
            "secrets/token.txt",
        ] {
            assert!(project_overview_protected_path(protected), "{protected}");
            assert!(
                crate::project_overview::normalize_project_overview_path(protected).is_err(),
                "{protected}"
            );
        }
        for high_volume in ["target/debug/app", "node_modules/pkg/index.js"] {
            assert!(
                !project_overview_protected_path(high_volume),
                "{high_volume}"
            );
            let error =
                crate::project_overview::normalize_project_overview_path(high_volume).unwrap_err();
            assert!(error.contains("high-volume"), "{high_volume}: {error}");
            assert!(!error.contains("sensitive"), "{high_volume}: {error}");
        }
        for template in [".env.example", ".ENV.SAMPLE", ".env.template", ".env.dist"] {
            assert!(!sensitive_path(template), "{template}");
            assert!(!project_overview_protected_path(template), "{template}");
            assert!(crate::project_overview::normalize_project_overview_path(template).is_ok());
        }
        assert!(project_overview_protected_path("secrets/.env.example"));
        assert!(!sensitive_path("src/lib.rs"));
        assert!(!sensitive_path("docs/architecture.md"));
    }
}
