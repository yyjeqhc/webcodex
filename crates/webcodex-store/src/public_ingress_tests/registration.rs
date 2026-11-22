use super::*;
use crate::models::{DynamicClientAuth, DynamicClientRegistration};
use crate::DynamicRegistrationOutcome::{
    AtCapacity, Created, IngressUnavailable, OwnerUnavailable,
};

impl Fixture {
    fn registration(&self, id: &str) -> OAuthClientRecord {
        OAuthClientRecord {
            id: format!("row-{id}"),
            client_id: id.into(),
            owner_user_id: Some(self.spec.owner_user_id.clone()),
            dynamic_registration: Some(DynamicClientRegistration {
                issuer: self.spec.origin.clone(),
                auth_method: DynamicClientAuth::Post,
                refresh: true,
            }),
            ..self.client.clone()
        }
    }

    fn restart(&mut self, previous: &PublicIngressEntry) -> PublicIngressEntry {
        self.spec.process_generation += 1;
        let entry = self
            .db
            .prepare_public_ingress_entry(&self.spec, Some(previous.runtime_revision))
            .unwrap()
            .unwrap();
        assert!(self
            .db
            .set_public_ingress_admission(&entry.fence(), true)
            .unwrap());
        self.db
            .get_public_ingress_entry(&entry.entry_id)
            .unwrap()
            .unwrap()
    }
}

#[test]
fn stale_fences_invalid_metadata_and_disabled_owners_create_no_registration() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    let disabled = f.activate();
    let record = f.registration("new");
    assert_eq!(
        f.db.register_dynamic_oauth_client(&record, Some(&disabled.fence()))
            .unwrap(),
        IngressUnavailable
    );
    f.spec.oauth_dynamic_registration_enabled = true;
    let live = f.restart(&disabled);
    assert_eq!(
        f.db.register_dynamic_oauth_client(&record, Some(&disabled.fence()))
            .unwrap(),
        IngressUnavailable
    );
    let mut wrong = record.clone();
    wrong.dynamic_registration.as_mut().unwrap().issuer = "https://other.example".into();
    assert_eq!(
        f.db.register_dynamic_oauth_client(&wrong, Some(&live.fence()))
            .unwrap(),
        IngressUnavailable
    );
    let mut other = f.db.get_user_by_id("owner").unwrap().unwrap();
    other.id = "other".into();
    other.username = "other".into();
    f.db.create_user(&other).unwrap();
    wrong = record.clone();
    wrong.owner_user_id = Some(other.id);
    assert_eq!(
        f.db.register_dynamic_oauth_client(&wrong, Some(&live.fence()))
            .unwrap(),
        IngressUnavailable
    );
    wrong = record.clone();
    wrong.allowed_scopes = "account:manage".into();
    assert!(f
        .db
        .register_dynamic_oauth_client(&wrong, Some(&live.fence()))
        .is_err());
    wrong = record.clone();
    wrong.redirect_uris = "http://remote.example".into();
    assert!(f
        .db
        .register_dynamic_oauth_client(&wrong, Some(&live.fence()))
        .is_err());
    f.db.set_user_disabled("owner", true, f.now).unwrap();
    assert_eq!(
        f.db.register_dynamic_oauth_client(&record, Some(&live.fence()))
            .unwrap(),
        OwnerUnavailable
    );
    assert!(f.db.get_oauth_client_by_client_id("new").unwrap().is_none());
}

