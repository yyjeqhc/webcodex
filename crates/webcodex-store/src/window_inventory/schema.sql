CREATE TABLE IF NOT EXISTS window_inventory_meta(version INTEGER PRIMARY KEY CHECK(version=1));
CREATE TABLE IF NOT EXISTS window_inventory_dirty(window_key TEXT PRIMARY KEY) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS window_inventory_cells(
    window_key TEXT NOT NULL, scope_key TEXT NOT NULL,
    principal_kind TEXT, principal_id TEXT, project TEXT, anchors TEXT NOT NULL,
    source TEXT NOT NULL, first_seen INTEGER NOT NULL, last_seen INTEGER NOT NULL,
    last_tool INTEGER, last_meaningful INTEGER, gap_count INTEGER NOT NULL,
    activity_name TEXT, activity_status TEXT, activity_meaningful INTEGER,
    observed INTEGER NOT NULL, event_id TEXT NOT NULL,
    PRIMARY KEY(window_key,scope_key)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_window_inventory_project ON window_inventory_cells(project,window_key);
CREATE INDEX IF NOT EXISTS idx_window_inventory_principal ON window_inventory_cells(principal_kind,principal_id,window_key);
CREATE TABLE IF NOT EXISTS window_inventory_links(
    window_key TEXT NOT NULL, principal_key TEXT NOT NULL, principal_kind TEXT, principal_id TEXT,
    workflow_session_id TEXT NOT NULL, project TEXT, last_linked INTEGER NOT NULL,
    PRIMARY KEY(window_key,principal_key,workflow_session_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_window_inventory_links_principal ON window_inventory_links(principal_kind,principal_id,window_key);

CREATE TRIGGER IF NOT EXISTS window_inventory_event_insert AFTER INSERT ON action_events
WHEN NEW.client_window_key IS NOT NULL BEGIN
    INSERT OR IGNORE INTO window_inventory_dirty VALUES(NEW.client_window_key);
END;
CREATE TRIGGER IF NOT EXISTS window_inventory_event_delete AFTER DELETE ON action_events
WHEN OLD.client_window_key IS NOT NULL BEGIN
    INSERT OR IGNORE INTO window_inventory_dirty VALUES(OLD.client_window_key);
END;
CREATE TRIGGER IF NOT EXISTS window_inventory_event_update AFTER UPDATE ON action_events BEGIN
    INSERT OR IGNORE INTO window_inventory_dirty SELECT OLD.client_window_key WHERE OLD.client_window_key IS NOT NULL;
    INSERT OR IGNORE INTO window_inventory_dirty SELECT NEW.client_window_key WHERE NEW.client_window_key IS NOT NULL;
END;
CREATE TRIGGER IF NOT EXISTS window_inventory_link_insert AFTER INSERT ON action_event_workflow_links BEGIN
    INSERT OR IGNORE INTO window_inventory_dirty SELECT client_window_key FROM action_events WHERE event_id=NEW.event_id AND client_window_key IS NOT NULL;
END;
CREATE TRIGGER IF NOT EXISTS window_inventory_link_delete AFTER DELETE ON action_event_workflow_links BEGIN
    INSERT OR IGNORE INTO window_inventory_dirty SELECT client_window_key FROM action_events WHERE event_id=OLD.event_id AND client_window_key IS NOT NULL;
END;
CREATE TRIGGER IF NOT EXISTS window_inventory_link_update AFTER UPDATE ON action_event_workflow_links BEGIN
    INSERT OR IGNORE INTO window_inventory_dirty SELECT client_window_key FROM action_events WHERE event_id IN (OLD.event_id,NEW.event_id) AND client_window_key IS NOT NULL;
END;
