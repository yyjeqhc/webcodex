type RequestOptions = RequestInit & { headers?: HeadersInit };

declare global {
  interface Window {
    getToken: () => string;
    setToken: (token: string) => void;
    clearToken: () => void;
    requireToken: () => boolean;
    apiCall: (url: string, options?: RequestOptions) => Promise<Response | null>;
    escapeHtml: (value: unknown) => string;
    formatSize: (bytes: number) => string;
    fmtTime: (timestampSeconds: number) => string;
    deleteMsg: (id: string) => Promise<void>;
    copyText: (text: string) => void;
  }
}

const TOKEN_KEY = "drop_token";

function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) || "";
}

function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

function requireToken(): boolean {
  if (!getToken()) {
    window.location.href = "/login";
    return false;
  }
  return true;
}

async function apiCall(url: string, options: RequestOptions = {}): Promise<Response | null> {
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

function escapeHtml(value: unknown): string {
  const div = document.createElement("div");
  div.textContent = String(value ?? "");
  return div.innerHTML;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
  return `${(bytes / 1073741824).toFixed(2)} GB`;
}

function fmtTime(timestampSeconds: number): string {
  return new Date(timestampSeconds * 1000).toLocaleString();
}

async function deleteMsg(id: string): Promise<void> {
  if (!confirm("Delete this message?")) return;
  const response = await apiCall(`/api/messages/${encodeURIComponent(id)}`, { method: "DELETE" });
  if (response?.ok) window.location.reload();
}

function copyText(text: string): void {
  navigator.clipboard.writeText(text).then(() => alert("Copied!"));
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

export {};