#[test]
fn two_registration_writers_share_entry_quota_and_failed_bind_rolls_back_client() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    f.spec.oauth_dynamic_registration_enabled = true;
    let entry = f.activate();
    let fence = entry.fence();
    f.db.conn_for_tests().execute_batch("CREATE TEMP TRIGGER fail_binding BEFORE INSERT ON oauth_public_ingress_clients BEGIN SELECT RAISE(ABORT,'test rollback'); END").unwrap();
    assert!(f
        .db
        .register_dynamic_oauth_client(&f.registration("rollback"), Some(&fence))
        .is_err());
    assert!(f
        .db
        .get_oauth_client_by_client_id("rollback")
        .unwrap()
        .is_none());
    f.db.conn_for_tests()
        .execute_batch("DROP TRIGGER fail_binding")
        .unwrap();
    for i in 0..15 {
        assert_eq!(
            f.db.register_dynamic_oauth_client(&f.registration(&format!("dcr-{i}")), Some(&fence))
                .unwrap(),
            Created
        );
    }
    let other = Database::open(&f._dir.path().join("ingress.sqlite")).unwrap();
    let barrier = Barrier::new(2);
    let candidate = f.registration("second-writer");
    std::thread::scope(|scope| {
        let thread = scope.spawn(|| {
            barrier.wait();
            other
                .register_dynamic_oauth_client(&candidate, Some(&fence))
                .unwrap()
        });
        barrier.wait();
        let first =
            f.db.register_dynamic_oauth_client(&f.registration("first-writer"), Some(&fence))
                .unwrap();
        let second = thread.join().unwrap();
        assert!(matches!(
            (first, second),
            (Created, AtCapacity) | (AtCapacity, Created)
        ));
    });
    {
        let conn = f.db.conn_for_tests();
        let counts: (i64,i64) = conn.query_row("SELECT count(*),count(b.client_id) FROM oauth_clients c LEFT JOIN oauth_public_ingress_clients b ON b.client_id=c.client_id AND b.entry_id=? AND b.registration_epoch=? WHERE c.dynamic_registration IS NOT NULL", rusqlite::params![entry.entry_id,entry.auth_epoch],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(counts, (16, 16));
    }
    assert_eq!(
        f.db.register_dynamic_oauth_client(&f.registration("private"), None)
            .unwrap(),
        Created
    );
    assert!(!f
        .db
        .bind_oauth_client_to_public_ingress("private", &fence)
        .unwrap());
    f.db.revoke_oauth_client_by_client_id("dcr-1", f.now)
        .unwrap();
    assert_eq!(
        f.db.register_dynamic_oauth_client(&f.registration("reclaimed"), Some(&fence))
            .unwrap(),
        Created
    );
    // The manual provisioning UI retains its one manual client alongside DCR.
    assert_eq!(
        f.db.list_manual_oauth_clients_for_public_ingress(&entry.entry_id)
            .unwrap()
            .len(),
        1
    );
    let manual = OAuthClientRecord {
        id: "manual-replacement".into(),
        client_id: "manual-replacement".into(),
        ..f.client.clone()
    };
    assert!(f
        .db
        .insert_public_ingress_oauth_client(&manual, &fence, true)
        .unwrap());
    assert!(f
        .db
        .oauth_client_matches_public_ingress("reclaimed", &fence)
        .unwrap());
}

#[test]
fn quick_reconnect_releases_registration_slots_without_reviving_old_grants() {
    let mut f = Fixture::new(PublicIngressMode::Quick);
    f.spec.oauth_dynamic_registration_enabled = true;
    let original = f.activate();
    for i in 0..16 {
        assert_eq!(
            f.db.register_dynamic_oauth_client(
                &f.registration(&format!("quick-old-{i}")),
                Some(&original.fence()),
            )
            .unwrap(),
            Created
        );
    }
    f.client = f.registration("quick-old-0");
    let (old_access, _) = f.issue(&original, "quick-old-grant");
    assert_eq!(
        f.db.register_dynamic_oauth_client(
            &f.registration("old-overflow"),
            Some(&original.fence())
        )
        .unwrap(),
        AtCapacity
    );
    assert!(f
        .db
        .set_public_ingress_admission(&original.fence(), false)
        .unwrap());
    let new_entry = f.restart(&original);
    assert_ne!(new_entry.auth_epoch, original.auth_epoch);
    assert!(!f
        .db
        .oauth_client_matches_public_ingress("quick-old-0", &new_entry.fence())
        .unwrap());
    assert!(!f
        .db
        .oauth_access_token_matches_public_ingress(&old_access.token_hash, Some(&new_entry.fence()))
        .unwrap());

    // Old registrations are still retained for audit and the global capacity,
    // but they must not occupy the new Quick attempt's per-entry slots.
    for i in 0..16 {
        assert_eq!(
            f.db.register_dynamic_oauth_client(
                &f.registration(&format!("quick-new-{i}")),
                Some(&new_entry.fence()),
            )
            .unwrap(),
            Created
        );
    }
    assert_eq!(
        f.db.register_dynamic_oauth_client(
            &f.registration("new-overflow"),
            Some(&new_entry.fence())
        )
        .unwrap(),
        AtCapacity
    );
    assert!(!f
        .db
        .oauth_client_matches_public_ingress("quick-old-0", &new_entry.fence())
        .unwrap());
}

