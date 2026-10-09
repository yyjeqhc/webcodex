use crate::models::{
    OAuthAccessTokenRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
    OAuthRefreshTokenRecord, UserRecord,
};
use crate::{
    Database, PublicIngressEntry, PublicIngressEntrySpec, PublicIngressMode, RotateResult,
};
use std::sync::{Arc, Barrier};

struct Fixture {
    _dir: tempfile::TempDir,
    db: Database,
    now: i64,
    spec: PublicIngressEntrySpec,
    client: OAuthClientRecord,
}
impl Fixture {
    fn new(mode: PublicIngressMode) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("ingress.sqlite")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "owner".into(),
            username: "owner".into(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".into(),
            disabled_at: None,
            updated_at: None,
        })
        .unwrap();
        let client = OAuthClientRecord {
            id: "client-row".into(),
            client_id: "client".into(),
            client_secret_hash: "hashed-secret".into(),
            name: "app".into(),
            owner_user_id: Some("owner".into()),
            owner_project_grant_id: None,
            owner_shared_key_hash: None,
            redirect_uris: "https://client.example/callback".into(),
            allowed_scopes: "runtime:read".into(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&client).unwrap();
        let spec = PublicIngressEntrySpec {
            entry_id: "entry".into(),
            profile_id: "profile".into(),
            mode,
            origin: "https://public.example".into(),
            owner_user_id: "owner".into(),
            process_generation: 1,
            server_instance_id: "server".into(),
        };
        Self {
            _dir: dir,
            db,
            now,
            spec,
            client,
        }
    }
    fn activate(&self) -> PublicIngressEntry {
        let entry = self
            .db
            .prepare_public_ingress_entry(&self.spec, None)
            .unwrap()
            .unwrap();
        assert!(self
            .db
            .set_public_ingress_admission(&entry.fence(), true)
            .unwrap());
        assert!(self
            .db
            .bind_oauth_client_to_public_ingress(&self.client.client_id, &entry.fence())
            .unwrap());
        self.db
            .get_public_ingress_entry(&entry.entry_id)
            .unwrap()
            .unwrap()
    }
    fn code(&self, suffix: &str) -> OAuthAuthorizationCodeRecord {
        OAuthAuthorizationCodeRecord {
            id: format!("code-{suffix}"),
            code_hash: format!("code-hash-{suffix}"),
            client_id: self.client.client_id.clone(),
            subject_kind: "managed_user".into(),
            subject_id: "owner".into(),
            user_id: Some("owner".into()),
            redirect_uri: "https://client.example/callback".into(),
            scopes: "runtime:read".into(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: self.now,
            expires_at: self.now + 600,
            used_at: None,
            revoked_at: None,
            admin_authority: false,
        }
    }
    fn tokens(&self, suffix: &str) -> (OAuthAccessTokenRecord, OAuthRefreshTokenRecord) {
        let access = OAuthAccessTokenRecord {
            id: format!("access-{suffix}"),
            token_hash: format!("access-hash-{suffix}"),
            client_id: self.client.client_id.clone(),
            subject_kind: "managed_user".into(),
            subject_id: "owner".into(),
            user_id: Some("owner".into()),
            scopes: "runtime:read".into(),
            resource: None,
            shared_key_hash: None,
            created_at: self.now,
            expires_at: self.now + 3600,
            revoked_at: None,
            last_used_at: None,
            admin_authority: false,
        };
        let refresh = OAuthRefreshTokenRecord {
            id: format!("refresh-{suffix}"),
            token_hash: format!("refresh-hash-{suffix}"),
            client_id: access.client_id.clone(),
            subject_kind: access.subject_kind.clone(),
            subject_id: access.subject_id.clone(),
            user_id: access.user_id.clone(),
            scopes: access.scopes.clone(),
            resource: None,
            shared_key_hash: None,
            created_at: self.now,
            expires_at: self.now + 7200,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
            admin_authority: false,
        };
        (access, refresh)
    }
    fn issue(
        &self,
        entry: &PublicIngressEntry,
        suffix: &str,
    ) -> (OAuthAccessTokenRecord, OAuthRefreshTokenRecord) {
        let code = self.code(suffix);
        assert!(self
            .db
            .insert_public_ingress_oauth_authorization_code(&code, &code.code_hash, &entry.fence())
            .unwrap());
        let (access, refresh) = self.tokens(suffix);
        assert!(self
            .db
            .exchange_oauth_authorization_code_for_tokens(
                &code.code_hash,
                self.now,
                &access,
                &refresh
            )
            .unwrap()
            .is_some());
        (access, refresh)
    }
}

