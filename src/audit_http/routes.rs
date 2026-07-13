use super::responses::{bad_json, bad_request, no_db, not_found, query_failed, sanitize_event};
use crate::action_audit_sessions::{compute_stats, decode_event, ActionEventView};
use crate::get_db;
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
        bad_request(res, "session_id is required");
        return;
    }
    let session = match db.get_action_session(&session_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            not_found(res, "session not found");
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
