use super::*;
use crate::models::UserRecord;

fn oauth_seed_user(db: &Database, username: &str) -> UserRecord {
    let now = chrono::Utc::now().timestamp();
    let user = UserRecord {
        id: format!("u-{username}"),
        username: username.to_string(),
        created_at: now,
        disabled: 0,
        display_name: None,
        role: "user".to_string(),
        disabled_at: None,
        updated_at: Some(now),
    };
    db.create_user(&user).unwrap();
    user
}

use crate::test_support::seed_oauth_client_named as oauth_seed_client;

#[test]
fn verify_oauth_client_secret_works() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
    let user = oauth_seed_user(&db, "alice");
    let (client, plaintext_secret) = oauth_seed_client(&db, &user, "Test App");

    assert!(
        crate::auth::verify_oauth_client_secret(&db, &client.client_id, &plaintext_secret).unwrap()
    );
    assert!(
        !crate::auth::verify_oauth_client_secret(&db, &client.client_id, "wrong-secret").unwrap()
    );
    assert!(!crate::auth::verify_oauth_client_secret(
        &db,
        "wc_client_nonexistent",
        &plaintext_secret
    )
    .unwrap());
}
