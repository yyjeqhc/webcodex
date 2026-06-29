//! Read-only audit query API.
//!
//! Three thin POST handlers mounted under `/api/audit/*` behind
//! `AuthMiddleware`. They wrap the existing action-session query functions in
//! `db.rs` and the decode/stats helpers in `action_sessions.rs`. They perform
//! no write operations and are intentionally **not** part of the GPT Actions
//! OpenAPI schema (`/openapi.json`).

use crate::action_sessions::{
    compute_stats, decode_event, sanitize_value_for_read, ActionEventView,
};
use crate::{get_db, json_error};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::json;

/// Default and maximum bounds for client-supplied `limit` values. The default
/// never exceeds 50 and the hard cap never exceeds 200, per the audit API
/// safety contract.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

/// Bounds for the recent-sessions scan performed by `/api/audit/stats` when no
/// `session_id` is supplied. Kept small to bound work; well within the limit
/// contract above.
const DEFAULT_STATS_SESSIONS: usize = 20;
const MAX_STATS_SESSIONS: usize = 50;
const STATS_EVENTS_PER_SESSION: usize = 200;
const STATS_SINGLE_SESSION_EVENTS: usize = 500;

fn clamp_limit(raw: Option<usize>, default: usize, max: usize) -> usize {
    raw.unwrap_or(default).clamp(1, max)
}

fn no_db(res: &mut Response) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "DB not available",
    ));
}

fn bad_json(res: &mut Response, e: &str) {
    res.status_code(StatusCode::BAD_REQUEST);
    res.render(json_error(
        StatusCode::BAD_REQUEST,
        format!("Invalid JSON: {}", e),
    ));
}

fn query_failed(res: &mut Response, e: &str) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Query failed: {}", e),
    ));
}

/// Apply the strict read-time sanitization pass to a decoded event so the
/// audit response never echoes secret-like keys or large raw payloads.
fn sanitize_event(mut ev: ActionEventView) -> ActionEventView {
    ev.ids = sanitize_value_for_read(&ev.ids);
    ev.summary = sanitize_value_for_read(&ev.summary);
    ev
}

#[derive(Debug, Deserialize)]
struct AuditSessionsRequest {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    status: Option<String>,
}

/// `POST /api/audit/sessions` — list recent action sessions (newest first).
///
/// Body: `{ "limit"?: number, "status"?: "open" | "closed" }`.
/// Returns `{ "sessions": [ActionSessionRecord, ...] }`. Session records carry
/// only metadata and aggregate counts; no secrets.
#[handler]
pub async fn audit_sessions(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        no_db(res);
        return;
    };
    let body: AuditSessionsRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            bad_json(res, &e.to_string());
            return;
        }
    };
    let limit = clamp_limit(body.limit, DEFAULT_LIMIT, MAX_LIMIT);
    let status = body
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let sessions = match db.list_action_sessions(status, limit) {
        Ok(s) => s,
        Err(e) => {
            query_failed(res, &e.to_string());
            return;
        }
    };
    res.render(Json(json!({ "sessions": sessions })));
}

#[derive(Debug, Deserialize)]
struct AuditSessionRequest {
    session_id: String,
    #[serde(default)]
    events_limit: Option<usize>,
}

/// `POST /api/audit/session` — fetch one session summary plus its events.
///
/// Body: `{ "session_id": string, "events_limit"?: number }`.
/// Returns `{ "session": ActionSessionRecord, "events": [ActionEventView] }`,
/// or `404` when the session id is unknown. Event `ids`/`summary` payloads are
/// passed through the strict read-time sanitizer.
#[handler]
pub async fn audit_session(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        no_db(res);
        return;
    };
    let body: AuditSessionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            bad_json(res, &e.to_string());
            return;
        }
    };
    let session_id = body.session_id.trim().to_string();
    if session_id.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            "session_id is required",
        ));
        return;
    }
    let session = match db.get_action_session(&session_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(json_error(StatusCode::NOT_FOUND, "session not found"));
            return;
        }
        Err(e) => {
            query_failed(res, &e.to_string());
            return;
        }
    };
    let events_limit = clamp_limit(body.events_limit, DEFAULT_LIMIT, MAX_LIMIT);
    let raw_events = match db.list_action_events(&session_id, events_limit) {
        Ok(e) => e,
        Err(e) => {
            query_failed(res, &e.to_string());
            return;
        }
    };
    let events: Vec<ActionEventView> = raw_events
        .into_iter()
        .map(decode_event)
        .map(sanitize_event)
        .collect();
    res.render(Json(json!({ "session": session, "events": events })));
}

