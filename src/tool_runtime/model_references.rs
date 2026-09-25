use super::session_context::{
    workflow_session_authority_fingerprint, workflow_session_incarnation_fingerprint,
};
use super::ToolRuntime;
use crate::auth::AuthContext;
use serde_json::Value;
use webcodex_core::model_reference::{
    format_model_reference, parse_model_reference, ModelReferenceKind,
};

impl ToolRuntime {
    pub(crate) fn session_reference_for_id(
        &self,
        session_id: &str,
        auth: Option<&AuthContext>,
    ) -> Option<String> {
        let db = self.project_reference_db.as_ref()?;
        let principal_key = workflow_session_authority_fingerprint(auth).ok()?;
        let (project, owner_authority_fingerprint) =
            self.sessions.session_target_authority(session_id)?;
        if owner_authority_fingerprint != principal_key {
            return None;
        }
        let summary = self.sessions.summary(session_id, Some(1))?;
        let incarnation_fingerprint = workflow_session_incarnation_fingerprint(
            session_id,
            summary.created_at,
            project.as_deref(),
            &owner_authority_fingerprint,
        );
        let record = db
            .get_or_create_model_reference(
                &principal_key,
                ModelReferenceKind::Session,
                session_id,
                &incarnation_fingerprint,
                chrono::Utc::now().timestamp(),
            )
            .ok()?;
        Some(format_model_reference(
            ModelReferenceKind::Session,
            record.ref_index,
        ))
    }

    fn resolve_session_reference(
        &self,
        raw: &str,
        auth: Option<&AuthContext>,
    ) -> Option<Result<String, ()>> {
        let ref_index = match parse_model_reference(raw, ModelReferenceKind::Session)? {
            Ok(ref_index) => ref_index,
            Err(()) => return Some(Err(())),
        };
        let db = match self.project_reference_db.as_ref() {
            Some(db) => db,
            None => return Some(Err(())),
        };
        let principal_key = match workflow_session_authority_fingerprint(auth) {
            Ok(principal_key) => principal_key,
            Err(_) => return Some(Err(())),
        };
        let record = match db
            .lookup_model_reference(&principal_key, ModelReferenceKind::Session, ref_index)
            .ok()
            .flatten()
        {
            Some(record) => record,
            None => return Some(Err(())),
        };
        let (project, owner_authority_fingerprint) =
            match self.sessions.session_target_authority(&record.canonical_id) {
                Some(target) => target,
                None => return Some(Err(())),
            };
        if owner_authority_fingerprint != principal_key {
            return Some(Err(()));
        }
        let summary = match self.sessions.summary(&record.canonical_id, Some(1)) {
            Some(summary) => summary,
            None => return Some(Err(())),
        };
        let current_incarnation = workflow_session_incarnation_fingerprint(
            &record.canonical_id,
            summary.created_at,
            project.as_deref(),
            &owner_authority_fingerprint,
        );
        if current_incarnation != record.incarnation_fingerprint {
            return Some(Err(()));
        }
        Some(Ok(record.canonical_id))
    }

    /// Canonicalize only the concrete business Session selector. Wrapper
    /// recording_session_id is stripped by the transport adapter and never
    /// enters this path.
    pub(crate) fn canonicalize_session_reference_argument(
        &self,
        arguments: &mut Value,
        auth: Option<&AuthContext>,
    ) -> Result<(), String> {
        let Some(object) = arguments.as_object_mut() else {
            return Ok(());
        };
        let Some(raw) = object.get("session_id").and_then(Value::as_str) else {
            return Ok(());
        };
        let Some(resolved) = self.resolve_session_reference(raw, auth) else {
            return Ok(());
        };
        let canonical = resolved.map_err(|_| {
            format!(
                "unknown_session_ref: {raw}; reuse a currently issued session_ref or the canonical wc_sess_* id"
            )
        })?;
        object.insert("session_id".to_string(), Value::String(canonical));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_runtime::sessions::{SessionCreateOptions, SessionGuards};
    use crate::tool_runtime::SessionMode;
    use std::sync::Arc;

    #[test]
    fn malformed_session_ref_fails_without_touching_canonical_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = ToolRuntime::new_for_tests().with_project_reference_database(Arc::new(
            crate::Database::open(&tmp.path().join("session-refs.db")).unwrap(),
        ));
        let mut canonical = serde_json::json!({"session_id": "wc_sess_exact"});
        runtime
            .canonicalize_session_reference_argument(&mut canonical, None)
            .unwrap();
        assert_eq!(canonical["session_id"], "wc_sess_exact");

        let mut malformed = serde_json::json!({"session_id": "~s01"});
        assert!(runtime
            .canonicalize_session_reference_argument(&mut malformed, None)
            .unwrap_err()
            .contains("unknown_session_ref"));
    }

