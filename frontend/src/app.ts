// WebCodex project readiness console.
//
// The page projects the application-level readiness facts returned by
// `POST /api/connector/readiness`. It never reads the runtime registry and
// never renders Agent client ids, transport details, queue ids, or secrets.

declare global {
  interface Window {
    webcodexConsole?: unknown;
  }
}

export {};

const TOKEN_KEY = "webcodex_token";
const READINESS_URL = "/api/connector/readiness";
const REFRESH_MS = 8000;

let timer = 0;
let autoEnabled = true;

function el(id: string) {
  return document.getElementById(id);
}

function value(input: unknown) {
  return input === null || input === undefined || input === ""
    ? "—"
    : String(input);
}

function setText(id: string, input: unknown) {
  const node = el(id);
  if (node) {
    node.textContent = value(input);
  }
}

function getToken() {
  return localStorage.getItem(TOKEN_KEY) || "";
}

function clearToken() {
  localStorage.removeItem(TOKEN_KEY);
}

function showGate(message: string) {
  const gate = el("token-gate");
  const consoleRoot = el("console");
  const controls = el("topbar-controls");
  if (gate) gate.hidden = false;
  if (consoleRoot) consoleRoot.hidden = true;
  if (controls) controls.hidden = true;
  stopAuto();
  setText("token-error", message);
  const input = el("token-input") as HTMLInputElement;
  if (input) {
    input.value = "";
    input.focus();
  }
}

function showConsole() {
  const gate = el("token-gate");
  const consoleRoot = el("console");
  const controls = el("topbar-controls");
  if (gate) gate.hidden = true;
  if (consoleRoot) consoleRoot.hidden = false;
  if (controls) controls.hidden = false;
}

function showError(message: string) {
  const banner = el("error-banner");
  if (banner) {
    banner.textContent = message;
    banner.hidden = false;
  }
}

function hideError() {
  const banner = el("error-banner");
  if (banner) {
    banner.textContent = "";
    banner.hidden = true;
  }
}

async function fetchReadiness() {
  const token = getToken();
  if (!token) {
    showGate("Token required.");
    return;
  }
  const headers = new Headers();
  headers.set("Authorization", "Bearer " + token);
  headers.set("Content-Type", "application/json");
  let response;
  try {
    response = await fetch(READINESS_URL, {
      method: "POST",
      headers: headers,
      body: "{}",
    });
  } catch {
    showError("WebCodex is not reachable. Run webcodex doctor.");
    return;
  }
  if (response.status === 401) {
    clearToken();
    showGate("Token rejected. Re-enter the Bearer token.");
    return;
  }
  let body;
  try {
    body = await response.json();
  } catch {
    showError("Readiness returned invalid data.");
    return;
  }
  if (!response.ok && response.status !== 404) {
    showError("Readiness check failed (HTTP " + response.status + ").");
    return;
  }
  hideError();
  render(body);
  setText("last-updated", "Updated " + new Date().toLocaleTimeString());
}

function render(body: unknown) {
  const readiness = body as any;
  setText("project", readiness.project || "Not configured");
  setText("connection", readiness.connection);
  setText("agent", readiness.agent);
  setText("capabilities", readiness.capabilities);
  setText("coding", readiness.ready ? "Ready" : "Needs action");
  setText("next-action", readiness.next_action || "No action needed");

  const list = el("findings");
  if (!list) return;
  while (list.firstChild) list.removeChild(list.firstChild);
  const findings = Array.isArray(readiness.findings)
    ? readiness.findings
    : [];
  for (const finding of findings) {
    const item = document.createElement("li");
    const status = value(finding.status).toLowerCase();
    item.className = "finding finding-" + status;
    const title = document.createElement("div");
    title.className = "finding-title";
    title.textContent = value(finding.name) + " · " + status;
    const summary = document.createElement("div");
    summary.className = "finding-summary";
    summary.textContent = value(finding.summary);
    item.appendChild(title);
    item.appendChild(summary);
    list.appendChild(item);
  }
}

function stopAuto() {
  if (timer) {
    window.clearInterval(timer);
    timer = 0;
  }
}

function startAuto() {
  stopAuto();
  if (autoEnabled) {
    timer = window.setInterval(() => {
      void fetchReadiness();
    }, REFRESH_MS);
  }
}

function onTokenSubmit(event: SubmitEvent) {
  event.preventDefault();
  const input = el("token-input") as HTMLInputElement;
  const token = input ? input.value.trim() : "";
  if (!token) {
    setText("token-error", "Token cannot be empty.");
    return;
  }
  localStorage.setItem(TOKEN_KEY, token);
  input.value = "";
  showConsole();
  void fetchReadiness();
  startAuto();
}

function init() {
  el("token-form")?.addEventListener("submit", onTokenSubmit);
  el("refresh-btn")?.addEventListener("click", () => {
    void fetchReadiness();
  });
  el("auto-toggle")?.addEventListener("change", () => {
    const toggle = el("auto-toggle") as HTMLInputElement;
    autoEnabled = toggle.checked;
    if (autoEnabled) startAuto();
    else stopAuto();
  });
  if (getToken()) {
    showConsole();
    void fetchReadiness();
    startAuto();
  } else {
    showGate("");
  }
}

init();
