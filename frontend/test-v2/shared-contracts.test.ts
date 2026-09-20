import { describe, expect, it, vi } from "vitest";
import {
  RUNTIME_CREDENTIAL_SESSION_KEY,
  clearRememberedRuntimeCredential,
  loadRememberedRuntimeCredential,
  persistRuntimeCredentialForTab,
} from "../src/runtime_storage.js";
import { translate } from "../src/runtime_i18n.js";
import { RuntimeApiClient } from "../src/runtime_api.js";

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