#[test]
fn public_ingress_named_restart_preserves_epoch_and_rotated_grant_binding_without_resource() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    let entry = f.activate();
    let (access, refresh) = f.issue(&entry, "first");
    assert!(f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&entry.fence()))
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, None)
        .unwrap());
    assert!(f
        .db
        .set_public_ingress_admission(&entry.fence(), false)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&entry.fence()))
        .unwrap());
    f.spec.server_instance_id = "new-server".into();
    let restarted =
        f.db.prepare_public_ingress_entry(&f.spec, Some(entry.runtime_revision))
            .unwrap()
            .unwrap();
    assert_eq!(restarted.auth_epoch, entry.auth_epoch);
    assert!(f
        .db
        .set_public_ingress_admission(&restarted.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&entry.fence()))
        .unwrap());
    assert!(f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&restarted.fence()))
        .unwrap());
    let (next_access, next_refresh) = f.tokens("rotated");
    assert!(matches!(
        f.db.rotate_oauth_refresh_token(
            &refresh.token_hash,
            &f.client.client_id,
            f.now,
            &next_access,
            &next_refresh
        )
        .unwrap(),
        RotateResult::Rotated(_)
    ));
    assert!(f
        .db
        .oauth_access_token_matches_public_ingress(
            &next_access.token_hash,
            Some(&restarted.fence())
        )
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&next_access.token_hash, None)
        .unwrap());
    let conn = f.db.conn_for_tests();
    let bindings: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM oauth_public_ingress_refresh",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bindings, 2);
}

#[test]
fn public_ingress_quick_stop_and_address_change_revoke_grants_but_retain_client() {
    let mut f = Fixture::new(PublicIngressMode::Quick);
    let entry = f.activate();
    let (access, refresh) = f.issue(&entry, "first");
    let pending = f.code("pending");
    assert!(f
        .db
        .insert_public_ingress_oauth_authorization_code(
            &pending,
            &pending.code_hash,
            &entry.fence()
        )
        .unwrap());
    assert!(f
        .db
        .set_public_ingress_admission(&entry.fence(), false)
        .unwrap());
    assert!(!f
        .db
        .set_public_ingress_admission(&entry.fence(), true)
        .unwrap());
    let stopped = f.db.get_public_ingress_entry("entry").unwrap().unwrap();
    assert!(stopped.attempt_closed);
    assert!(stopped.auth_epoch > entry.auth_epoch);
    let (a, r) = f.tokens("closed");
    assert!(f
        .db
        .exchange_oauth_authorization_code_for_tokens(&pending.code_hash, f.now, &a, &r)
        .unwrap()
        .is_none());
    assert!(matches!(
        f.db.rotate_oauth_refresh_token(&refresh.token_hash, "client", f.now, &a, &r)
            .unwrap(),
        RotateResult::Revoked
    ));
    f.spec.origin = "https://new-address.example".into();
    f.spec.process_generation += 1;
    let next =
        f.db.prepare_public_ingress_entry(&f.spec, Some(entry.runtime_revision))
            .unwrap()
            .unwrap();
    assert!(next.auth_epoch > stopped.auth_epoch);
    assert!(f
        .db
        .set_public_ingress_admission(&next.fence(), true)
        .unwrap());
    assert!(f
        .db
        .oauth_client_matches_public_ingress("client", &next.fence())
        .unwrap());
    assert_eq!(
        f.db.oauth_client_public_ingress_entry_id("client")
            .unwrap()
            .as_deref(),
        Some("entry")
    );
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&next.fence()))
        .unwrap());
    assert!(f
        .db
        .exchange_oauth_authorization_code_for_tokens(&pending.code_hash, f.now, &a, &r)
        .unwrap()
        .is_none());
    assert!(matches!(
        f.db.rotate_oauth_refresh_token(&refresh.token_hash, "client", f.now, &a, &r)
            .unwrap(),
        RotateResult::Revoked
    ));
    let (fresh, _) = f.issue(&next, "fresh");
    assert!(f
        .db
        .oauth_access_token_matches_public_ingress(&fresh.token_hash, Some(&next.fence()))
        .unwrap());
}

