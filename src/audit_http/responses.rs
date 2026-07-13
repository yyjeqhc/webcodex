use crate::action_audit_sessions::{sanitize_value_for_read, ActionEventView};
use crate::json_error;
use salvo::prelude::*;

pub(super) fn no_db(res: &mut Response) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "DB not available",
    ));
}

pub(super) fn bad_json(res: &mut Response, e: &str) {
    res.status_code(StatusCode::BAD_REQUEST);
    res.render(json_error(
        StatusCode::BAD_REQUEST,
        format!("Invalid JSON: {}", e),
    ));
}

pub(super) fn query_failed(res: &mut Response, e: &str) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Query failed: {}", e),
    ));
}

pub(super) fn bad_request(res: &mut Response, message: &str) {
    res.status_code(StatusCode::BAD_REQUEST);
    res.render(json_error(StatusCode::BAD_REQUEST, message));
}

pub(super) fn not_found(res: &mut Response, message: &str) {
    res.status_code(StatusCode::NOT_FOUND);
    res.render(json_error(StatusCode::NOT_FOUND, message));
}

/// Apply the strict read-time sanitization pass to a decoded event so the
/// audit response never echoes secret-like keys or large raw payloads.
pub(super) fn sanitize_event(mut ev: ActionEventView) -> ActionEventView {
    ev.ids = sanitize_value_for_read(&ev.ids);
    ev.summary = sanitize_value_for_read(&ev.summary);
    ev
}