#[test]
fn quick_epoch_slots_do_not_bypass_global_registration_capacity() {
    let mut f = Fixture::new(PublicIngressMode::Quick);
    f.spec.oauth_dynamic_registration_enabled = true;
    let mut entry = f.activate();
    // Eight generations can each allocate 16 current-epoch registrations,
    // but historical records must still count toward the global 128 cap.
    for generation in 0..8 {
        for i in 0..16 {
            assert_eq!(
                f.db.register_dynamic_oauth_client(
                    &f.registration(&format!("quick-{generation}-{i}")),
                    Some(&entry.fence()),
                )
                .unwrap(),
                Created
            );
        }
        assert!(f
            .db
            .set_public_ingress_admission(&entry.fence(), false)
            .unwrap());
        entry = f.restart(&entry);
    }
    assert_eq!(
        f.db.register_dynamic_oauth_client(
            &f.registration("global-overflow"),
            Some(&entry.fence()),
        )
        .unwrap(),
        AtCapacity
    );
    assert!(f
        .db
        .get_oauth_client_by_client_id("global-overflow")
        .unwrap()
        .is_none());
}

#[test]
fn named_and_quick_dynamic_clients_follow_entry_epoch_even_when_owner_and_hostname_repeat() {
    for mode in [PublicIngressMode::Named, PublicIngressMode::Quick] {
        let mut f = Fixture::new(mode);
        f.spec.oauth_dynamic_registration_enabled = true;
        let entry = f.activate();
        f.client = f.registration("gemini");
        assert_eq!(
            f.db.register_dynamic_oauth_client(&f.client, Some(&entry.fence()))
                .unwrap(),
            Created
        );
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
        let next = f.restart(&entry);
        assert!(!f
            .db
            .set_public_ingress_admission(&entry.fence(), true)
            .unwrap());
        let preserved = mode == PublicIngressMode::Named;
        assert_eq!(
            f.db.oauth_client_matches_public_ingress(&f.client.client_id, &next.fence())
                .unwrap(),
            preserved
        );
        assert_eq!(
            f.db.oauth_access_token_matches_public_ingress(&access.token_hash, Some(&next.fence()))
                .unwrap(),
            preserved
        );
        let (a, r) = f.tokens("new");
        for wrong in [None, Some(entry.fence())] {
            assert!(f
                .db
                .exchange_oauth_authorization_code_for_tokens(
                    wrong.as_ref(),
                    &pending.code_hash,
                    f.now,
                    &a,
                    &r
                )
                .unwrap()
                .is_none());
            assert!(matches!(
                f.db.rotate_oauth_refresh_token(
                    wrong.as_ref(),
                    &refresh.token_hash,
                    &f.client.client_id,
                    f.now,
                    &a,
                    &r
                )
                .unwrap(),
                RotateResult::Revoked
            ));
        }
        assert_eq!(
            matches!(
                f.db.rotate_oauth_refresh_token(
                    Some(&next.fence()),
                    &refresh.token_hash,
                    &f.client.client_id,
                    f.now,
                    &a,
                    &r
                )
                .unwrap(),
                RotateResult::Rotated(_)
            ),
            preserved
        );
        let (a, r) = f.tokens("pending");
        assert_eq!(
            f.db.exchange_oauth_authorization_code_for_tokens(
                Some(&next.fence()),
                &pending.code_hash,
                f.now,
                &a,
                &r
            )
            .unwrap()
            .is_some(),
            preserved
        );
    }
}