#[test]
fn public_ingress_named_origin_and_owner_changes_rotate_epoch() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    let entry = f.activate();
    let (access, _) = f.issue(&entry, "first");
    f.spec.origin = "https://replacement.example".into();
    f.spec.process_generation += 1;
    let changed =
        f.db.prepare_public_ingress_entry(&f.spec, Some(entry.runtime_revision))
            .unwrap()
            .unwrap();
    assert!(changed.auth_epoch > entry.auth_epoch);
    assert!(f
        .db
        .set_public_ingress_admission(&changed.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&changed.fence()))
        .unwrap());
    f.db.create_user(&UserRecord {
        id: "other".into(),
        username: "other".into(),
        created_at: f.now,
        disabled: 0,
        display_name: None,
        role: "user".into(),
        disabled_at: None,
        updated_at: None,
    })
    .unwrap();
    f.spec.owner_user_id = "other".into();
    f.spec.process_generation += 1;
    let reassigned =
        f.db.prepare_public_ingress_entry(&f.spec, Some(changed.runtime_revision))
            .unwrap()
            .unwrap();
    assert!(reassigned.auth_epoch > changed.auth_epoch);
    assert!(f
        .db
        .set_public_ingress_admission(&reassigned.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_client_matches_public_ingress("client", &reassigned.fence())
        .unwrap());
}

#[test]
fn public_ingress_single_active_and_stale_generation_cas() {
    let mut f = Fixture::new(PublicIngressMode::Quick);
    let first = f.activate();
    let second_spec = PublicIngressEntrySpec {
        entry_id: "second".into(),
        profile_id: "second-profile".into(),
        ..f.spec.clone()
    };
    let second =
        f.db.prepare_public_ingress_entry(&second_spec, None)
            .unwrap()
            .unwrap();
    assert!(f
        .db
        .set_public_ingress_admission(&second.fence(), true)
        .unwrap());
    assert_eq!(
        f.db.get_active_public_ingress_entry()
            .unwrap()
            .unwrap()
            .entry_id,
        "second"
    );
    assert!(!f
        .db
        .set_public_ingress_admission(&first.fence(), true)
        .unwrap());
    assert!(
        f.db.get_public_ingress_entry("entry")
            .unwrap()
            .unwrap()
            .auth_epoch
            > first.auth_epoch
    );
    assert!(f
        .db
        .prepare_public_ingress_entry(&f.spec, Some(first.runtime_revision))
        .unwrap()
        .is_none());
    f.spec.process_generation += 1;
    assert!(f
        .db
        .prepare_public_ingress_entry(&f.spec, None)
        .unwrap()
        .is_none());
    let replacement =
        f.db.prepare_public_ingress_entry(&f.spec, Some(first.runtime_revision))
            .unwrap()
            .unwrap();
    assert!(!f
        .db
        .set_public_ingress_admission(&first.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .set_public_ingress_admission(&first.fence(), false)
        .unwrap());
    assert!(f
        .db
        .set_public_ingress_admission(&replacement.fence(), true)
        .unwrap());
    let conn = f.db.conn_for_tests();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM public_ingress_entries WHERE active=1",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn public_ingress_legacy_migration_keeps_private_grants_unbound() {
    let f = Fixture::new(PublicIngressMode::Named);
    let code = f.code("legacy");
    f.db.insert_oauth_authorization_code(&code, &code.code_hash)
        .unwrap();
    let (access, refresh) = f.tokens("legacy");
    assert!(f
        .db
        .exchange_oauth_authorization_code_for_tokens(&code.code_hash, f.now, &access, &refresh)
        .unwrap()
        .is_some());
    let path = f._dir.path().join("ingress.sqlite");
    f.db.conn_for_tests().execute_batch("DROP TABLE oauth_public_ingress_codes; DROP TABLE oauth_public_ingress_access; DROP TABLE oauth_public_ingress_refresh; DROP TABLE oauth_public_ingress_clients; DROP TABLE public_ingress_entries;").unwrap();
    for _ in 0..2 {
        let reopened = Database::open(&path).unwrap();
        assert!(reopened
            .oauth_access_token_matches_public_ingress(&access.token_hash, None)
            .unwrap());
        assert!(reopened
            .oauth_client_public_ingress_entry_id("client")
            .unwrap()
            .is_none());
        assert!(reopened
            .get_active_public_ingress_entry()
            .unwrap()
            .is_none());
    }
    let entry =
        f.db.prepare_public_ingress_entry(&f.spec, None)
            .unwrap()
            .unwrap();
    assert!(f
        .db
        .set_public_ingress_admission(&entry.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&entry.fence()))
        .unwrap());
    assert!(!f
        .db
        .bind_oauth_client_to_public_ingress("client", &entry.fence())
        .unwrap());
    let (a, r) = f.tokens("private-rotated");
    assert!(matches!(
        f.db.rotate_oauth_refresh_token(&refresh.token_hash, "client", f.now, &a, &r)
            .unwrap(),
        RotateResult::Rotated(_)
    ));
    assert!(f
        .db
        .oauth_access_token_matches_public_ingress(&a.token_hash, None)
        .unwrap());
}

