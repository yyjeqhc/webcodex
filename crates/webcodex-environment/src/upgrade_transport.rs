//! Core's private, recoverable handle for the Server upgrade admission fence.
//! The lease token is never written to the ordinary upgrade journal or logs.
use crate::storage::{atomic_private_write, read_private};
use crate::{read_secret, EnvironmentRecord, EnvironmentStore, SetupDiagnostic, SetupResultValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const ROUTE: &str = "/api/runtime/upgrade-maintenance";
const CONTRACT: &str = "webcodex_upgrade_maintenance_v1";
const FILE: &str = "upgrade-maintenance.secret";

#[derive(Serialize, Deserialize)]
struct SavedLease {
    server_url: String,
    server_epoch: Option<String>,
    client_id: Option<String>,
    operation_nonce: String,
    token: Option<String>,
}

fn diagnostic(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Keep the running service and current installation; use the explicit stop flow if the Server cannot fence new work")
}

fn valid_token(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn call(
    record: &EnvironmentRecord,
    store: &EnvironmentStore,
    body: Value,
) -> SetupResultValue<(u16, Value)> {
    let token = if record.request.local_server() {
        crate::native::bootstrap_token(store)?
    } else {
        read_secret(&store.root().join("webcodex-user-token"))?
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| {
            diagnostic(
                "upgrade_transport",
                "Could not establish the maintenance transport",
            )
        })?;
    let mut response = client
        .post(format!("{}{ROUTE}", record.request.server_url))
        .bearer_auth(token.expose())
        .json(&body)
        .send()
        .await
        .map_err(|_| {
            diagnostic(
                "upgrade_transport",
                "The Server maintenance response is unavailable",
            )
        })?;
    let status = response.status().as_u16();
    if status == 404 || (300..400).contains(&status) {
        return Ok((status, Value::Null));
    }
    if response
        .content_length()
        .is_some_and(|length| length > 4096)
    {
        return Err(diagnostic(
            "upgrade_transport",
            "The Server maintenance response exceeded its bound",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        diagnostic(
            "upgrade_transport",
            "The Server maintenance response was interrupted",
        )
    })? {
        if bytes.len().saturating_add(chunk.len()) > 4096 {
            return Err(diagnostic(
                "upgrade_transport",
                "The Server maintenance response exceeded its bound",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| {
        diagnostic(
            "upgrade_transport",
            "The Server maintenance response was invalid",
        )
    })?;
    Ok((status, value))
}

fn contract(value: &Value) -> SetupResultValue<()> {
    if value.get("contract").and_then(Value::as_str) == Some(CONTRACT) {
        Ok(())
    } else {
        Err(diagnostic("upgrade_fence_unsupported", "The Server does not support safe upgrade admission; stop services explicitly or upgrade the Server first"))
    }
}

fn saved(store: &EnvironmentStore) -> SetupResultValue<Option<SavedLease>> {
    let path = store.root().join(FILE);
    if !path.exists() {
        return Ok(None);
    }
    let data = read_private(&path)?;
    let lease: SavedLease = serde_json::from_slice(&data).map_err(|_| {
        diagnostic(
            "upgrade_lease_invalid",
            "The protected maintenance lease is invalid",
        )
    })?;
    if !valid_token(&lease.operation_nonce)
        || lease
            .token
            .as_deref()
            .is_some_and(|token| !valid_token(token))
        || lease.server_epoch.as_deref().is_some_and(str::is_empty)
        || lease.token.is_some() != lease.server_epoch.is_some()
    {
        return Err(diagnostic(
            "upgrade_lease_invalid",
            "The protected maintenance lease is invalid",
        ));
    }
    Ok(Some(lease))
}

fn clear(store: &EnvironmentStore) -> SetupResultValue<()> {
    let path = store.root().join(FILE);
    std::fs::remove_file(path).map_err(|_| SetupDiagnostic::io())
}

/// Install the fence before any managed service is stopped. Caller must already
/// hold the environment store lock. `None` scopes the entire local Server.
pub async fn begin_maintenance(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    client_id: Option<&str>,
) -> SetupResultValue<()> {
    if client_id.is_none() && !record.request.local_server() {
        return Err(diagnostic(
            "upgrade_scope",
            "A joined environment may fence only its own Runner",
        ));
    }
    if client_id.is_some_and(|id| record.runner_client_id.as_deref() != Some(id)) {
        return Err(diagnostic(
            "upgrade_scope",
            "The requested Runner is not the configured local Runner",
        ));
    }
    let mut lease = match saved(store)? {
        Some(existing)
            if existing.server_url == record.request.server_url
                && existing.client_id.as_deref() == client_id =>
        {
            existing
        }
        Some(_) => {
            return Err(diagnostic(
                "upgrade_lease_exists",
                "A prior protected maintenance lease requires recovery",
            ))
        }
        None => {
            let nonce = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            let pending = SavedLease {
                server_url: record.request.server_url.clone(),
                server_epoch: None,
                client_id: client_id.map(str::to_string),
                operation_nonce: nonce,
                token: None,
            };
            let bytes = serde_json::to_vec(&pending).map_err(|_| SetupDiagnostic::io())?;
            atomic_private_write(&store.root().join(FILE), &bytes)?;
            pending
        }
    };
    // Always round-trip: a saved token from an earlier process epoch does not
    // prove the current Server still holds the fence.
    let (status, body) = call(
        record,
        store,
        json!({"action":"begin","client_id":client_id,"operation_nonce":lease.operation_nonce}),
    )
    .await?;
    if status == 404 {
        // A first begin against an older Server cannot have installed a
        // fence. Do not retain an unusable pending nonce in that case.
        if lease.token.is_none() {
            clear(store)?;
        }
        return Err(diagnostic("upgrade_fence_unsupported", "The Server does not support safe upgrade admission; stop services explicitly or upgrade the Server first"));
    }
    if status == 409 {
        return Err(diagnostic(
            "upgrade_active_tasks",
            "Active tasks or another maintenance owner prevent the upgrade",
        ));
    }
    if status != 200 {
        return Err(diagnostic(
            "upgrade_fence_denied",
            "The Server denied the maintenance fence",
        ));
    }
    contract(&body)?;
    let token = body
        .get("lease_token")
        .and_then(Value::as_str)
        .filter(|value| valid_token(value))
        .ok_or_else(|| {
            diagnostic(
                "upgrade_fence_invalid",
                "The Server returned an invalid maintenance token",
            )
        })?;
    let server_epoch = body
        .get("server_epoch")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| {
            diagnostic(
                "upgrade_fence_invalid",
                "The Server returned an invalid maintenance epoch",
            )
        })?;
    lease.server_epoch = Some(server_epoch.to_string());
    lease.token = Some(token.to_string());
    let bytes = serde_json::to_vec(&lease).map_err(|_| SetupDiagnostic::io())?;
    if let Err(error) = atomic_private_write(&store.root().join(FILE), &bytes) {
        let _ = call(
            record,
            store,
            json!({"action":"end_pending","operation_nonce":lease.operation_nonce}),
        )
        .await;
        return Err(error);
    }
    Ok(())
}

/// Refresh before the 90-second deadline while the Server remains reachable.
/// An expired lease still blocks admission; only its original owner can end it.
pub async fn renew_maintenance(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
) -> SetupResultValue<()> {
    let lease = saved(store)?.ok_or_else(|| {
        diagnostic(
            "upgrade_lease_missing",
            "No protected maintenance lease exists",
        )
    })?;
    if lease.server_url != record.request.server_url {
        return Err(diagnostic(
            "upgrade_lease_mismatch",
            "The maintenance lease belongs to another Server",
        ));
    }
    let token = lease.token.ok_or_else(|| {
        diagnostic(
            "upgrade_lease_pending",
            "The maintenance begin result is uncertain; retry begin or roll back",
        )
    })?;
    let (status, body) = call(record, store, json!({"action":"renew","lease_token":token})).await?;
    if status != 200 {
        return Err(diagnostic(
            "upgrade_lease_expired",
            "The maintenance lease could not be renewed; admission remains paused",
        ));
    }
    contract(&body)
}

/// Release after successful restart or rollback. The nonce provides an
/// idempotent confirmation if the token-based end response was lost.
pub async fn end_maintenance(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
) -> SetupResultValue<()> {
    let Some(lease) = saved(store)? else {
        return Ok(());
    };
    if lease.server_url != record.request.server_url {
        return Err(diagnostic(
            "upgrade_lease_mismatch",
            "The maintenance lease belongs to another Server",
        ));
    }
    let Some(token) = lease.token.as_deref() else {
        let (status, body) = call(
            record,
            store,
            json!({"action":"end_pending","operation_nonce":lease.operation_nonce}),
        )
        .await?;
        if status == 200 {
            contract(&body)?;
            return clear(store);
        }
        return Err(diagnostic(
            "upgrade_lease_release",
            "The Server did not confirm cleanup of the pending maintenance begin",
        ));
    };
    let (status, body) = call(record, store, json!({"action":"end","lease_token":token})).await?;
    if status == 200 {
        contract(&body)?;
        return clear(store);
    }
    if status == 409 {
        let (pending_status, pending_body) = call(
            record,
            store,
            json!({"action":"end_pending","operation_nonce":lease.operation_nonce}),
        )
        .await?;
        if pending_status == 200 {
            contract(&pending_body)?;
            return clear(store);
        }
    }
    Err(diagnostic(
        "upgrade_lease_release",
        "The Server did not confirm release of the maintenance lease",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_lease_file_requires_nonce_and_consistent_complete_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("env")).unwrap();
        let pending = SavedLease {
            server_url: "https://example.invalid".into(),
            server_epoch: None,
            client_id: Some("runner".into()),
            operation_nonce: "a".repeat(64),
            token: None,
        };
        atomic_private_write(
            &store.root().join(FILE),
            &serde_json::to_vec(&pending).unwrap(),
        )
        .unwrap();
        assert!(saved(&store).unwrap().unwrap().token.is_none());

        let invalid = SavedLease {
            token: Some("b".repeat(64)),
            ..pending
        };
        atomic_private_write(
            &store.root().join(FILE),
            &serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        assert_eq!(saved(&store).err().unwrap().code, "upgrade_lease_invalid");

        let complete = SavedLease {
            server_epoch: Some("epoch".into()),
            ..invalid
        };
        atomic_private_write(
            &store.root().join(FILE),
            &serde_json::to_vec(&complete).unwrap(),
        )
        .unwrap();
        assert_eq!(
            saved(&store).unwrap().unwrap().token.as_deref(),
            Some("b".repeat(64).as_str())
        );
    }
}
