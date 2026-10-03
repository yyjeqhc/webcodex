//! Derived, authority-free Window inventory. Historical ActionAudit remains canonical.
//!
//! Cells partition events by exact principal and the complete Project-anchor set.
//! Console supplies freshly authorized Project IDs for each read. No grant or
//! visibility result is persisted. Normal appends update one cell transactionally;
//! canonical UPDATE/DELETE (including retention) marks affected Windows dirty.
//! Reads repair dirty Windows as a set before exposing any summary.

use crate::Database;
use rusqlite::{named_params, params, Connection};
use serde::{Deserialize, Serialize};

const SCHEMA: &str = include_str!("window_inventory/schema.sql");
const CELLS: &str = include_str!("window_inventory/cells.sql");
const LINKS: &str = include_str!("window_inventory/links.sql");
const INVENTORY: &str = include_str!("window_inventory/read.sql");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInventoryRow {
    pub client_window_key: String,
    pub source: String,
    pub last_project: Option<String>,
    pub first_seen_at_ms: i64,
    pub last_seen_at_ms: i64,
    pub last_tool_call_at_ms: Option<i64>,
    pub last_meaningful_activity_at_ms: Option<i64>,
    pub last_activity_name: Option<String>,
    pub last_activity_status: Option<String>,
    pub last_activity_meaningful: Option<bool>,
    pub active_count: usize,
    pub linked_session_count: usize,
    pub recorder_gap_count: usize,
}

pub struct WindowInventoryQuery<'a> {
    pub principal: Option<(&'a str, &'a str)>,
    pub caller: Option<(&'a str, &'a str)>,
    /// Ordinary management credentials restrict projectless unanchored events
    /// to their caller principal. Admin/scoped-principal reads use native links.
    pub management: bool,
    pub visible_projects: &'a [String],
    pub projects: Option<&'a [String]>,
    pub window_key: Option<&'a str>,
    pub query: &'a str,
    /// Already-authorized live requests; never inferred from historical state.
    pub live: &'a [WindowInventoryRow],
    pub offset: usize,
    pub limit: usize,
}

pub struct WindowInventoryPage {
    pub rows: Vec<WindowInventoryRow>,
    pub total: usize,
}

pub(crate) fn ensure_schema(conn: &mut Connection) -> anyhow::Result<()> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch(SCHEMA)?;
    let initialized: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM window_inventory_meta WHERE version = 1)",
        [],
        |r| r.get(0),
    )?;
    if !initialized {
        tx.execute_batch("INSERT OR IGNORE INTO window_inventory_dirty SELECT DISTINCT client_window_key FROM action_events WHERE client_window_key IS NOT NULL;")?;
        repair_dirty(&tx)?;
        tx.execute("INSERT INTO window_inventory_meta(version) VALUES (1)", [])?;
    }
    tx.commit()?;
    Ok(())
}

/// Caller owns the write/read transaction. No nested lock or independent commit.
pub(crate) fn repair_dirty(conn: &Connection) -> anyhow::Result<()> {
    repair_selected(conn, None)
}

pub(crate) fn repair_dirty_for_event(
    conn: &Connection,
    window: Option<&str>,
) -> anyhow::Result<()> {
    // An unrelated historical repair must not join the critical append transaction.
    if let Some(window) = window {
        repair_selected(conn, Some(window))?;
    }
    Ok(())
}

fn repair_selected(conn: &Connection, window: Option<&str>) -> anyhow::Result<()> {
    let dirty: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM window_inventory_dirty WHERE ?1 IS NULL OR window_key=?1)",
        [window],
        |r| r.get(0),
    )?;
    if !dirty {
        return Ok(());
    }
    conn.execute("DELETE FROM window_inventory_cells WHERE window_key IN (SELECT window_key FROM window_inventory_dirty WHERE ?1 IS NULL OR window_key=?1)", [window])?;
    conn.execute("DELETE FROM window_inventory_links WHERE window_key IN (SELECT window_key FROM window_inventory_dirty WHERE ?1 IS NULL OR window_key=?1)", [window])?;
    let predicate = "e.client_window_key IN (SELECT window_key FROM window_inventory_dirty WHERE ?1 IS NULL OR window_key=?1)";
    conn.execute(&CELLS.replace("__SELECTION__", predicate), [window])?;
    conn.execute(&LINKS.replace("__SELECTION__", predicate), [window])?;
    conn.execute(
        "DELETE FROM window_inventory_dirty WHERE ?1 IS NULL OR window_key=?1",
        [window],
    )?;
    Ok(())
}
/// transaction. The caller repairs any earlier dirty history BEFORE inserting.
pub(crate) fn index_appended_event(conn: &Connection, event_id: &str) -> anyhow::Result<()> {
    conn.execute(
        &CELLS.replace("__SELECTION__", "e.event_id = ?1"),
        [event_id],
    )?;
    conn.execute(
        &LINKS.replace("__SELECTION__", "e.event_id = ?1"),
        [event_id],
    )?;
    conn.execute("DELETE FROM window_inventory_dirty WHERE window_key = (SELECT client_window_key FROM action_events WHERE event_id = ?1)", [event_id])?;
    Ok(())
}

