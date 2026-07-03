use super::Database;
use crate::{AgentModelProfileRecord, AgentSpecRecord};
use rusqlite::params;

impl Database {
    pub fn upsert_agent_spec(&self, record: &AgentSpecRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO agent_specs (id, name, base_url, auth_token, openapi_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                base_url = excluded.base_url,
                auth_token = excluded.auth_token,
                openapi_json = excluded.openapi_json,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.name,
                record.base_url,
                record.auth_token,
                record.openapi_json,
                record.created_at,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_agent_specs(&self) -> anyhow::Result<Vec<AgentSpecRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, auth_token, openapi_json, created_at, updated_at
             FROM agent_specs ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_agent_spec)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_agent_spec(&self, id: &str) -> anyhow::Result<Option<AgentSpecRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, auth_token, openapi_json, created_at, updated_at
             FROM agent_specs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_agent_spec)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn delete_agent_spec(&self, id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM agent_specs WHERE id = ?1", params![id])?;
        Ok(changed == 1)
    }

    pub fn upsert_agent_model_profile(
        &self,
        record: &AgentModelProfileRecord,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO agent_model_profiles (id, base_url, api_key, model, temperature, max_rounds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                base_url = excluded.base_url,
                api_key = excluded.api_key,
                model = excluded.model,
                temperature = excluded.temperature,
                max_rounds = excluded.max_rounds,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.base_url,
                record.api_key,
                record.model,
                record.temperature,
                record.max_rounds.map(|v| v as i64),
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_agent_model_profile(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<AgentModelProfileRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, base_url, api_key, model, temperature, max_rounds, updated_at
             FROM agent_model_profiles WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_agent_model_profile)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }
}

fn row_to_agent_spec(row: &rusqlite::Row) -> rusqlite::Result<AgentSpecRecord> {
    Ok(AgentSpecRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        auth_token: row.get(3)?,
        openapi_json: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn row_to_agent_model_profile(row: &rusqlite::Row) -> rusqlite::Result<AgentModelProfileRecord> {
    Ok(AgentModelProfileRecord {
        id: row.get(0)?,
        base_url: row.get(1)?,
        api_key: row.get(2)?,
        model: row.get(3)?,
        temperature: row.get(4)?,
        max_rounds: row.get::<_, Option<i64>>(5)?.map(|v| v as usize),
        updated_at: row.get(6)?,
    })
}