#[test]
fn named_registration_cannot_revive_after_policy_owner_origin_or_profile_replacement() {
    for change in ["disable", "owner", "origin", "recreate"] {
        let mut f = Fixture::new(PublicIngressMode::Named);
        f.spec.oauth_dynamic_registration_enabled = true;
        f.db.reconcile_public_ingress_configuration("profile", "incarnation-one")
            .unwrap();
        let entry = f.activate();
        f.client = f.registration("first");
        assert_eq!(
            f.db.register_dynamic_oauth_client(&f.client, Some(&entry.fence()))
                .unwrap(),
            Created
        );
        let (access, _) = f.issue(&entry, "first");
        let original = f.spec.clone();
        match change {
            "disable" => f.spec.oauth_dynamic_registration_enabled = false,
            "origin" => f.spec.origin = "https://changed.example".into(),
            "owner" => {
                let mut other = f.db.get_user_by_id("owner").unwrap().unwrap();
                other.id = "other".into();
                other.username = "other".into();
                f.db.create_user(&other).unwrap();
                f.spec.owner_user_id = other.id;
            }
            _ => {
                f.db.reconcile_public_ingress_configuration("profile", "incarnation-two")
                    .unwrap()
            }
        }
        let changed = f.restart(&entry);
        f.spec = PublicIngressEntrySpec {
            process_generation: f.spec.process_generation,
            ..original
        };
        let restored = f.restart(&changed);
        assert!(
            !f.db
                .oauth_client_matches_public_ingress("first", &restored.fence())
                .unwrap(),
            "{change}"
        );
        assert!(
            !f.db
                .oauth_access_token_matches_public_ingress(
                    &access.token_hash,
                    Some(&restored.fence())
                )
                .unwrap(),
            "{change}"
        );
        assert_eq!(
            f.db.register_dynamic_oauth_client(
                &f.registration("replacement"),
                Some(&restored.fence())
            )
            .unwrap(),
            Created
        );
    }
}

#[test]
fn upgrading_pre_dcr_ingress_preserves_manual_authorization_and_defaults_off() {
    let f = Fixture::new(PublicIngressMode::Named);
    let entry = f.activate();
    let (access, _) = f.issue(&entry, "legacy");
    let path = f._dir.path().join("ingress.sqlite");
    f.db.conn_for_tests()
        .execute_batch(
            "ALTER TABLE oauth_public_ingress_clients DROP COLUMN registration_epoch;
        ALTER TABLE public_ingress_entries DROP COLUMN oauth_dynamic_registration_enabled;",
        )
        .unwrap();
    for _ in 0..2 {
        let reopened = Database::open(&path).unwrap();
        assert!(!reopened
            .public_ingress_registration_available(&entry.fence())
            .unwrap());
        assert!(reopened
            .oauth_client_matches_public_ingress("client", &entry.fence())
            .unwrap());
        assert!(reopened
            .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&entry.fence()))
            .unwrap());
    }
}

#[test]
fn prepared_but_unapplied_policy_cannot_retire_named_client() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    f.spec.oauth_dynamic_registration_enabled = true;
    let live = f.activate();
    f.client = f.registration("retained");
    assert_eq!(
        f.db.register_dynamic_oauth_client(&f.client, Some(&live.fence()))
            .unwrap(),
        Created
    );
    let (access, _) = f.issue(&live, "retained");
    f.spec.process_generation += 1;
    f.spec.oauth_dynamic_registration_enabled = false;
    let pending =
        f.db.prepare_public_ingress_entry(&f.spec, Some(live.runtime_revision))
            .unwrap()
            .unwrap();
    f.spec.oauth_dynamic_registration_enabled = true;
    let next = f.restart(&pending);
    assert!(!f
        .db
        .set_public_ingress_admission(&pending.fence(), true)
        .unwrap());
    assert!(f
        .db
        .oauth_access_token_matches_public_ingress(&access.token_hash, Some(&next.fence()))
        .unwrap());
}

#[test]
fn dynamic_code_writer_checks_current_callback_scope_and_pkce_before_issuance() {
    let mut f = Fixture::new(PublicIngressMode::Named);
    f.spec.oauth_dynamic_registration_enabled = true;
    let entry = f.activate();
    f.client = f.registration("narrow");
    assert_eq!(
        f.db.register_dynamic_oauth_client(&f.client, Some(&entry.fence()))
            .unwrap(),
        Created
    );
    for field in ["scope", "callback", "admin", "pkce"] {
        let mut code = f.code(field);
        match field {
            "scope" => code.scopes = "project:write".into(),
            "callback" => code.redirect_uri = "https://other.example".into(),
            "admin" => code.admin_authority = true,
            _ => code.code_challenge = None,
        }
        assert!(!f
            .db
            .insert_public_ingress_oauth_authorization_code(&code, &code.code_hash, &entry.fence())
            .unwrap());
        assert!(f
            .db
            .get_oauth_authorization_code_by_hash(&code.code_hash)
            .unwrap()
            .is_none());
    }
}
