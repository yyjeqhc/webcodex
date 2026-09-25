import { displayProjectPath, projectPresentationName } from "../ui/projectPresentation.js";
import { Alert, Badge, Button, Checkbox, Menu, Modal, PasswordInput, Switch, Table, TextInput } from "@mantine/core";
import { Activity, AlertTriangle, Ellipsis, Folder, LayoutDashboard, LockKeyhole, MoonStar, Plus, RefreshCw, Sun, Users } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { AdminHttpError, AdminRefreshController } from "../admin_controller.js";
import { AdminMutationController, AdminMutationError, type MutationKind, type MutationErrorCode } from "../admin_mutation_controller.js";
import { AdminMutationDialogCoordinator } from "../admin_mutation_view.js";
import { AccentPicker } from "../runtime-v2/components/ui/AccentPicker.js";
import { ACCENT_CHANGE_EVENT, ACCENT_STORAGE_KEY, applyAccentPreference, loadAccentPreference, normalizeAccent, persistAccentPreference } from "../ui/accent.js";
import { BrandMark } from "../ui/BrandMark.js";
import { capabilityLabels, display, emptyDashboard, mergeDashboard, record, semanticStatus, type DashboardSection, type DashboardView, type JsonRecord } from "./model.js";

const ADMIN_BASE = "/api/admin/";
const REFRESH_MS = 10000;
const APPEARANCE_KEY = "webcodex.admin.appearance.v1";
type Appearance = "system" | "light" | "dark";
type DialogState = { kind: MutationKind; target: string; project?: JsonRecord };
type FormFields = {
  client_id: string; project_id: string; name: string; description: string; path: string;
  allow_patch: boolean; git_init: boolean; template: string; adopt_existing_empty: boolean; confirm_project: string;
};
const emptyFields: FormFields = {
  client_id: "", project_id: "", name: "", description: "", path: "", allow_patch: true,
  git_init: false, template: "", adopt_existing_empty: false, confirm_project: "",
};
const errorMessage: Record<MutationErrorCode, string> = {
  invalid_request: "The request is invalid. Review the fields and try again.",
  revision_conflict: "Project state changed. The dashboard was refreshed; confirm again using the latest revision.",
  active_jobs_conflict: "The project has active jobs. No jobs were stopped; refresh and retry after they finish.",
  idempotency_conflict: "This retry no longer matches its original operation. Start a new operation.",
  unsupported_runner_version: "The Agent does not support project lifecycle operations.",
  agent_unavailable: "The Agent is unavailable. Current dashboard data is preserved; retry this same operation later.",
  operation_indeterminate: "The Agent may have completed the operation. Refresh state first, then retry this same operation context rather than creating a new mutation.",
  operation_failed: "The operation failed safely. Internal details were not displayed.",
  network_error: "Network failure. Current data is preserved; retry this same operation.",
  unauthorized: "Administrator authentication required.",
};

function savedAppearance(): Appearance {
  try {
    const value = localStorage.getItem(APPEARANCE_KEY);
    if (value === "light" || value === "dark") return value;
  } catch { /* Memory-only preferences remain usable. */ }
  return "system";
}

async function parseResponse(response: Response): Promise<unknown> {
  try { return await response.json(); } catch { return null; }
}

async function requestDashboard(token: string, signal: AbortSignal): Promise<unknown> {
  const response = await fetch(ADMIN_BASE + "dashboard", {
    method: "POST", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: "{}", signal,
  });
  const data = await parseResponse(response);
  if (!response.ok) throw new AdminHttpError(response.status, "dashboard_failed");
  return data;
}

async function requestMutation(kind: MutationKind, token: string, body: Record<string, unknown>, signal: AbortSignal): Promise<unknown> {
  const response = await fetch(`${ADMIN_BASE}projects/${kind}`, {
    method: "POST", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: JSON.stringify(body), signal,
  });
  const data = record(await parseResponse(response));
  if (response.ok) return data;
  if (response.status === 401 || response.status === 403) throw new AdminMutationError(response.status, "unauthorized");
  const code = record(data.error).code;
  const allowed = new Set<MutationErrorCode>([
    "invalid_request", "revision_conflict", "active_jobs_conflict", "idempotency_conflict",
    "unsupported_runner_version", "agent_unavailable", "operation_indeterminate", "operation_failed",
  ]);
  throw new AdminMutationError(response.status,
    typeof code === "string" && allowed.has(code as MutationErrorCode) ? code as MutationErrorCode : "operation_failed",
    typeof data.active_jobs === "number" ? data.active_jobs : undefined);
}

