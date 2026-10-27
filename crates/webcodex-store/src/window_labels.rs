//! Operator-only display annotations. Never used as identity or routing authority.
use crate::{Database, StoreDomain};
use rusqlite::{params, OptionalExtension};

pub fn normalize_window_label(label: Option<&str>) -> anyhow::Result<Option<String>> {
    let value = label.unwrap_or("").trim();
    anyhow::ensure!(
        value.chars().count() <= 80,
        "Window label exceeds 80 characters"
    );
    anyhow::ensure!(
        !value.chars().any(char::is_control),
        "Window label contains control characters"
    );
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

impl Database {
    /// The Console must authorize this exact principal/Window before writing.
    /// A repeated value preserves its original update timestamp.
    pub fn set_window_label(
        &self,
        principal: (&str, &str),
        window_key: &str,
        label: Option<&str>,
        updated_at_ms: i64,
    ) -> anyhow::Result<Option<String>> {
        let label = normalize_window_label(label)?;
        let conn = self.lock_connection(StoreDomain::WindowActivity);
        if let Some(label) = &label {
            conn.execute("INSERT INTO window_labels(principal_kind,principal_id,window_key,label,updated_at_ms)
                VALUES (?1,?2,?3,?4,?5) ON CONFLICT(principal_kind,principal_id,window_key)
                DO UPDATE SET label=excluded.label,updated_at_ms=excluded.updated_at_ms
                WHERE window_labels.label<>excluded.label", params![principal.0, principal.1, window_key, label, updated_at_ms])?;
        } else {
            conn.execute("DELETE FROM window_labels WHERE principal_kind=?1 AND principal_id=?2 AND window_key=?3", params![principal.0, principal.1, window_key])?;
        }
        Ok(label)
    }

    pub fn clear_window_label(
        &self,
        principal: (&str, &str),
        window_key: &str,
    ) -> anyhow::Result<()> {
        self.set_window_label(principal, window_key, None, 0)
            .map(|_| ())
    }

    pub fn get_window_label(
        &self,
        principal: (&str, &str),
        window_key: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(self.lock_connection(StoreDomain::WindowActivity).query_row(
            "SELECT label FROM window_labels WHERE principal_kind=?1 AND principal_id=?2 AND window_key=?3",
            params![principal.0, principal.1, window_key], |row| row.get(0),
        ).optional()?)
    }
}
