//! Shared test-only helpers.
//!
//! Single authoritative home for the `Config` builders, temp-dir `Database`
//! constructor, and DB row seeding that the HTTP/auth test modules previously
//! each carried a byte-identical copy of. Compiled only under `cfg(test)`
//! (see the `mod test_support` declaration in `main.rs`).

use std::path::PathBuf;
use std::sync::Arc;

/// Minimal `Config` for tests (token sets whether auth is enabled).
pub(crate) fn test_config(token: Option<&str>) -> Arc<crate::Config> {
    Arc::new(crate::Config {
        addr: "127.0.0.1:0".to_string(),
        data_dir: PathBuf::from("./data"),
        token: token.map(str::to_string),
        max_text_size: 2 * 1024 * 1024,
        max_file_size: 100 * 1024 * 1024,
        codex: crate::CodexConfig::default(),
        oauth2: crate::OAuth2Config::default(),
    })
}

/// Like [`test_config`] but with OAuth2 enabled (1h access-token TTL,
/// 30d refresh-token TTL).
pub(crate) fn test_config_oauth2(token: Option<&str>) -> Arc<crate::Config> {
    Arc::new(crate::Config {
        addr: "127.0.0.1:0".to_string(),
        data_dir: PathBuf::from("./data"),
        token: token.map(str::to_string),
        max_text_size: 2 * 1024 * 1024,
        max_file_size: 100 * 1024 * 1024,
        codex: crate::CodexConfig::default(),
        oauth2: crate::OAuth2Config {
            enabled: true,
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 2_592_000,
            ..crate::OAuth2Config::default()
        },
    })
}

/// Create an empty Database in a temp dir. The TempDir must be kept alive
/// for the lifetime of the returned Database so the sqlite file is not
/// deleted mid-test.
pub(crate) fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
    let tmp = tempfile::tempdir().unwrap();
    let db = crate::Database::open(&tmp.path().join("test.db")).unwrap();
    (tmp, Arc::new(db))
}

/// Bootstrap helper: create a user with the given role directly via the DB
/// so tests can mint tokens for them.
pub(crate) fn seed_user_with_role(
    db: &crate::Database,
    username: &str,
    role: &str,
) -> crate::models::UserRecord {
    let now = chrono::Utc::now().timestamp();
    let user = crate::models::UserRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.to_string(),
        created_at: now,
        disabled: 0,
        display_name: None,
        role: role.to_string(),
        disabled_at: None,
        updated_at: Some(now),
    };
    db.create_user(&user).unwrap();
    user
}

/// [`seed_user_with_role`] with the default `"user"` role.
pub(crate) fn seed_user(db: &crate::Database, username: &str) -> crate::models::UserRecord {
    seed_user_with_role(db, username, "user")
}

/// Shared body for the two `seed_oauth_client*` shapes below.
fn seed_oauth_client_record(
    db: &crate::Database,
    user: &crate::models::UserRecord,
    name: &str,
    allowed_scopes: &str,
) -> (crate::models::OAuthClientRecord, String) {
    let now = chrono::Utc::now().timestamp();
    let plaintext_secret = crate::auth::generate_oauth_client_secret();
    let record = crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: crate::auth::hash_token(&plaintext_secret),
        name: name.to_string(),
        owner_user_id: Some(user.id.clone()),
        owner_project_grant_id: None,
        redirect_uris: "https://example.com/callback".to_string(),
        allowed_scopes: allowed_scopes.to_string(),
        created_at: now,
        revoked_at: None,
    };
    db.insert_oauth_client(&record).unwrap();
    (record, plaintext_secret)
}

/// Seed an OAuth2 client ("Test App") owned by `user` with the broad scope
/// set used by the mcp/runtime_http HTTP tests. The plaintext secret is
/// discarded.
pub(crate) fn seed_oauth_client(
    db: &crate::Database,
    user: &crate::models::UserRecord,
) -> crate::models::OAuthClientRecord {
    seed_oauth_client_record(
        db,
        user,
        "Test App",
        "runtime:read project:read project:write job:run account:manage",
    )
    .0
}

/// Seed a named OAuth2 client with the narrow `runtime:read project:read`
/// scope set and return `(record, plaintext_secret)`.
pub(crate) fn seed_oauth_client_named(
    db: &crate::Database,
    user: &crate::models::UserRecord,
    name: &str,
) -> (crate::models::OAuthClientRecord, String) {
    seed_oauth_client_record(db, user, name, "runtime:read project:read")
}