#[test]
fn public_ingress_bound_clients_reject_unbound_issuance_and_wrong_resource() {
    let f = Fixture::new(PublicIngressMode::Named);
    let entry = f.activate();
    let code = f.code("private");
    assert!(f
        .db
        .insert_oauth_authorization_code(&code, &code.code_hash)
        .is_err());
    let (a, r) = f.tokens("private");
    assert!(f.db.insert_oauth_access_token(&a).is_err());
    assert!(f.db.insert_oauth_refresh_token(&r).is_err());
    let mut wrong = f.code("wrong-origin");
    wrong.resource = Some("https://other.example/mcp".into());
    assert!(!f
        .db
        .insert_public_ingress_oauth_authorization_code(&wrong, &wrong.code_hash, &entry.fence())
        .unwrap());
    let right = f.code("right");
    assert!(f
        .db
        .insert_public_ingress_oauth_authorization_code(&right, &right.code_hash, &entry.fence())
        .unwrap());
    let mut wrong_access = a.clone();
    wrong_access.resource = Some("https://other.example/mcp".into());
    assert!(f
        .db
        .exchange_oauth_authorization_code_for_tokens(&right.code_hash, f.now, &wrong_access, &r)
        .unwrap()
        .is_none());
    assert!(f
        .db
        .get_oauth_authorization_code_by_hash(&right.code_hash)
        .unwrap()
        .unwrap()
        .used_at
        .is_none());
    assert!(f
        .db
        .exchange_oauth_authorization_code_for_tokens(&right.code_hash, f.now, &a, &r)
        .unwrap()
        .is_some());
    f.db.revoke_oauth_client_by_client_id("client", f.now)
        .unwrap();
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&a.token_hash, Some(&entry.fence()))
        .unwrap());
}

#[test]
fn public_ingress_concurrent_epoch_rotation_and_omitted_resource_exchange_fail_closed() {
    let mut f = Fixture::new(PublicIngressMode::Quick);
    let entry = f.activate();
    let code = f.code("race");
    assert!(f
        .db
        .insert_public_ingress_oauth_authorization_code(&code, &code.code_hash, &entry.fence())
        .unwrap());
    let (access, refresh) = f.tokens("race");
    let path = f._dir.path().join("ingress.sqlite");
    let other = Database::open(&path).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    f.spec.process_generation += 1;
    let next = std::thread::scope(|scope| {
        let child_barrier = barrier.clone();
        let spec = &f.spec;
        let handle = scope.spawn(move || {
            child_barrier.wait();
            other
                .prepare_public_ingress_entry(spec, Some(entry.runtime_revision))
                .unwrap()
                .unwrap()
        });
        barrier.wait();
        let _issued = f
            .db
            .exchange_oauth_authorization_code_for_tokens(&code.code_hash, f.now, &access, &refresh)
            .unwrap();
        handle.join().unwrap()
    });
    assert!(f
        .db
        .set_public_ingress_admission(&next.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&next.fence()))
        .unwrap());
    let (a, r) = f.tokens("after-race");
    assert!(!matches!(
        f.db.rotate_oauth_refresh_token(&refresh.token_hash, "client", f.now, &a, &r)
            .unwrap(),
        RotateResult::Rotated(_)
    ));
    assert!(f
        .db
        .exchange_oauth_authorization_code_for_tokens(&code.code_hash, f.now, &a, &r)
        .unwrap()
        .is_none());
}

