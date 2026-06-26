// Private Drop — read-only Runtime / Agent status console (Phase B).
//
// This script drives a single screen: a runtime status panel + an agent table.
// It fetches `POST /api/runtime/status` with a Bearer token and renders the
// result. It never displays the token, Authorization header, API keys, full
// env, or any secret — the token lives only in localStorage and is sent solely
//  request header. On 401 it clears the token and re-shows the token gate.
//
// There are no file-browse, diff, patch-approval, command-execution, or
// job-log controls here; Phase B is strictly read-only observation.
//
// NOTE on the build: `frontend/scripts/build.mjs` strips TypeScript via targeted
// regexes. To stay compatible, this file omits return-type annotations, avoids
// ` | Y` union casts, and uses only the param types the stripper recognizes
// (string/number/unknown/boolean + DOM event types). `as <Identifier>` casts
// are used only after a null check.

const TOKEN_KEY = "drop_token";
const STATUS_URL = "/api/runtime/status";
// Refresh cadence: 8s. Conservative — avoids aggressive polling while keeping
// stale/online transitions visible in near-real time.
const REFRESH_MS = 8000;

let timer = 0;
let autoEnabled = true;

// ---------------------------------------------------------------------------
// Token storage (localStorage only; never rendered into the DOM)
// ---------------------------------------------------------------------------

function getToken() {
  return localStorage.getItem(TOKEN_KEY) || "";
}

function setToken(token) {
  localStorage.setItem(TOKEN_KEY, token);
}

function clearToken() {
  localStorage.removeItem(TOKEN_KEY);
}

// ---------------------------------------------------------------------------
// DOM helpers
// ---------------------------------------------------------------------------

function el(id) {
  return document.getElementById(id);
}

function text(value) {
  if (value === null || value === undefined) {
    return "";
  }
  return String(value);
}

function fmtTime(ts) {
  const n = Number(ts);
  if (!Number.isFinite(n) || n <= 0) {
    return "—";
  }
  // server_time / last_seen are unix seconds.
  const d = new Date(n * 1000);
  return d.toLocaleString();
}

function ago(ts, now) {
  const t = Number(ts);
  const base = Number(now);
  if (!Number.isFinite(t) || !Number.isFinite(base) || t <= 0) {
    return "—";
  }
  const secs = Math.max(0, Math.floor(base - t));
  if (secs < 60) {
    return secs + "s ago";
  }
  if (secs < 3600) {
    return Math.floor(secs / 60) + "m ago";
  }
  if (secs < 86400) {
    return Math.floor(secs / 3600) + "h ago";
  }
  return Math.floor(secs / 86400) + "d ago";
}

function setText(id, value) {
  const node = el(id);
  if (node) {
    node.textContent = value;
  }
}

function showGate(message) {
  const gate = el("token-gate");
  const consoleRoot = el("console");
  const controls = el("topbar-controls");
  if (gate) {
    gate.hidden = false;
  }
  if (consoleRoot) {
    consoleRoot.hidden = true;
  }
  if (controls) {
    controls.hidden = true;
  }
  stopAuto();
  const err = el("token-error");
  if (err) {
    err.textContent = message;
  }
  const inputEl = el("token-input");
  if (inputEl) {
    const input = inputEl ;
    input.value = "";
    input.focus();
  }
}

function showConsole() {
  const gate = el("token-gate");
  const consoleRoot = el("console");
  const controls = el("topbar-controls");
  if (gate) {
    gate.hidden = true;
  }
  if (consoleRoot) {
    consoleRoot.hidden = false;
  }
  if (controls) {
    controls.hidden = false;
  }
}

function showError(message) {
  const banner = el("error-banner");
  if (banner) {
    banner.textContent = message;
    banner.hidden = false;
  }
}

function hideError() {
  const banner = el("error-banner");
  if (banner) {
    banner.hidden = true;
    banner.textContent = "";
  }
}

