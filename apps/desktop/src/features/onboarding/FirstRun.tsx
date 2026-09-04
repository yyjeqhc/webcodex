import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi, type QuickShareProvider } from "../../lib/desktop-api";
import type {
  DesktopError,
  DesktopState,
  ProjectSelection,
} from "../../models/topology";

type SetupMode = "local" | "remote" | "share";

interface FirstRunProps {
  state: DesktopState;
  onState: (state: DesktopState) => void;
}

export function FirstRun({ state, onState }: FirstRunProps) {
  const initialMode = useMemo<SetupMode | null>(() => {
    if (state.topology?.experience === "quick_share") return "share";
    if (state.topology?.server.kind === "local") return "local";
    if (state.topology?.server.kind === "remote") return "remote";
    return null;
  }, [state.topology]);
  const [mode, setMode] = useState<SetupMode | null>(initialMode);
  const [project, setProject] = useState<ProjectSelection | null>(
    state.project ?? null,
  );
  const [serverUrl, setServerUrl] = useState(
    state.topology?.server.kind === "remote" ? state.topology.server.url : "",
  );
  const [pairingCode, setPairingCode] = useState("");
  const [provider, setProvider] = useState<QuickShareProvider>("cloudflare");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const canReuseRemoteEnrollment = Boolean(
    mode === "remote" &&
      project?.runtime_project_id &&
      state.topology?.experience === "full" &&
      state.topology.server.kind === "remote" &&
      sameServerOrigin(serverUrl, state.topology.server.url),
  );

  const chooseProject = async () => {
    setError(null);
    const selection = await open({
      directory: true,
      multiple: false,
      title: "Choose a project folder",
    });
    if (typeof selection !== "string") return;
    try {
      setProject(await desktopApi.inspectProject(selection));
    } catch (value) {
      setError(normalizeError(value));
    }
  };

  const run = async () => {
    if (!mode || !project) return;
    setBusy(true);
    setError(null);
    try {
      if (mode === "local") {
        onState(await desktopApi.configureLocal(project.path));
      } else if (mode === "remote") {
        const oneTimeCode = pairingCode;
        setPairingCode("");
        const next = await desktopApi.configureRemote(
          serverUrl,
          oneTimeCode,
          project.path,
        );
        onState(next);
      } else {
        onState(await desktopApi.startQuickShare(project.path, provider));
      }
    } catch (value) {
      setError(normalizeError(value));
    } finally {
      setBusy(false);
    }
  };

  if (!mode) {
    return (
      <section className="first-run">
        <div className="eyebrow">Welcome to WebCodex</div>
        <h1>How do you want to use this computer?</h1>
        <p className="lede">
          Desktop manages the Service, Runner, projects, and ChatGPT connection
          without requiring PowerShell setup.
        </p>
        <div className="entry-grid">
          <button className="entry-card recommended" onClick={() => setMode("local")}>
            <span className="entry-badge">Recommended</span>
            <strong>Use WebCodex on this computer</strong>
            <span>Run the WebCodex Service and Runner here, then add projects.</span>
          </button>
          <button className="entry-card" onClick={() => setMode("remote")}>
            <strong>Connect this computer to an existing Server</strong>
            <span>Keep the Server remote and run only this computer&apos;s Runner here.</span>
          </button>
          <button className="entry-card" onClick={() => setMode("share")}>
            <strong>Quick Share a project</strong>
            <span>Start a temporary one-project session that ends when you stop it.</span>
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="setup-shell">
      <button className="back-button" onClick={() => !busy && setMode(null)}>
        ← All setup options
      </button>
      <div className="eyebrow">{modeLabel(mode)}</div>
      <h1>{setupTitle(mode)}</h1>
      <p className="lede">{setupDescription(mode)}</p>

      {mode === "remote" && (
        <div className="form-card">
          <label>
            Server URL
            <input
              type="url"
              value={serverUrl}
              onChange={(event) => setServerUrl(event.target.value)}
              placeholder="https://webcodex.example.com"
              disabled={busy}
            />
          </label>
          {canReuseRemoteEnrollment ? (
            <div className="enrollment-note">
              <span className="section-kicker">Enrollment</span>
              <strong>Existing connection will be reused</strong>
              <span>No new login code is required for this Server and project.</span>
            </div>
          ) : (
            <label>
              One-time login code
              <input
                type="password"
                value={pairingCode}
                onChange={(event) => setPairingCode(event.target.value)}
                placeholder="wc_pair_…"
                autoComplete="off"
                spellCheck={false}
                disabled={busy}
              />
              <span className="field-help">
                The code is passed to Rust for this login attempt, then cleared from the UI.
              </span>
            </label>
          )}
        </div>
      )}

      {mode === "share" && (
        <div className="provider-row" role="radiogroup" aria-label="Quick Share provider">
          {(["cloudflare", "openai", "none"] as QuickShareProvider[]).map((value) => (
            <button
              className={`provider-option ${provider === value ? "selected" : ""}`}
              key={value}
              onClick={() => setProvider(value)}
              disabled={busy}
            >
              <strong>{providerLabel(value)}</strong>
              <span>{providerDescription(value)}</span>
            </button>
          ))}
        </div>
      )}

      <div className="project-picker-card">
        <div>
          <span className="section-kicker">Project</span>
          <strong>{project ? project.path : "Choose a project folder"}</strong>
          {project && (
            <span className="project-meta">
              Allowed root: {project.allowed_root} · {project.is_git_repository ? "Git repository" : "Folder"}
            </span>
          )}
        </div>
        <button className="secondary-button" onClick={chooseProject} disabled={busy}>
          {project ? "Change folder" : "Choose folder"}
        </button>
      </div>

      {mode === "remote" && (
        <details className="advanced-enrollment">
          <summary>Advanced enrollment</summary>
          <p>
            Shared key and existing hosted profiles remain owned by the canonical
            <code> webcodex connect </code> lifecycle. D1 does not copy bootstrap or
            admin credentials into Desktop. Direct profile handoff is planned for D2.
          </p>
        </details>
      )}

      {error && (
        <div className="error-card" role="alert">
          <strong>{error.message}</strong>
          <span>{error.next_action}</span>
          <code>{error.code}</code>
        </div>
      )}

      <div className="setup-actions">
        <button
          className="primary-button"
          onClick={run}
          disabled={
            busy ||
            !project ||
            (mode === "remote" &&
              (!serverUrl.trim() || (!canReuseRemoteEnrollment && !pairingCode.trim())))
          }
        >
          {busy ? "Checking real readiness…" : actionLabel(mode, canReuseRemoteEnrollment)}
        </button>
        <span className="action-help">
          {busy ? "Desktop is verifying Service, Runner, and project state." : "No terminal commands required."}
        </span>
      </div>
    </section>
  );
}

function normalizeError(value: unknown): DesktopError {
  if (value && typeof value === "object") {
    const candidate = value as Partial<DesktopError>;
    if (candidate.code && candidate.message && candidate.next_action) {
      return candidate as DesktopError;
    }
  }
  return {
    code: "desktop_operation_failed",
    message: "Desktop could not complete the operation.",
    next_action: "Retry the operation or open Activity for safe diagnostics.",
  };
}

function modeLabel(mode: SetupMode) {
  return mode === "local" ? "This computer" : mode === "remote" ? "Existing Server" : "Temporary session";
}

function setupTitle(mode: SetupMode) {
  return mode === "local"
    ? "Set up WebCodex here"
    : mode === "remote"
      ? "Connect this computer"
      : "Quick Share a project";
}

function setupDescription(mode: SetupMode) {
  if (mode === "local") return "Desktop will prepare a local Service, enroll this Runner once, and verify the selected project.";
  if (mode === "remote") return "The Server stays remote. Desktop enrolls and supervises only this computer's Runner.";
  return "Desktop reuses the existing WebCodex share lifecycle and stops the entire temporary session together.";
}

function actionLabel(mode: SetupMode, canReuseRemoteEnrollment: boolean) {
  if (mode === "local") return "Set up WebCodex";
  if (mode === "remote") return canReuseRemoteEnrollment ? "Reconnect computer" : "Connect computer";
  return "Start Quick Share";
}

function sameServerOrigin(left: string, right: string) {
  return left.trim().replace(/\/+$/, "").toLowerCase() === right.trim().replace(/\/+$/, "").toLowerCase();
}

function providerLabel(provider: QuickShareProvider) {
  if (provider === "cloudflare") return "Cloudflare";
  if (provider === "openai") return "OpenAI Secure Tunnel";
  return "Local only";
}

function providerDescription(provider: QuickShareProvider) {
  if (provider === "cloudflare") return "Temporary public HTTPS reachability";
  if (provider === "openai") return "Private OpenAI tunnel when already configured";
  return "No external exposure";
}