#[test]
fn public_ingress_exact_https_origin_and_profile_identity_are_required() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    for origin in [
        "http://public.example",
        "https://public.example/",
        "https://public.example/path",
        "https://public.example?x=1",
        "https://user:pass@public.example",
        "https://public.example:443",
        "https://PUBLIC.example",
    ] {
        f.spec.origin = origin.into();
        assert!(
            f.db.prepare_public_ingress_entry(&f.spec, None).is_err(),
            "accepted {origin}"
        );
    }
    f.spec.origin = "https://public.example".into();
    let entry = f.activate();
    f.spec.profile_id = "different-profile".into();
    f.spec.process_generation += 1;
    assert!(f
        .db
        .prepare_public_ingress_entry(&f.spec, Some(entry.runtime_revision))
        .is_err());
}

#[test]
fn public_ingress_companion_grants_are_removed_by_auth_cleanup() {
    let f = Fixture::new(PublicIngressMode::Named);
    let entry = f.activate();
    let (access, refresh) = f.issue(&entry, "cleanup");
    f.db.revoke_oauth_access_token(&access.id, f.now).unwrap();
    f.db.revoke_oauth_refresh_token(&refresh.id, f.now).unwrap();
    f.db.purge_stale_auth_rows(f.now).unwrap();
    let conn = f.db.conn_for_tests();
    for table in [
        "oauth_public_ingress_codes",
        "oauth_public_ingress_access",
        "oauth_public_ingress_refresh",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM oauth_public_ingress_clients",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn public_ingress_quick_origin_finalization_keeps_generation_for_second_start() {
    let mut f = Fixture::new(PublicIngressMode::Quick);
    f.spec.origin = "https://pending.invalid".into();
    let first =
        f.db.prepare_public_ingress_entry(&f.spec, None)
            .unwrap()
            .unwrap();
    assert!(!f
        .db
        .bind_oauth_client_to_public_ingress("client", &first.fence())
        .unwrap());
    let pending_code = f.code("before-admission");
    assert!(!f
        .db
        .insert_public_ingress_oauth_authorization_code(
            &pending_code,
            &pending_code.code_hash,
            &first.fence()
        )
        .unwrap());
    let finalized =
        f.db.finalize_public_ingress_origin(&first.fence(), "https://first.trycloudflare.com")
            .unwrap()
            .unwrap();
    assert_eq!(finalized.origin, "https://first.trycloudflare.com");
    assert_eq!(finalized.fence(), first.fence());
    assert_eq!(finalized.auth_epoch, first.auth_epoch);
    assert!(!finalized.admission_open);
    assert!(!f
        .db
        .insert_public_ingress_oauth_authorization_code(
            &pending_code,
            &pending_code.code_hash,
            &finalized.fence()
        )
        .unwrap());
    assert!(f
        .db
        .set_public_ingress_admission(&finalized.fence(), true)
        .unwrap());
    assert!(f
        .db
        .bind_oauth_client_to_public_ingress("client", &finalized.fence())
        .unwrap());
    let (old_access, _) = f.issue(&finalized, "first-attempt");
    assert!(f
        .db
        .finalize_public_ingress_origin(
            &finalized.fence(),
            "https://while-active.trycloudflare.com"
        )
        .unwrap()
        .is_none());
    assert!(f
        .db
        .set_public_ingress_admission(&finalized.fence(), false)
        .unwrap());
    assert!(f
        .db
        .finalize_public_ingress_origin(
            &finalized.fence(),
            "https://while-closed.trycloudflare.com"
        )
        .unwrap()
        .is_none());
    // The transport's next generation is exactly +1, including after discovering
    // the first address. Origin discovery must not consume a phantom generation.
    f.spec.process_generation += 1;
    let second =
        f.db.prepare_public_ingress_entry(&f.spec, Some(finalized.runtime_revision))
            .unwrap()
            .unwrap();
    assert_eq!(second.process_generation, first.process_generation + 1);
    assert!(second.auth_epoch > first.auth_epoch);
    assert!(f
        .db
        .finalize_public_ingress_origin(&first.fence(), "https://stale.trycloudflare.com")
        .unwrap()
        .is_none());
    let second_finalized =
        f.db.finalize_public_ingress_origin(&second.fence(), "https://second.trycloudflare.com")
            .unwrap()
            .unwrap();
    assert_eq!(second_finalized.fence(), second.fence());
    assert!(f
        .db
        .set_public_ingress_admission(&second_finalized.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(
            &old_access.token_hash,
            Some(&second_finalized.fence())
        )
        .unwrap());
    let (fresh, _) = f.issue(&second_finalized, "second-attempt");
    assert!(f
        .db
        .oauth_access_token_matches_public_ingress(
            &fresh.token_hash,
            Some(&second_finalized.fence())
        )
        .unwrap());
}

#[test]
fn public_ingress_origin_finalization_rejects_named_and_forged_fences() {
    let named = Fixture::new(PublicIngressMode::Named);
    let entry = named
        .db
        .prepare_public_ingress_entry(&named.spec, None)
        .unwrap()
        .unwrap();
    assert!(named
        .db
        .finalize_public_ingress_origin(&entry.fence(), "https://changed.example")
        .unwrap()
        .is_none());
    assert_eq!(
        named
            .db
            .get_public_ingress_entry(&entry.entry_id)
            .unwrap()
            .unwrap()
            .origin,
        named.spec.origin
    );

    let quick = Fixture::new(PublicIngressMode::Quick);
    let entry = quick
        .db
        .prepare_public_ingress_entry(&quick.spec, None)
        .unwrap()
        .unwrap();
    for fence in [
        webcodex_store_fence(&entry, "entry_id"),
        webcodex_store_fence(&entry, "runtime_revision"),
        webcodex_store_fence(&entry, "process_generation"),
        webcodex_store_fence(&entry, "server_instance_id"),
    ] {
        assert!(quick
            .db
            .finalize_public_ingress_origin(&fence, "https://forged.trycloudflare.com")
            .unwrap()
            .is_none());
    }
    for invalid in [
        "http://insecure.example",
        "https://origin.example/path",
        "https://user:secret@origin.example",
        "https://ORIGIN.example",
        "https://origin.example/",
    ] {
        assert!(quick
            .db
            .finalize_public_ingress_origin(&entry.fence(), invalid)
            .is_err());
    }
    assert_eq!(
        quick
            .db
            .get_public_ingress_entry(&entry.entry_id)
            .unwrap()
            .unwrap()
            .origin,
        quick.spec.origin
    );
}

fn webcodex_store_fence(entry: &PublicIngressEntry, change: &str) -> crate::PublicIngressFence {
    let mut fence = entry.fence();
    match change {
        "entry_id" => fence.entry_id = "other-entry".into(),
        "runtime_revision" => fence.runtime_revision += 1,
        "process_generation" => fence.process_generation += 1,
        "server_instance_id" => fence.server_instance_id = "other-server".into(),
        _ => unreachable!(),
    }
    fence
}

#[test]
fn public_ingress_observed_authorization_requires_used_live_grant_in_exact_epoch() {
    for mode in [PublicIngressMode::Named, PublicIngressMode::Quick] {
        let mut f = Fixture::new(mode);
        let entry = f.activate();
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&entry.fence())
            .unwrap());
        let (access, _) = f.issue(&entry, "unused");
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&entry.fence())
            .unwrap());
        f.db.update_oauth_access_token_last_used(&access.id, f.now)
            .unwrap();
        assert!(f
            .db
            .public_ingress_has_observed_authorization(&entry.fence())
            .unwrap());
        for change in [
            "entry_id",
            "runtime_revision",
            "process_generation",
            "server_instance_id",
        ] {
            assert!(!f
                .db
                .public_ingress_has_observed_authorization(&webcodex_store_fence(&entry, change))
                .unwrap());
        }
        assert!(f
            .db
            .set_public_ingress_admission(&entry.fence(), false)
            .unwrap());
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&entry.fence())
            .unwrap());
        f.spec.process_generation += 1;
        let restarted =
            f.db.prepare_public_ingress_entry(&f.spec, Some(entry.runtime_revision))
                .unwrap()
                .unwrap();
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&restarted.fence())
            .unwrap());
        assert!(f
            .db
            .set_public_ingress_admission(&restarted.fence(), true)
            .unwrap());
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&entry.fence())
            .unwrap());
        assert_eq!(
            f.db.public_ingress_has_observed_authorization(&restarted.fence())
                .unwrap(),
            mode == PublicIngressMode::Named
        );
        let (fresh, _) = f.issue(&restarted, "observed-current");
        f.db.update_oauth_access_token_last_used(&fresh.id, f.now)
            .unwrap();
        assert!(f
            .db
            .public_ingress_has_observed_authorization(&restarted.fence())
            .unwrap());
        f.db.revoke_oauth_access_token(&fresh.id, f.now).unwrap();
        if mode == PublicIngressMode::Named {
            f.db.revoke_oauth_access_token(&access.id, f.now).unwrap();
        }
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&restarted.fence())
            .unwrap());
        let (expired, _) = f.issue(&restarted, "expired-observation");
        f.db.update_oauth_access_token_last_used(&expired.id, f.now)
            .unwrap();
        f.db.conn_for_tests()
            .execute(
                "UPDATE oauth_access_tokens SET expires_at=?2 WHERE id=?1",
                rusqlite::params![expired.id, f.now - 1],
            )
            .unwrap();
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&restarted.fence())
            .unwrap());
        let (live, _) = f.issue(&restarted, "live-client-owner-observation");
        f.db.update_oauth_access_token_last_used(&live.id, f.now)
            .unwrap();
        assert!(f
            .db
            .public_ingress_has_observed_authorization(&restarted.fence())
            .unwrap());
        if mode == PublicIngressMode::Named {
            f.db.revoke_oauth_client(&f.client.id, f.now).unwrap();
        } else {
            f.db.conn_for_tests()
                .execute(
                    "UPDATE users SET disabled=1 WHERE id=?1",
                    [&f.spec.owner_user_id],
                )
                .unwrap();
        }
        assert!(!f
            .db
            .public_ingress_has_observed_authorization(&restarted.fence())
            .unwrap());
    }
}

