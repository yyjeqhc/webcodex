/** Metadata only: native observations never include configuration or log contents. */
export interface PathEntry {
  id: string;
  component: string;
  purpose: string;
  source: string;
  configured_path: string | null;
  canonical_path: string | null;
  directory_to_open: string | null;
  status: "present" | "missing" | "unreadable" | "unsafe_path" | "invalid" | "unconfirmed" | "not_applicable" | "not_configured" | "remote";
  kind: "file" | "directory" | "system_log" | "in_memory" | "remote_reference";
  log_source: { kind: "systemd_journal" | "windows_events" | "task_scheduler" | "lifecycle_file" | "in_memory"; unit_name: string | null; service_scope: "user" | "system" | null } | null;
  category: "metadata" | "mixed_configuration" | "secret" | "data" | "log" | "cache" | "binary";
}
export interface PathInventory {
  schema_version: 1;
  observed_at_ms: number;
  environment_id: string | null;
  local_server: boolean | null;
  local_runner: boolean | null;
  service_scope: "user" | "system" | null;
  roots: PathEntry[];
  revision: string;
  entries: PathEntry[];
  identities: { kind: string; value: string; source: string }[];
  issues: { code: string; entry_id: string | null }[];
  builds: { source: "entry_point" | "previously_verified"; build: {
    binary: string; version: string | null; git_commit: string | null; git_dirty: boolean | null;
    target: string | null; architecture: string | null;
    desktop_runtime_contract: { min_generation: number; max_generation: number };
    environment_data_format: number | null;
  } }[];
}
export type InventoryDocumentKind = "inventory" | "backup_manifest" | "settings_export";