// ---------------------------------------------------------------------------
// Data fetch
// ---------------------------------------------------------------------------

async function fetchStatus() {
  const token = getToken();
  if (!token) {
    showGate("Token required.");
    return;
  }
  const headers = new Headers();
  headers.set("Authorization", "Bearer " + token);
  headers.set("Content-Type", "application/json");
  let res;
  try {
    res = await fetch(STATUS_URL, {
      method: "POST",
      headers: headers,
      body: "{}",
    });
  } catch {
    showError("Network error reaching " + STATUS_URL + ".");
    return;
  }
  if (res.status === 401) {
    clearToken();
    showGate("Token rejected (401). Re-enter the Bearer token.");
    return;
  }
  if (!res.ok) {
    showError("Runtime status request failed (HTTP " + res.status + ").");
    return;
  }
  let body;
  try {
    body = await res.json();
  } catch {
    showError("Runtime status returned invalid JSON.");
    return;
  }
  hideError();
  render(body);
  const stamp = el("last-updated");
  if (stamp) {
    stamp.textContent = "Updated " + new Date().toLocaleTimeString();
  }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

function render(body) {
  const data = body ;
  const out = data && data.output ? data.output : {};
  const serverTime = out.server_time;

  // --- Runtime stat grid ---
  setText(
    "stat-public-url",
    out.configured_public_url ? text(out.configured_public_url) : "(not set)",
  );
  setText("stat-auth", out.auth_enabled ? "enabled" : "disabled");
  setText("stat-version", text(out.version));
  setText("stat-server-time", fmtTime(serverTime));
  setText("stat-projects", text(out.projects && out.projects.count));
  setText("stat-active-jobs", text(out.jobs && out.jobs.active_count));
  setText("stat-tools", text(out.tools && out.tools.count));

  const agents = out.agents || {};
  const online = Number(agents.online_count) || 0;
  const stale = Number(agents.stale_count) || 0;
  const total = Number(agents.count) || 0;
  setText(
    "stat-agents",
    total + " (online " + online + " · stale " + stale + ")",
  );

  renderAgents(agents.clients || [], serverTime);
}

function renderAgents(clients, serverTime) {
  const tbody = el("agents-tbody");
  const empty = el("agents-empty");
  if (!tbody) {
    return;
  }
  // Clear previous rows.
  while (tbody.firstChild) {
    tbody.removeChild(tbody.firstChild);
  }
  const list = Array.isArray(clients) ? clients : [];
  if (empty) {
    empty.hidden = list.length > 0;
  }
  for (const client of list) {
    const c = client ;
    const tr = document.createElement("tr");
    const status = text(c.status).toLowerCase();
    const transport = text(c.transport).toLowerCase();
    // A WebSocket agent that flipped online -> stale must be visually obvious.
    const isStaleWs = status === "stale" && transport === "websocket";
    if (status === "stale") {
      tr.className = isStaleWs ? "row-stale-ws" : "row-stale";
    } else if (status === "offline") {
      tr.className = "row-offline";
    }

    tr.appendChild(tdClient(c));
    tr.appendChild(tdText(text(c.owner) || "—"));
    tr.appendChild(tdStatus(status, transport));
    tr.appendChild(tdTransport(transport));
    tr.appendChild(tdBool(c.connected));
    tr.appendChild(tdText(text(c.agent_protocol_version) || "—"));
    tr.appendChild(tdLastSeen(c.last_seen, serverTime));
    tr.appendChild(tdNum(c.pending_requests));
    tr.appendChild(tdNum(c.projects_count));
    tbody.appendChild(tr);
  }
}

function tdText(value) {
  const td = document.createElement("td");
  td.textContent = value;
  return td;
}

function tdNum(value) {
  const td = document.createElement("td");
  td.className = "num";
  td.textContent = text(value);
  return td;
}

function tdBool(value) {
  const td = document.createElement("td");
  const ok = value === true;
  const badge = document.createElement("span");
  badge.className = "badge " + (ok ? "badge-ok" : "badge-no");
  badge.textContent = ok ? "yes" : "no";
  td.appendChild(badge);
  return td;
}

function tdClient(c) {
  const cl = c ;
  const td = document.createElement("td");
  const id = document.createElement("div");
  id.className = "cell-strong";
  id.textContent = text(cl.client_id) || "—";
  td.appendChild(id);
  const name = text(cl.display_name);
  if (name) {
    const sub = document.createElement("div");
    sub.className = "cell-sub";
    sub.textContent = name;
    td.appendChild(sub);
  }
  return td;
}

function tdStatus(status, transport) {
  const td = document.createElement("td");
  const badge = document.createElement("span");
  const isStaleWs = status === "stale" && transport === "websocket";
  let cls = "badge ";
  let label = status || "unknown";
  if (status === "online") {
    cls += "badge-online";
  } else if (isStaleWs) {
    cls += "badge-stale-ws";
    label = "STALE (ws)";
  } else if (status === "stale") {
    cls += "badge-stale";
  } else if (status === "offline") {
    cls += "badge-offline";
  } else {
    cls += "badge-no";
  }
  badge.className = cls;
  badge.textContent = label;
  td.appendChild(badge);
  return td;
}

function tdTransport(transport) {
  const td = document.createElement("td");
  const badge = document.createElement("span");
  let cls = "badge ";
  if (transport === "websocket") {
    cls += "badge-ws";
  } else if (transport === "polling") {
    cls += "badge-polling";
  } else {
    cls += "badge-no";
  }
  badge.className = cls;
  badge.textContent = transport || "—";
  td.appendChild(badge);
  return td;
}

function tdLastSeen(seen, serverTime) {
  const td = document.createElement("td");
  const rel = document.createElement("div");
  rel.className = "cell-strong";
  rel.textContent = ago(seen, serverTime);
  td.appendChild(rel);
  const abs = document.createElement("div");
  abs.className = "cell-sub";
  abs.textContent = fmtTime(seen);
  td.appendChild(abs);
  return td;
}

// ---------------------------------------------------------------------------
// Auto-refresh
// ---------------------------------------------------------------------------

function startAuto() {
  stopAuto();
  if (!autoEnabled) {
    return;
  }
  timer = window.setInterval(() => {
    void fetchStatus();
  }, REFRESH_MS);
}

function stopAuto() {
  if (timer) {
    window.clearInterval(timer);
    timer = 0;
  }
}

function onAutoChange() {
  const toggleEl = el("auto-toggle");
  if (!toggleEl) {
    return;
  }
  const toggle = toggleEl ;
  autoEnabled = toggle.checked;
  if (autoEnabled) {
    startAuto();
  } else {
    stopAuto();
  }
}

// ---------------------------------------------------------------------------
// Token gate form
// ---------------------------------------------------------------------------

function onTokenSubmit(e) {
  e.preventDefault();
  const inputEl = el("token-input");
  const input = inputEl ;
  const token = input ? input.value.trim() : "";
  const err = el("token-error");
  if (!token) {
    if (err) {
      err.textContent = "Token cannot be empty.";
    }
    return;
  }
  setToken(token);
  if (err) {
    err.textContent = "";
  }
  if (input) {
    input.value = "";
  }
  showConsole();
  void fetchStatus();
  startAuto();
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

function init() {
  const form = el("token-form");
  if (form) {
    form.addEventListener("submit", onTokenSubmit);
  }
  const refreshBtn = el("refresh-btn");
  if (refreshBtn) {
    refreshBtn.addEventListener("click", () => {
      void fetchStatus();
    });
  }
  const toggle = el("auto-toggle");
  if (toggle) {
    toggle.addEventListener("change", onAutoChange);
  }
  if (getToken()) {
    showConsole();
    void fetchStatus();
    startAuto();
  } else {
    showGate("");
  }
}

init();