function Status({ value }: { value: unknown }) {
  const tone = semanticStatus(value);
  return <Badge className="admin-status-badge" size="sm" radius="xl" variant="light"
    color={tone === "good" ? "green" : tone === "warning" ? "yellow" : tone === "error" ? "red" : "blue"}>{display(value)}</Badge>;
}

function Section({ id, eyebrow, title, error, actions, children }: {
  id: string; eyebrow: string; title: string; error?: string; actions?: ReactNode; children: ReactNode;
}) {
  return <section id={id} className="admin-section" aria-labelledby={`${id}-title`}>
    <div className="section-heading"><div><span className="eyebrow">{eyebrow}</span><h2 id={`${id}-title`}>{title}</h2></div>{actions}</div>
    {error && <Alert color="red" role="alert" className="section-error">{error}</Alert>}
    {children}
  </section>;
}

function DataTable({ label, headings, rows, empty }: {
  label: string; headings: string[]; rows: ReactNode[][]; empty: string;
}) {
  return rows.length ? <Table.ScrollContainer className="table-wrap" type="native" minWidth={960}
    role="region" aria-label={`${label} table. Scroll horizontally to view all fields.`} tabIndex={0}>
    <Table stickyHeader className="admin-data-table">
      <Table.Thead><Table.Tr>{headings.map((heading) => <Table.Th key={heading}>{heading}</Table.Th>)}</Table.Tr></Table.Thead>
      <Table.Tbody>{rows.map((cells, index) => <Table.Tr key={index}>{cells.map((cell, column) => <Table.Td key={column}>{cell}</Table.Td>)}</Table.Tr>)}</Table.Tbody>
    </Table>
  </Table.ScrollContainer> : <div className="empty-state">{empty}</div>;
}

function ProjectActionMenu({ project, onAction }: {
  project: JsonRecord;
  onAction: (kind: "enable" | "disable" | "unregister", project: JsonRecord, trigger: HTMLElement) => void;
}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const name = display(project.name || project.id);
  return <Menu position="bottom-end" shadow="md" width={176}>
    <Menu.Target><Button ref={trigger} variant="subtle" color="gray" size="xs" rightSection={<Ellipsis size={16} />}
      aria-label={`Actions for ${name}`}>Actions</Button></Menu.Target>
    <Menu.Dropdown>
      {(["enable", "disable", "unregister"] as const).map((kind) => <Menu.Item key={kind}
        color={kind === "unregister" ? "red" : undefined}
        disabled={record(project.actions)[kind] !== true}
        onClick={() => { if (trigger.current) onAction(kind, project, trigger.current); }}>
        {kind[0].toUpperCase() + kind.slice(1)}
      </Menu.Item>)}
    </Menu.Dropdown>
  </Menu>;
}