#[test]
fn public_ingress_configuration_recreation_rotates_named_grants_without_reviving_them() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    f.db.reconcile_public_ingress_configuration("profile", "incarnation-one")
        .unwrap();
    let entry = f.activate();
    let (access, refresh) = f.issue(&entry, "configuration");
    f.db.reconcile_public_ingress_configuration("profile", "incarnation-one")
        .unwrap();
    f.db.reconcile_public_ingress_configuration("other", "incarnation-two")
        .unwrap();
    assert_eq!(
        f.db.get_public_ingress_entry("entry").unwrap().unwrap(),
        entry
    );
    assert!(f
        .db
        .reconcile_public_ingress_configuration("profile", "")
        .is_err());
    assert!(f
        .db
        .reconcile_public_ingress_configuration("", "incarnation-two")
        .is_err());
    assert_eq!(
        f.db.get_public_ingress_entry("entry").unwrap().unwrap(),
        entry
    );
    f.db.reconcile_public_ingress_configuration("profile", "incarnation-two")
        .unwrap();
    let closed = f.db.get_public_ingress_entry("entry").unwrap().unwrap();
    assert_eq!(closed.auth_epoch, entry.auth_epoch + 1);
    assert!(!closed.active && !closed.admission_open && closed.attempt_closed);
    assert!(!f
        .db
        .set_public_ingress_admission(&entry.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&entry.fence()))
        .unwrap());
    f.db.reconcile_public_ingress_configuration("profile", "incarnation-two")
        .unwrap();
    assert_eq!(
        f.db.get_public_ingress_entry("entry").unwrap().unwrap(),
        closed
    );
    f.spec.process_generation += 1;
    let restarted =
        f.db.prepare_public_ingress_entry(&f.spec, Some(closed.runtime_revision))
            .unwrap()
            .unwrap();
    assert_eq!(restarted.auth_epoch, closed.auth_epoch);
    assert!(f
        .db
        .set_public_ingress_admission(&restarted.fence(), true)
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&restarted.fence()))
        .unwrap());
    let (next_access, next_refresh) = f.tokens("configuration-old-refresh");
    assert!(matches!(
        f.db.rotate_oauth_refresh_token(
            &refresh.token_hash,
            "client",
            f.now,
            &next_access,
            &next_refresh
        )
        .unwrap(),
        RotateResult::Revoked
    ));
    f.issue(&restarted, "configuration-fresh");
}

