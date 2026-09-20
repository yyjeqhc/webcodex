import { describe, expect, it, vi } from "vitest";
import {
  RUNTIME_CREDENTIAL_SESSION_KEY,
  clearRememberedRuntimeCredential,
  loadRememberedRuntimeCredential,
  persistRuntimeCredentialForTab,
} from "../src/runtime_storage.js";
import { translate } from "../src/runtime_i18n.js";
import { RuntimeApiClient } from "../src/runtime_api.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { fetchProjects } from "../src/runtime-v2/api/projects.js";
import { fetchProjectSessions, fetchSessionDetail } from "../src/runtime-v2/api/sessions.js";
import { fetchWindowDetail } from "../src/runtime-v2/api/windows.js";
import { fetchRunner } from "../src/runtime-v2/api/runtime.js";

describe("shared Runtime browser contracts", () => {
  it("keeps remembered credentials tab-scoped and clears them on lock", () => {
    persistRuntimeCredentialForTab("secret-token", true);
    expect(window.sessionStorage.getItem(RUNTIME_CREDENTIAL_SESSION_KEY)).toBe("secret-token");
    expect(window.localStorage.getItem(RUNTIME_CREDENTIAL_SESSION_KEY)).toBeNull();
    expect(loadRememberedRuntimeCredential()).toBe("secret-token");
    clearRememberedRuntimeCredential();
    expect(loadRememberedRuntimeCredential()).toBe("");
  });

  it("keeps English keys stable and localizes known Chinese strings", () => {
    expect(translate("Work", "en")).toBe("Work");
    expect(translate("Work", "zh-CN")).toBe("工作");
    expect(translate("Window Activity", "zh-CN")).toBe("窗口活动");
  });

  it("lets Runtime Console inventory endpoints own their retained-data bounds", async () => {
    const post = vi.fn(async () => ({ ok: true, status: 200, data: {} }));
    const client = { post } as unknown as RuntimeV2Client;

    await fetchProjects(client, {});
    await fetchProjectSessions(client, "agent:special:webcodex");
    await fetchSessionDetail(client, "agent:special:webcodex", "wc_sess_1234567890abcdef");
    await fetchSessionDetail(client, "agent:special:webcodex", "wc_sess_1234567890abcdef", undefined, 1);
    await fetchWindowDetail(client, "a".repeat(64));
    await fetchRunner(client, "special");

    expect(post.mock.calls[0][1]).toEqual({});
    expect(post.mock.calls[1][1]).toEqual({ project: "agent:special:webcodex" });
    expect(post.mock.calls[2][1]).toEqual({ project: "agent:special:webcodex", session_id: "wc_sess_1234567890abcdef" });
    expect(post.mock.calls[3][1]).toEqual({ project: "agent:special:webcodex", session_id: "wc_sess_1234567890abcdef", limit: 1 });
    expect(post.mock.calls[4][1]).toEqual({ client_window_key: "a".repeat(64), activity_limit: 2_000 });
    expect(post.mock.calls[5][1]).toEqual({ client_id: "special" });
  });

  it("sends bearer auth only to the configured API base and treats transport failure as status 0", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true }), { status: 200 }))
      .mockRejectedValueOnce(new TypeError("network"));
    vi.stubGlobal("fetch", fetchMock);
    const client = new RuntimeApiClient("/api/test/");
    client.setToken("token");
    const success = await client.post("path", { value: 1 });
    expect(success?.status).toBe(200);
    expect(fetchMock.mock.calls[0][0]).toBe("/api/test/path");
    expect((fetchMock.mock.calls[0][1]?.headers as Record<string, string>).Authorization).toBe("Bearer token");
    const failure = await client.post("path", {});
    expect(failure).toEqual({ ok: false, status: 0, data: null });
  });
});
