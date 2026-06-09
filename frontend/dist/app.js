const TOKEN_KEY = "drop_token";
let toastTimer = 0;

function getToken() {
  return localStorage.getItem(TOKEN_KEY) || "";
}

function setToken(token) {
  localStorage.setItem(TOKEN_KEY, token);
}

function clearToken() {
  localStorage.removeItem(TOKEN_KEY);
}

function requireToken() {
  if (!getToken()) {
    window.location.href = "/login";
    return false;
  }
  return true;
}

async function apiCall(url, options = {}) {
  const token = getToken();
  if (!token) {
    window.location.href = "/login";
    return null;
  }

  const headers = new Headers(options.headers || {});
  headers.set("Authorization", `Bearer ${token}`);

  const response = await fetch(url, { ...options, headers });
  if (response.status === 401) {
    clearToken();
    window.location.href = "/login";
    return null;
  }
  return response;
}

function escapeHtml(value) {
  const div = document.createElement("div");
  div.textContent = String(value ?? "");
  return div.innerHTML;
}

function formatSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
  return `${(bytes / 1073741824).toFixed(2)} GB`;
}

function fmtTime(timestampSeconds) {
  return new Date(timestampSeconds * 1000).toLocaleString();
}

function showToast(message, tone = "success") {
  let toast = document.getElementById("toast");
  if (!toast) {
    toast = document.createElement("div");
    toast.id = "toast";
    document.body.appendChild(toast);
  }
  toast.textContent = message;
  toast.className = `toast toast-${tone} toast-visible`;
  if (toastTimer) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    toast?.classList.remove("toast-visible");
  }, 1800);
}

async function deleteMsg(id) {
  if (!confirm("Delete this message?")) return;
  const response = await apiCall(`/api/messages/${encodeURIComponent(id)}`, { method: "DELETE" });
  if (response?.ok) window.location.reload();
}

function fallbackCopyText(text) {
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  if (!copied) throw new Error("copy failed");
}

async function copyText(text) {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
    } else {
      fallbackCopyText(text);
    }
    showToast("Copied");
  } catch (_) {
    showToast("Copy failed", "error");
  }
}

Object.assign(window, {
  getToken,
  setToken,
  clearToken,
  requireToken,
  apiCall,
  escapeHtml,
  formatSize,
  fmtTime,
  deleteMsg,
  copyText,
});