#[derive(Debug, Deserialize)]
struct AuditStatsRequest {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

/// `POST /api/audit/stats` — aggregate `ActionSessionStats` over events.
///
/// Body: `{ "session_id"?: string, "limit"?: number }`.
/// When `session_id` is supplied, stats cover that single session's events
/// (capped internally). When omitted, stats cover the events of the `limit`
/// most recent sessions (default 20, max 50; each session capped at 200
/// events) to bound the scan. Returns the `ActionSessionStats` object.
#[handler]
pub async fn audit_stats(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        no_db(res);
        return;
    };
    let body: AuditStatsRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            bad_json(res, &e.to_string());
            return;
        }
    };

    let mut views: Vec<ActionEventView> = Vec::new();
    let scoped = body
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(session_id) = scoped {
        let raw = match db.list_action_events(session_id, STATS_SINGLE_SESSION_EVENTS) {
            Ok(e) => e,
            Err(e) => {
                query_failed(res, &e.to_string());
                return;
            }
        };
        for record in raw {
            views.push(sanitize_event(decode_event(record)));
        }
    } else {
        let limit = clamp_limit(body.limit, DEFAULT_STATS_SESSIONS, MAX_STATS_SESSIONS);
        let sessions = match db.list_action_sessions(None, limit) {
            Ok(s) => s,
            Err(e) => {
                query_failed(res, &e.to_string());
                return;
            }
        };
        for session in sessions {
            let Ok(raw) = db.list_action_events(&session.session_id, STATS_EVENTS_PER_SESSION)
            else {
                // Skip a session whose events cannot be read rather than
                // failing the whole aggregate.
                continue;
            };
            for record in raw {
                views.push(sanitize_event(decode_event(record)));
            }
        }
    }

    let stats = compute_stats(&views);
    res.render(Json(stats));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_sessions::{record_action_event, ActionAuditEventInput};
    use crate::Database;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_config(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: crate::CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        })
    }

    fn test_db() -> (tempfile::TempDir, Arc<Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn build_audit_router(config: Arc<crate::Config>, db: Arc<Database>) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("audit/sessions").post(audit_sessions))
                    .push(Router::with_path("audit/session").post(audit_session))
                    .push(Router::with_path("audit/stats").post(audit_stats)),
            )
    }

    fn effective_status(resp: &Response) -> StatusCode {
        resp.status_code.unwrap_or(StatusCode::OK)
    }

    fn seed_event(
        db: &Arc<Database>,
        session_id: &str,
        endpoint: &str,
        action_name: &str,
        status: &str,
        summary: Value,
    ) {
        record_action_event(
            db,
            ActionAuditEventInput {
                explicit_session_id: Some(session_id.to_string()),
                session_title: None,
                endpoint: endpoint.to_string(),
                action_name: action_name.to_string(),
                operation: Some("op".to_string()),
                project: Some("demo".to_string()),
                status: status.to_string(),
                http_status: Some(200),
                started_at: 1,
                ended_at: 2,
                duration_ms: 10,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary,
                request_bytes: None,
                response_bytes: None,
            },
        );
    }

    // =========================================================================
    // /api/audit/sessions
    // =========================================================================

    #[tokio::test]
    async fn http_audit_sessions_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_audit_router(config, db));

        let resp = TestClient::post("http://localhost/api/audit/sessions")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_audit_sessions_rejects_wrong_bearer() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_audit_router(config, db));

        let resp = TestClient::post("http://localhost/api/audit/sessions")
            .bearer_auth("wrong")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_audit_sessions_happy_path_returns_seeded_session() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_event(
            &db,
            "sess-a",
            "/api/codex/projects",
            "getCodexProjects",
            "success",
            json!({"project_count": 2}),
        );
        let service = Service::new(build_audit_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/audit/sessions")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        let sessions = body["sessions"].as_array().expect("sessions array");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["session_id"], "sess-a");
        assert_eq!(sessions[0]["status"], "open");
        assert_eq!(sessions[0]["total_actions"], 1);
    }

    #[tokio::test]
    async fn http_audit_sessions_limit_upper_cap_is_two_hundred() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        // Seed more sessions than the hard cap to prove the API never returns
        // more than 200.
        for i in 0..250 {
            seed_event(
                &db,
                &format!("cap-{}", i),
                "/api/codex/projects",
                "getCodexProjects",
                "success",
                json!({}),
            );
        }
        let service = Service::new(build_audit_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/audit/sessions")
            .bearer_auth("secret")
            .json(&json!({ "limit": 10000 }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        let sessions = body["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 200);
    }

    #[tokio::test]
    async fn http_audit_sessions_limit_lower_bound_is_one() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        for i in 0..3 {
            seed_event(
                &db,
                &format!("low-{}", i),
                "/api/codex/projects",
                "getCodexProjects",
                "success",
                json!({}),
            );
        }
        let service = Service::new(build_audit_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/audit/sessions")
            .bearer_auth("secret")
            .json(&json!({ "limit": 0 }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["sessions"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn http_audit_sessions_status_filter() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_event(
            &db,
            "open-1",
            "/api/codex/projects",
            "getCodexProjects",
            "success",
            json!({}),
        );
        seed_event(
            &db,
            "closed-1",
            "/api/codex/projects",
            "getCodexProjects",
            "success",
            json!({}),
        );
        db.close_action_session("closed-1", 100).unwrap();
        let service = Service::new(build_audit_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/audit/sessions")
            .bearer_auth("secret")
            .json(&json!({ "status": "closed" }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        let sessions = body["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["session_id"], "closed-1");
        assert_eq!(sessions[0]["status"], "closed");
    }

    // =========================================================================
    // /api/audit/session
    // =========================================================================

    #[tokio::test]
    async fn http_audit_session_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_audit_router(config, db));

        let resp = TestClient::post("http://localhost/api/audit/session")
            .json(&json!({ "session_id": "x" }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_audit_session_not_found() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_audit_router(config, db));

        let resp = TestClient::post("http://localhost/api/audit/session")
            .bearer_auth("secret")
            .json(&json!({ "session_id": "missing" }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn http_audit_session_happy_path_returns_session_and_events() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_event(
            &db,
            "sess-detail",
            "/api/codex/edit",
            "applyCodexEdit",
            "success",
            json!({"files_changed": 1}),
        );
        let service = Service::new(build_audit_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/audit/session")
            .bearer_auth("secret")
            .json(&json!({ "session_id": "sess-detail" }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["session"]["session_id"], "sess-detail");
        let events = body["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["endpoint"], "/api/codex/edit");
        assert_eq!(events[0]["summary"]["files_changed"], 1);
    }

    // =========================================================================
    // /api/audit/stats
    // =========================================================================

    #[tokio::test]
    async fn http_audit_stats_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let service = Service::new(build_audit_router(config, db));

        let resp = TestClient::post("http://localhost/api/audit/stats")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_audit_stats_happy_path_scoped_to_session() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_event(
            &db,
            "stats-1",
            "/api/codex/job",
            "runCodexJob",
            "success",
            json!({}),
        );
        seed_event(
            &db,
            "stats-1",
            "/api/codex/edit",
            "applyCodexEdit",
            "failed",
            json!({}),
        );
        let service = Service::new(build_audit_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/audit/stats")
            .bearer_auth("secret")
            .json(&json!({ "session_id": "stats-1" }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["by_endpoint"]["/api/codex/job"], 1);
        assert_eq!(body["by_endpoint"]["/api/codex/edit"], 1);
        assert_eq!(body["by_status"]["success"], 1);
        assert_eq!(body["by_status"]["failed"], 1);
        assert_eq!(body["job_count"], 1);
        assert_eq!(body["edit_count"], 1);
    }

    #[tokio::test]
    async fn http_audit_stats_global_over_recent_sessions() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        seed_event(
            &db,
            "g-1",
            "/api/codex/git",
            "getGitStatus",
            "success",
            json!({}),
        );
        seed_event(
            &db,
            "g-2",
            "/api/codex/report",
            "getReport",
            "success",
            json!({}),
        );
        let service = Service::new(build_audit_router(config, db));

        let mut resp = TestClient::post("http://localhost/api/audit/stats")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["by_endpoint"]["/api/codex/git"], 1);
        assert_eq!(body["by_endpoint"]["/api/codex/report"], 1);
        assert_eq!(body["git_count"], 1);
        assert_eq!(body["report_count"], 1);
    }

    // =========================================================================
    // Secret leakage
    // =========================================================================

    #[tokio::test]
    async fn http_audit_responses_do_not_leak_secret_fields_or_values() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        // Seed a payload that contains secret-like keys and secret-like values
        // at write time. The write path already redacts, and the audit read
        // path must drop the keys entirely.
        seed_event(
            &db,
            "leak-1",
            "/api/codex/edit",
            "applyCodexEdit",
            "success",
            json!({
                "api_key": "sk-leak-12345",
                "password": "hunter2",
                "token": "bearer abc",
                "project_count": 7,
                "stdout": "should-be-stripped",
                "command_text": "echo hello"
            }),
        );
        let service = Service::new(build_audit_router(config, db.clone()));

        for (path, body) in [
            ("/api/audit/sessions", json!({ "limit": 10 })),
            ("/api/audit/session", json!({ "session_id": "leak-1" })),
            ("/api/audit/stats", json!({ "session_id": "leak-1" })),
        ] {
            let mut resp = TestClient::post(&format!("http://localhost{}", path))
                .bearer_auth("secret")
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(effective_status(&resp), StatusCode::OK, "path {}", path);
            let text = resp.take_string().await.unwrap();
            let lower = text.to_lowercase();
            for forbidden in ["sk-leak-12345", "hunter2", "should-be-stripped"] {
                assert!(
                    !lower.contains(&forbidden.to_lowercase()),
                    "path {} leaked value '{}': {}",
                    path,
                    forbidden,
                    text
                );
            }
            for forbidden in ["api_key", "password", "token", "secret"] {
                assert!(
                    !lower.contains(&forbidden.to_lowercase()),
                    "path {} leaked secret field name '{}': {}",
                    path,
                    forbidden,
                    text
                );
            }
        }

        // Legitimate non-secret fields still pass through.
        let mut resp = TestClient::post("http://localhost/api/audit/session")
            .bearer_auth("secret")
            .json(&json!({ "session_id": "leak-1" }))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["events"][0]["summary"]["project_count"], 7);
        // command_text is summarized into an object (hash + bounded preview),
        // never stored as the raw string.
        let cmd = &body["events"][0]["summary"]["command_text"];
        assert!(
            cmd.is_object(),
            "command_text must be summarized, got: {}",
            cmd
        );
        assert!(cmd["sha256"].is_string());
        // A non-secret one-liner keeps a short first-line preview by design.
        assert_eq!(cmd["first_line_preview"], "echo hello");

        // A secret-like command value is redacted in the preview and never
        // echoed verbatim.
        seed_event(
            &db,
            "leak-2",
            "/api/codex/edit",
            "applyCodexEdit",
            "success",
            json!({ "command_text": "token=cmd-secret-xyz" }),
        );
        let mut resp = TestClient::post("http://localhost/api/audit/session")
            .bearer_auth("secret")
            .json(&json!({ "session_id": "leak-2" }))
            .send(&service)
            .await;
        let body: Value = resp.take_json().await.unwrap();
        let cmd = &body["events"][0]["summary"]["command_text"];
        assert!(cmd.is_object());
        assert_eq!(cmd["first_line_preview"], "[redacted]");
        assert!(!body.to_string().contains("cmd-secret-xyz"));
    }
}