impl Database {
    /// A clean derived projection and its dirty check share one read snapshot.
    /// Never upgrade a reader lock to a writer: release it, repair on the single
    /// writer, then start a new snapshot. Concurrent invalidation is bounded and
    /// fails unavailable rather than returning stale evidence.
    fn with_window_inventory_read<T>(
        &self,
        mut read: impl FnMut(&Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        for _ in 0..3 {
            {
                let mut reader =
                    self.lock_history_connection(crate::StoreDomain::WindowActivity)?;
                let snapshot = reader.transaction()?;
                let dirty: bool = snapshot.query_row(
                    "SELECT EXISTS(SELECT 1 FROM window_inventory_dirty)",
                    [],
                    |r| r.get(0),
                )?;
                if !dirty {
                    let result = read(&snapshot)?;
                    snapshot.commit()?;
                    return Ok(result);
                }
            }
            let mut writer =
                self.lock_history_repair_connection(crate::StoreDomain::WindowActivity)?;
            let transaction = writer.transaction()?;
            repair_dirty(&transaction)?;
            transaction.commit()?;
        }
        anyhow::bail!("window inventory changed during bounded repair")
    }

    /// All distinct authority anchors, not event bodies. Never model-visible.
    /// No candidate limit can hide an older authorized Window behind denied rows.
    pub fn window_inventory_project_anchors(
        &self,
        principal: Option<(&str, &str)>,
    ) -> anyhow::Result<Vec<String>> {
        #[cfg(any(test, feature = "root-test-support"))]
        self.window_inventory_reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.with_window_inventory_read(|tx| {
        let (kind, id) = principal.map_or((None, None), |(k, i)| (Some(k), Some(i)));
        let rows = {
            let mut stmt = tx.prepare_cached("WITH cells AS (SELECT project, anchors FROM window_inventory_cells WHERE ?1 IS NULL OR (principal_kind=?1 AND principal_id=?2))
                SELECT project FROM cells WHERE project IS NOT NULL
                UNION SELECT j.value FROM cells c, json_each(c.anchors) j WHERE j.value IS NOT NULL
                UNION SELECT project FROM window_inventory_links WHERE project IS NOT NULL AND (?1 IS NULL OR (principal_kind=?1 AND principal_id=?2))")?;
            let rows = stmt
                .query_map(params![kind, id], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            rows
        };
        Ok(rows)
        })
    }

    /// One set-oriented inventory statement in the steady state, independent of
    /// historical event count. Sorting/grouping concerns compact cells, not events.
    /// Pagination and total are computed AFTER Project/principal authorization
    /// and live merging, so a hidden row cannot affect totals or page placement.
    pub fn read_window_inventory(
        &self,
        query: WindowInventoryQuery<'_>,
    ) -> anyhow::Result<WindowInventoryPage> {
        #[cfg(any(test, feature = "root-test-support"))]
        self.window_inventory_reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.with_window_inventory_read(|tx| {
        let allowed = serde_json::to_string(query.visible_projects)?;
        let projects = query.projects.map(serde_json::to_string).transpose()?;
        let live = serde_json::to_string(query.live)?;
        let (pk, pi) = query
            .principal
            .map_or((None, None), |(k, i)| (Some(k), Some(i)));
        let (ck, ci) = query
            .caller
            .map_or((None, None), |(k, i)| (Some(k), Some(i)));
        let limit = query.limit.clamp(1, 2000) as i64;
        let offset = i64::try_from(query.offset).unwrap_or(i64::MAX);
        let params = named_params! {":allowed": allowed, ":projects": projects, ":pk": pk, ":pi": pi,
        ":ck": ck, ":ci": ci, ":management": query.management, ":live": live,
        ":key": query.window_key, ":query": query.query, ":limit": limit, ":offset": offset};
        let mut page = WindowInventoryPage {
            rows: Vec::new(),
            total: 0,
        };
        {
            let mut stmt = tx.prepare_cached(INVENTORY)?;
            let mut rows = stmt.query(params)?;
            while let Some(r) = rows.next()? {
                page.total = usize::try_from(r.get::<_, i64>(13)?).unwrap_or(usize::MAX);
                page.rows.push(WindowInventoryRow {
                    client_window_key: r.get(0)?,
                    source: r.get(1)?,
                    last_project: r.get(2)?,
                    first_seen_at_ms: r.get(3)?,
                    last_seen_at_ms: r.get(4)?,
                    last_tool_call_at_ms: r.get(5)?,
                    last_meaningful_activity_at_ms: r.get(6)?,
                    last_activity_name: r.get(7)?,
                    last_activity_status: r.get(8)?,
                    last_activity_meaningful: r.get(9)?,
                    active_count: r.get::<_, i64>(10)?.max(0) as usize,
                    linked_session_count: r.get::<_, i64>(11)?.max(0) as usize,
                    recorder_gap_count: r.get::<_, i64>(12)?.max(0) as usize,
                });
            }
        }
        // Empty pages still report the authorized total, not zero or a hidden
        // source count. This rare fallback repeats the same compact projection.
        if page.rows.is_empty() && query.offset > 0 {
            let count_sql = INVENTORY.replace("SELECT *, COUNT(*) OVER() AS total FROM filtered ORDER BY last_seen_at_ms DESC, client_window_key ASC LIMIT :limit OFFSET :offset", "SELECT COUNT(*) FROM filtered WHERE :limit >= 0 AND :offset >= 0");
            page.total = tx
                .query_row(&count_sql, params, |r| r.get::<_, i64>(0))?
                .max(0) as usize;
        }
        Ok(page)
        })
    }
}