export function AdminApp() {
  const reduceMotion = useReducedMotion();
  const [appearance, setAppearance] = useState<Appearance>(savedAppearance);
  const [accent, setAccent] = useState(loadAccentPreference);
  const [tokenInput, setTokenInput] = useState("");
  const [authenticated, setAuthenticated] = useState(false);
  const [gateError, setGateError] = useState("");
  const [dashboard, setDashboard] = useState<DashboardView>(emptyDashboard);
  const [status, setStatus] = useState("Locked");
  const [error, setError] = useState("");
  const [auto, setAuto] = useState(true);
  const [activeSection, setActiveSection] = useState("overview-section");
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [fields, setFields] = useState<FormFields>(emptyFields);
  const [dialogError, setDialogError] = useState("");
  const [dialogPending, setDialogPending] = useState(false);
  const dialogOpen = useRef(false);
  const dialogTrigger = useRef<HTMLElement | null>(null);
  const refreshRef = useRef<AdminRefreshController<unknown> | null>(null);
  const mutationRef = useRef<AdminMutationController | null>(null);
  const flowRef = useRef<AdminMutationDialogCoordinator | null>(null);

  const closeDialog = () => { dialogOpen.current = false; setDialog(null); };
  const lock = (message = "Locked.") => {
    flowRef.current?.closeForSessionEnd();
    mutationRef.current?.lock();
    refreshRef.current?.lock(message);
    setDashboard(emptyDashboard);
    setTokenInput("");
    // Old-generation mutation completions intentionally cannot update React
    // state. Clear their pending/error projection at the session boundary.
    setDialogPending(false);
    setDialogError("");
    setError("");
  };

  if (!refreshRef.current) {
    refreshRef.current = new AdminRefreshController({
      request: requestDashboard,
      render: (raw) => setDashboard((previous) => mergeDashboard(previous, raw)),
      showAuthenticated: () => { setAuthenticated(true); setGateError(""); },
      showLocked: (message) => { setAuthenticated(false); setGateError(message); setStatus("Locked"); },
      setStatus,
      showError: setError,
      clearError: () => setError(""),
      onUnauthorized: () => lock("Administrator authentication required."),
    });
  }
  if (!mutationRef.current) {
    mutationRef.current = new AdminMutationController({
      request: requestMutation,
      keyFactory: () => crypto.randomUUID(),
      refresh: () => refreshRef.current!.invalidateAndRefresh(),
      outcome: (message) => { setStatus(message); flowRef.current?.cancel(); },
      error: (code) => setDialogError(errorMessage[code]),
      pending: (_target, value) => setDialogPending(value),
      lock: (message) => lock(message),
    });
  }
  if (!flowRef.current) {
    flowRef.current = new AdminMutationDialogCoordinator(mutationRef.current, {
      close: closeDialog,
      isOpen: () => dialogOpen.current,
      clearSensitive: () => setFields(emptyFields),
      restoreFocus: () => {
        const target = dialogTrigger.current;
        dialogTrigger.current = null;
        window.setTimeout(() => { if (target?.isConnected) target.focus(); }, 0);
      },
    });
  }

  useEffect(() => {
    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    const apply = () => {
      const resolved = appearance === "system" ? (media?.matches ? "dark" : "light") : appearance;
      document.documentElement.dataset.theme = appearance;
      document.documentElement.dataset.resolvedTheme = resolved;
      applyAccentPreference(accent, resolved);
      document.querySelector('meta[name="theme-color"]')?.setAttribute("content", resolved === "dark" ? "#0b0b0c" : "#f5f5f5");
    };
    apply();
    try { localStorage.setItem(APPEARANCE_KEY, appearance); } catch { /* Memory-only. */ }
    media?.addEventListener?.("change", apply);
    return () => media?.removeEventListener?.("change", apply);
  }, [appearance, accent]);

  useEffect(() => {
    const sync = (event: Event) => {
      const next = event instanceof StorageEvent
        ? event.key === ACCENT_STORAGE_KEY ? normalizeAccent(event.newValue) : null
        : normalizeAccent((event as CustomEvent<string>).detail);
      if (next) setAccent(next);
    };
    window.addEventListener(ACCENT_CHANGE_EVENT, sync);
    window.addEventListener("storage", sync);
    return () => { window.removeEventListener(ACCENT_CHANGE_EVENT, sync); window.removeEventListener("storage", sync); };
  }, []);

  useEffect(() => {
    // pagehide can enter the back-forward cache without unmounting React.
    // Lock both the credential holders and rendered privileged state so a
    // restored page cannot display an authenticated but disposed workbench.
    const onPageHide = () => lock();
    window.addEventListener("pagehide", onPageHide);
    return () => {
      window.removeEventListener("pagehide", onPageHide);
      flowRef.current?.closeForSessionEnd();
      mutationRef.current?.dispose();
      refreshRef.current?.dispose();
    };
  }, []);

  useEffect(() => {
    if (!dialog || dialog.kind === "register" || dialog.kind === "create" || !dialogError.startsWith("Project state changed")) return;
    const latest = dashboard.projects.find((project) => String(project.id || "") === dialog.target);
    if (latest && latest.revision !== dialog.project?.revision) {
      setDialog((current) => current?.target === dialog.target ? { ...current, project: latest } : current);
    }
  }, [dashboard.projects, dialog, dialogError]);

  const unlock = async (event: FormEvent) => {
    event.preventDefault();
    const token = tokenInput.trim();
    setTokenInput("");
    if (!token) return;
    flowRef.current!.closeForSessionEnd();
    mutationRef.current!.beginSession(token);
    await refreshRef.current!.beginSession(token);
    if (auto) refreshRef.current!.startAutoRefresh(REFRESH_MS);
  };
  const changeAuto = (checked: boolean) => {
    setAuto(checked);
    if (checked) refreshRef.current!.startAutoRefresh(REFRESH_MS);
    else refreshRef.current!.stopAutoRefresh();
  };
  const changeAccent = (value: string) => { setAccent(value); persistAccentPreference(value); };
  const cycleAppearance = () => setAppearance((current) => current === "system" ? "light" : current === "light" ? "dark" : "system");
  const openCreate = (kind: "register" | "create", trigger: HTMLElement) => {
    const target = `${kind}:${crypto.randomUUID()}`;
    flowRef.current!.open(target);
    dialogTrigger.current = trigger;
    setFields(emptyFields); setDialogError(""); dialogOpen.current = true;
    setDialog({ kind, target });
  };
  const openAction = (kind: "enable" | "disable" | "unregister", project: JsonRecord, trigger: HTMLElement) => {
    const target = String(project.id || "");
    const body = { project: target, expected_revision: String(project.revision || ""), confirm: true };
    const context = mutationRef.current!.start(kind, target, body);
    flowRef.current!.open(target, context, body);
    dialogTrigger.current = trigger;
    setFields(emptyFields); setDialogError(""); dialogOpen.current = true;
    setDialog({ kind, target, project });
  };
  const submitDialog = (event: FormEvent) => {
    event.preventDefault();
    if (!dialog || dialogPending) return;
    const { kind, target } = dialog;
    let body: Record<string, unknown>;
    if (kind === "register" || kind === "create") {
      body = {
        client_id: fields.client_id.trim(), project_id: fields.project_id.trim(), name: fields.name.trim(),
        description: fields.description.trim() || null, path: fields.path.trim(), allow_patch: fields.allow_patch,
      };
      if (kind === "create") Object.assign(body, {
        git_init: fields.git_init, template: fields.template.trim() || null, adopt_existing_empty: fields.adopt_existing_empty,
      });
    } else {
      body = { project: target, expected_revision: String(dialog.project?.revision || ""), confirm: true };
    }
    if (kind === "unregister" && fields.confirm_project !== target) {
      setDialogError("Type the full runtime project ID to confirm.");
      return;
    }
    setDialogError("");
    void flowRef.current!.submit(kind, body);
  };
  const setField = <K extends keyof FormFields>(key: K, value: FormFields[K]) => setFields((current) => ({ ...current, [key]: value }));
  const sectionError = (section: DashboardSection) => dashboard.errors[section];
  const overview = dashboard.overview;
  const overviewItems = [
    ["Server", display(overview.version), `${display(overview.build_commit)} · ${display(overview.authority_mode)}`],
    ["Agents", `${display(overview.agents_online || 0)} / ${display(overview.agents_total || 0)}`, "online now"],
    ["Projects", `${display(overview.projects_online || 0)} / ${display(overview.projects_total || 0)}`, "ready for work"],
    ["Jobs", display(overview.active_jobs || 0), display(overview.version_compatibility || "compatibility unknown")],
  ];
  const navigation = [
    ["overview-section", "Overview", LayoutDashboard], ["devices-section", "Agents", Users],
    ["projects-section", "Projects", Folder], ["diagnostics-section", "Diagnostics", AlertTriangle],
  ] as const;

  return <div className="admin-shell ui-canvas">
    <aside className="admin-rail ui-glass" aria-label="WebCodex Admin navigation">
      <div className="product-identity"><BrandMark /><div><strong>WebCodex</strong><span>Admin console</span></div></div>
      <nav className="section-nav" aria-label="Dashboard sections">
        {navigation.map(([id, label, Icon]) => <a key={id} href={`#${id}`} aria-current={activeSection === id ? "location" : undefined}
          aria-label={label} title={label} onClick={() => setActiveSection(id)}>
          {activeSection === id && <motion.span className="admin-nav-rail" layoutId="admin-nav-rail" initial={false}
            transition={reduceMotion ? { duration: 0 } : { type: "spring", stiffness: 420, damping: 38 }} aria-hidden="true" />}
          <Icon size={18} aria-hidden="true" /><span>{label}</span></a>)}
      </nav>
      <div className="rail-footer">
        <p className="rail-status"><span aria-hidden="true" /> Control plane</p>
        <Button variant="subtle" color="gray" onClick={cycleAppearance} title={`Appearance: ${appearance}`} aria-label={`Appearance: ${appearance}`} className="appearance-button"
          leftSection={appearance === "dark" ? <MoonStar size={16} /> : <Sun size={16} />}>Appearance · {appearance}</Button>
        <AccentPicker color={accent} onChange={changeAccent} language="en" />
      </div>
    </aside>
    <main className="admin-workspace">
      {!authenticated ? <section className="gate ui-workbench-surface" aria-labelledby="gate-title">
        <div className="gate-icon"><LockKeyhole size={22} /></div>
        <span className="eyebrow">Secure control plane</span><h1 id="gate-title">Administrator access</h1>
        <p>Use a bootstrap token or admin-scoped PAT. The token stays only in this page’s memory.</p>
        <form onSubmit={(event) => void unlock(event)} autoComplete="off" className="gate-form">
          <PasswordInput label="Admin token" aria-label="Admin token" autoComplete="off" value={tokenInput} onChange={(event) => setTokenInput(event.currentTarget.value)} placeholder="Enter admin token" />
          <Button type="submit" className="admin-primary">Unlock console</Button>
        </form>
        {gateError && <Alert color="red" role="alert" mt="md">{gateError}</Alert>}
      </section> : <>
        <header className="workspace-toolbar ui-glass"><div className="page-context"><span className="eyebrow">Control plane</span><h1>Operations</h1></div>
          <div className="toolbar-actions"><Badge color="green" variant="light" className="status" role="status">{status}</Badge>
            <Switch label="Live" checked={auto} onChange={(event) => changeAuto(event.currentTarget.checked)} size="sm" />
            <Button variant="default" leftSection={<RefreshCw size={15} />} onClick={() => void refreshRef.current!.refresh()}>Refresh</Button>
            <Button variant="subtle" color="gray" onClick={() => lock()}>Lock</Button></div>
        </header>
        {error && <Alert color="red" role="alert" className="workspace-error">{error}</Alert>}
        <Section id="overview-section" eyebrow="At a glance" title="System flow" error={sectionError("overview")}>
          <div className="overview-strip">{overviewItems.map(([label, value, subtitle], index) => <div className="overview-item" key={label}>
            <span>{label}</span><strong className={index ? "metric" : ""}>{value}</strong><small>{subtitle}</small>
          </div>)}</div>
        </Section>
        <Section id="devices-section" eyebrow="Connected fleet" title="Agents" error={sectionError("devices")}>
          <DataTable label="Devices and Agents" empty="No devices observed." headings={["Name", "Client", "Status", "Transport", "Host", "Last seen", "Capabilities", "Projects", "Jobs", "Protocol", "Build alignment"]}
            rows={dashboard.devices.map((device) => [display(device.display_name), <code>{display(device.client_id)}</code>, <Status value={device.status} />,
              display(device.transport), display(device.hostname), <code>{display(device.last_seen)}</code>, display(capabilityLabels(device.capabilities).join(", ")),
              display(device.project_count), display(device.active_jobs), <Status value={device.protocol_compatibility || device.compatibility} />, <span title="Build identity is diagnostic, not functional compatibility">{display(device.build_alignment)}</span>])} />
        </Section>
        <Section id="projects-section" eyebrow="Runtime registry" title="Projects" error={sectionError("projects")}
          actions={<div className="section-actions"><Button variant="default" onClick={(event) => openCreate("register", event.currentTarget)}>Register</Button>
            <Button className="admin-primary" leftSection={<Plus size={15} />} onClick={(event) => openCreate("create", event.currentTarget)}>Create project</Button></div>}>
          <DataTable label="Projects" empty="No projects registered." headings={["Runtime project", "Name", "Client", "Path", "Lifecycle", "Jobs", "Git", "Patch", "Shell profile", "Protocol", "Build alignment", "Console", "Actions"]}
            rows={dashboard.projects.map((project) => [<code>{display(project.id)}</code>, projectPresentationName({ name: String(project.name || ""), path: String(project.path || ""), id: String(project.id || "") }), display(project.client_id), <code title={displayProjectPath(String(project.path || ""))}>{display(displayProjectPath(String(project.path || "")))}</code>,
              <Status value={project.lifecycle_status || project.readiness} />, display(project.active_jobs), <Status value={project.git_available} />,
              <Status value={project.allow_patch} />, <Status value={project.shell_profile_status} />, <Status value={project.protocol_compatibility || project.compatibility} />, <span title="Build identity is diagnostic, not functional compatibility">{display(project.build_alignment)}</span>,
              display(project.console_hint), <ProjectActionMenu project={project} onAction={openAction} />])} />
        </Section>
        <div className="details-grid">
          <Section id="diagnostics-section" eyebrow="System evidence" title="Diagnostics">
            <dl>{Object.entries(dashboard.diagnostics).map(([key, value]) => <div key={key}><dt>{key.replace(/_/g, " ")}</dt><dd>{display(value)}</dd></div>)}</dl>
          </Section>
          <Section id="activity-section" eyebrow="Recent events" title="Recent activity" error={sectionError("activity")}>
            {dashboard.activity.length ? <ol className="activity-list">{dashboard.activity.map((entry, index) => <li key={index}>
              <Activity size={15} aria-hidden="true" /><span>{[entry.created_at, entry.kind, entry.project_id, entry.status].filter(Boolean).map(String).join(" · ")}</span>
            </li>)}</ol> : <div className="empty-state">No recent bounded activity.</div>}
          </Section>
        </div>
      </>}
    </main>
    <Modal opened={dialog !== null} onClose={() => flowRef.current?.handleCancel({ preventDefault() {} })}
      returnFocus={false}
      closeOnEscape={!dialogPending} closeOnClickOutside={!dialogPending} withCloseButton={!dialogPending}
      closeButtonProps={{ "aria-label": "Close dialog" }}
      title={dialog ? `${dialog.kind[0].toUpperCase()}${dialog.kind.slice(1)} project` : "Project operation"} centered size="lg" classNames={{ content: "admin-modal-content", header: "admin-modal-header", title: "admin-modal-title", body: "admin-modal-body" }}>
      {dialog && <form onSubmit={submitDialog} autoComplete="off" className="dialog-form">
        {dialog.kind === "register" || dialog.kind === "create" ? <>
          <p>{dialog.kind === "create" ? "Create may create a directory and Git repository. An existing empty directory is used only when explicitly adopted; non-empty directories are never overwritten." : "Register an existing directory. The path remains only in this form and page memory."}</p>
          <div className="dialog-fields">
            <TextInput required label="Client ID" value={fields.client_id} onChange={(event) => setField("client_id", event.currentTarget.value)} />
            <TextInput required label="Project ID" value={fields.project_id} onChange={(event) => setField("project_id", event.currentTarget.value)} />
            <TextInput required label="Name" value={fields.name} onChange={(event) => setField("name", event.currentTarget.value)} />
            <TextInput label="Description" value={fields.description} onChange={(event) => setField("description", event.currentTarget.value)} />
            <TextInput required label="Path" value={fields.path} onChange={(event) => setField("path", event.currentTarget.value)} />
            <Checkbox label="Allow patch" checked={fields.allow_patch} onChange={(event) => setField("allow_patch", event.currentTarget.checked)} />
            {dialog.kind === "create" && <><Checkbox label="Initialize Git repository" checked={fields.git_init} onChange={(event) => setField("git_init", event.currentTarget.checked)} />
              <TextInput label="Template" value={fields.template} onChange={(event) => setField("template", event.currentTarget.value)} />
              <Checkbox label="Adopt existing empty directory" checked={fields.adopt_existing_empty} onChange={(event) => setField("adopt_existing_empty", event.currentTarget.checked)} /></>}
          </div>
        </> : <div className="action-confirmation">
          <p><strong>Project</strong><code>{dialog.target}</code></p>
          <p><strong>Revision</strong><code>{display(dialog.project?.revision)}</code></p>
          <p><strong>Active jobs</strong><span>{display(dialog.project?.active_jobs ?? 0)}</span></p>
          {dialog.kind === "disable" && <small>Already-started jobs will not be stopped. Project configuration and source directory are retained.</small>}
          {dialog.kind === "enable" && <small>This only re-enables a registered project. The Agent revalidates path policy.</small>}
          {dialog.kind === "unregister" && <><small>Only the Agent registry record is removed. The source directory and .git are not deleted. Active jobs cause rejection.</small>
            <TextInput required label="Type the full runtime project ID to confirm" value={fields.confirm_project} onChange={(event) => setField("confirm_project", event.currentTarget.value)} /></>}
        </div>}
        {dialogError && <Alert color="red" role="alert">{dialogError}</Alert>}
        <div className="dialog-actions"><Button variant="default" type="button" disabled={dialogPending} onClick={() => flowRef.current?.cancel()}>Cancel</Button>
          <Button type="submit" className="admin-primary" loading={dialogPending} aria-busy={dialogPending}>Continue</Button></div>
      </form>}
    </Modal>
  </div>;
}