#[test]
fn public_ingress_oauth_client_replacement_is_atomic_and_fenced() {
    let f = Fixture::new(PublicIngressMode::Named);
    let entry = f.activate();
    let mut replacement = f.client.clone();
    replacement.id = "replacement-row".into();
    replacement.client_id = "replacement-client".into();
    assert!(!f
        .db
        .insert_public_ingress_oauth_client(&replacement, &entry.fence(), false)
        .unwrap());
    assert!(f
        .db
        .get_oauth_client_by_client_id(&replacement.client_id)
        .unwrap()
        .is_none());
    let mut stale = entry.fence();
    stale.process_generation += 1;
    assert!(!f
        .db
        .insert_public_ingress_oauth_client(&replacement, &stale, true)
        .unwrap());
    for invalid in ["foreign", "project", "shared", "revoked"] {
        let mut candidate = replacement.clone();
        match invalid {
            "foreign" => candidate.owner_user_id = Some("foreign".into()),
            "project" => candidate.owner_project_grant_id = Some("project".into()),
            "shared" => candidate.owner_shared_key_hash = Some("shared".into()),
            _ => candidate.revoked_at = Some(f.now),
        }
        assert!(!f
            .db
            .insert_public_ingress_oauth_client(&candidate, &entry.fence(), true)
            .unwrap());
    }
    // Failure after client insertion must roll back both insertion and any
    // replacement side effect. No private listener can observe an unbound row.
    f.db.conn_for_tests().execute_batch("CREATE TRIGGER reject_public_client BEFORE INSERT ON oauth_public_ingress_clients BEGIN SELECT RAISE(ABORT, 'fixture binding failure'); END;").unwrap();
    assert!(f
        .db
        .insert_public_ingress_oauth_client(&replacement, &entry.fence(), true)
        .is_err());
    assert!(f
        .db
        .get_oauth_client_by_client_id(&replacement.client_id)
        .unwrap()
        .is_none());
    assert!(f
        .db
        .get_oauth_client_by_id(&f.client.id)
        .unwrap()
        .unwrap()
        .revoked_at
        .is_none());
    f.db.conn_for_tests()
        .execute_batch("DROP TRIGGER reject_public_client;")
        .unwrap();
    assert!(f
        .db
        .insert_public_ingress_oauth_client(&replacement, &entry.fence(), true)
        .unwrap());
    assert_eq!(
        f.db.oauth_client_public_ingress_entry_id(&replacement.client_id)
            .unwrap()
            .as_deref(),
        Some(entry.entry_id.as_str())
    );
    assert!(f
        .db
        .oauth_client_matches_public_ingress(&replacement.client_id, &entry.fence())
        .unwrap());
    assert!(f
        .db
        .get_oauth_client_by_id(&f.client.id)
        .unwrap()
        .unwrap()
        .revoked_at
        .is_some());
    assert!(!f
        .db
        .oauth_client_matches_public_ingress(&f.client.client_id, &entry.fence())
        .unwrap());
    assert!(f
        .db
        .insert_public_ingress_oauth_client(&replacement, &entry.fence(), true)
        .is_err());
    assert!(f
        .db
        .get_oauth_client_by_client_id(&replacement.client_id)
        .unwrap()
        .unwrap()
        .revoked_at
        .is_none());
}