    #[test]
    fn session_refs_are_stable_for_the_same_principal() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = ToolRuntime::new_for_tests().with_project_reference_database(Arc::new(
            crate::Database::open(&tmp.path().join("session-refs.db")).unwrap(),
        ));
        let authority = workflow_session_authority_fingerprint(None).unwrap();
        let summary = runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    None,
                    Some("ref test".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(authority)),
            )
            .unwrap();
        let session_ref = runtime
            .session_reference_for_id(&summary.session_id, None)
            .unwrap();
        assert!(session_ref.starts_with("~s"));
        assert_eq!(
            runtime
                .session_reference_for_id(&summary.session_id, None)
                .unwrap(),
            session_ref
        );

        let mut arguments = serde_json::json!({"session_id": session_ref});
        runtime
            .canonicalize_session_reference_argument(&mut arguments, None)
            .unwrap();
        assert_eq!(arguments["session_id"], summary.session_id);
    }

    #[test]
    fn closed_session_ref_still_resolves_before_existing_lifecycle_checks() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = ToolRuntime::new_for_tests().with_project_reference_database(Arc::new(
            crate::Database::open(&tmp.path().join("session-refs.db")).unwrap(),
        ));
        let authority = workflow_session_authority_fingerprint(None).unwrap();
        let summary = runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    None,
                    Some("closed ref".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(authority)),
            )
            .unwrap();
        let session_ref = runtime
            .session_reference_for_id(&summary.session_id, None)
            .unwrap();
        runtime.sessions.close_session(&summary.session_id).unwrap();

        let mut arguments = serde_json::json!({"session_id": session_ref});
        runtime
            .canonicalize_session_reference_argument(&mut arguments, None)
            .unwrap();
        assert_eq!(arguments["session_id"], summary.session_id);
        assert_eq!(
            runtime.sessions.lifecycle_state(&summary.session_id),
            Some(crate::tool_runtime::sessions::SessionLifecycle::Closed),
            "the ref resolver must not invent a stricter lifecycle than canonical session_id"
        );
    }

    #[test]
    fn session_refs_fail_closed_for_another_principal() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = ToolRuntime::new_for_tests().with_project_reference_database(Arc::new(
            crate::Database::open(&tmp.path().join("session-refs.db")).unwrap(),
        ));
        let mut alice = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
        alice.user_id = Some("user-alice".to_string());
        alice.username = Some("alice".to_string());
        alice.api_key_id = Some("key-alice".to_string());
        alice.token_kind = Some("user".to_string());

        let mut bob = crate::auth::AuthContext::new(crate::auth::AuthKind::ApiToken);
        bob.user_id = Some("user-bob".to_string());
        bob.username = Some("bob".to_string());
        bob.api_key_id = Some("key-bob".to_string());
        bob.token_kind = Some("user".to_string());
        let authority = workflow_session_authority_fingerprint(Some(&alice)).unwrap();
        let summary = runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    None,
                    Some("alice ref".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(authority)),
            )
            .unwrap();
        let session_ref = runtime
            .session_reference_for_id(&summary.session_id, Some(&alice))
            .unwrap();

        let mut arguments = serde_json::json!({"session_id": session_ref.clone()});
        let error = runtime
            .canonicalize_session_reference_argument(&mut arguments, Some(&bob))
            .unwrap_err();
        assert!(error.contains("unknown_session_ref"));
        assert_eq!(arguments["session_id"], session_ref);
    }

    #[test]
    fn durable_session_ref_never_retargets_after_runtime_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("session-refs.db");
        let original_ref = {
            let runtime = ToolRuntime::new_for_tests().with_project_reference_database(Arc::new(
                crate::Database::open(&db_path).unwrap(),
            ));
            let authority = workflow_session_authority_fingerprint(None).unwrap();
            let summary = runtime
                .sessions
                .start_session_with_options(
                    SessionCreateOptions::new(
                        None,
                        Some("original".to_string()),
                        SessionMode::Normal,
                        SessionGuards::default(),
                    )
                    .with_owner_authority_fingerprint(Some(authority)),
                )
                .unwrap();
            runtime
                .session_reference_for_id(&summary.session_id, None)
                .unwrap()
        };

        // Reopen the durable reference store with a fresh SessionStore. The
        // mapping survives, while the referenced Session does not; fail closed.
        let runtime = ToolRuntime::new_for_tests()
            .with_project_reference_database(Arc::new(crate::Database::open(&db_path).unwrap()));
        let mut stale = serde_json::json!({"session_id": original_ref.clone()});
        assert!(runtime
            .canonicalize_session_reference_argument(&mut stale, None)
            .is_err());

        // Creating another Session must not make the old selector point at it.
        let authority = workflow_session_authority_fingerprint(None).unwrap();
        let replacement = runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    None,
                    Some("replacement".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(authority)),
            )
            .unwrap();
        assert_ne!(stale["session_id"], replacement.session_id);
        assert!(runtime
            .canonicalize_session_reference_argument(&mut stale, None)
            .is_err());
        let replacement_ref = runtime
            .session_reference_for_id(&replacement.session_id, None)
            .unwrap();
        assert_ne!(replacement_ref, original_ref);
    }
}
