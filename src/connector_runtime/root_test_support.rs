use crate::auth::{
    AuthContext, AuthKind, ProjectCredentialVerifier, SCOPE_JOB_RUN, SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
};
use std::path::Path;

pub(crate) const PROJECT_GRANT_ID: &str = "wc_pgrant_1111111111111111";
const PROJECT_CREDENTIAL: &str =
    "webcodex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(crate) fn credential() -> ProjectCredentialVerifier {
    ProjectCredentialVerifier::new(PROJECT_GRANT_ID.to_string(), PROJECT_CREDENTIAL).unwrap()
}

pub(crate) fn auth(user_id: &str) -> AuthContext {
    let project_grant_id = if user_id == "u1" {
        PROJECT_GRANT_ID.to_string()
    } else {
        "wc_pgrant_2222222222222222".to_string()
    };
    AuthContext {
        role: Some("project".to_string()),
        scopes: vec![
            SCOPE_RUNTIME_READ.to_string(),
            SCOPE_PROJECT_READ.to_string(),
            SCOPE_PROJECT_WRITE.to_string(),
            SCOPE_JOB_RUN.to_string(),
        ],
        token_kind: Some("project".to_string()),
        project_grant_id: Some(project_grant_id),
        ..AuthContext::new(AuthKind::ProjectCredential)
    }
}

pub(crate) fn init_repo(project: &Path) {
    std::fs::create_dir(project).unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(project)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "-q"]);
    run(&["config", "core.autocrlf", "false"]);
    run(&["config", "core.longpaths", "true"]);
    std::fs::write(project.join("README.md"), "fixture\n").unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"connector-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    run(&["add", "README.md", "Cargo.toml"]);
    run(&[
        "-c",
        "user.name=WebCodex Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "initial",
    ]);
}
