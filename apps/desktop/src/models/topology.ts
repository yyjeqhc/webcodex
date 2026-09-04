export type Experience = "full" | "quick_share";

export type ServerTopology =
  | { kind: "local" }
  | { kind: "remote"; url: string };

export type RunnerTopology = { kind: "local" };

export type Exposure =
  | { kind: "none" }
  | { kind: "existing_https"; url: string }
  | { kind: "cloudflare" }
  | { kind: "open_ai_tunnel" };

export type Enrollment =
  | { kind: "managed_pairing" }
  | { kind: "shared_key" }
  | { kind: "existing_profile"; profile: string };

export interface RuntimeTopology {
  experience: Experience;
  server: ServerTopology;
  runner: RunnerTopology;
  exposure: Exposure;
  enrollment: Enrollment;
}

export type ServerReadiness =
  | "stopped"
  | "starting"
  | "ready"
  | "error"
  | "unknown";
export type RunnerReadiness =
  | "stopped"
  | "connecting"
  | "ready"
  | "error"
  | "unknown";
export type ExposureReadiness =
  | "disabled"
  | "starting"
  | "local_ready"
  | "remote_ready"
  | "degraded"
  | "error"
  | "unknown";
export type ProjectReadiness =
  | "none"
  | "configured"
  | "reload_required"
  | "ready"
  | "error"
  | "unknown";

export interface ReadinessSnapshot {
  server: ServerReadiness;
  runner: RunnerReadiness;
  exposure: ExposureReadiness;
  project: ProjectReadiness;
  runtime_ready: boolean;
  ready_for_chatgpt: boolean;
  summary: string;
  next_action?: string | null;
}

export interface ProjectSelection {
  path: string;
  allowed_root: string;
  is_git_repository: boolean;
  runtime_project_id?: string | null;
}

export interface BinaryInfo {
  directory: string;
  version: string;
  git_commit: string;
  source: string;
}

export interface QuickShareState {
  provider: string;
  project: string;
  mcp_url?: string | null;
  clipboard_state: string;
  clipboard_contains: string;
  ready_for_chatgpt: boolean;
}

export interface DesktopState {
  topology?: RuntimeTopology | null;
  readiness: ReadinessSnapshot;
  project?: ProjectSelection | null;
  binaries?: BinaryInfo | null;
  quick_share?: QuickShareState | null;
  activity_sequence: number;
  openai_tunnel_configured: boolean;
  regular_tunnel_available: boolean;
}

export interface DesktopError {
  code: string;
  message: string;
  next_action: string;
  details?: unknown;
}

export interface ActivityEntry {
  sequence: number;
  timestamp_ms: number;
  source: string;
  level: "info" | "warning" | "error";
  message: string;
}

