import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { AdminApp } from "../src/admin-react/AdminApp.js";
import { UiProvider } from "../src/ui/UiProvider.js";

afterEach(() => vi.unstubAllGlobals());

const dashboardFixture = {
  section_status: { overview: { status: "ok" }, devices: { status: "ok" }, projects: { status: "ok" }, activity: { status: "ok" } },
  overview: { version: "0.4.2", build_commit: "fixture", runners_online: 1, runners_total: 1, projects_online: 1, projects_total: 1 },
  diagnostics: { server_transport: "ready" }, devices: [{ client_id: "fixture-runner", status: "online" }],
  projects: [{ id: "agent:fixture:alpha", name: "Alpha", revision: "revision-1", actions: { enable: false, disable: true, unregister: true } }], activity: [],
};
const jsonResponse = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } });

test("Admin React shell unlocks, renders data, and locks without persisting the credential", async () => {
  const calls: RequestInit[] = [];
  vi.stubGlobal("fetch", vi.fn(async (_url: string, init: RequestInit) => {
    calls.push(init);
    return jsonResponse(dashboardFixture);
  }));

  render(<UiProvider><AdminApp /></UiProvider>);
  fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "fixture-secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
  expect(await screen.findByRole("heading", { name: "Operations" })).toBeTruthy();
  expect(screen.getByText("fixture-runner")).toBeTruthy();
  expect(screen.getByText("agent:fixture:alpha")).toBeTruthy();
  expect(calls[0].headers).toMatchObject({ Authorization: "Bearer fixture-secret" });
  expect(JSON.stringify(localStorage)).not.toContain("fixture-secret");
  fireEvent.click(screen.getByRole("button", { name: "Lock" }));
  await waitFor(() => expect(screen.getByRole("heading", { name: "Administrator access" })).toBeTruthy());
  expect(screen.queryByText("fixture-runner")).toBeNull();
});

test("Admin React dialog retries an uncertain mutation with the same idempotency key", async () => {
  const mutationBodies: Record<string, unknown>[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init: RequestInit) => {
    if (url.endsWith("dashboard")) return jsonResponse(dashboardFixture);
    mutationBodies.push(JSON.parse(String(init.body)));
    if (mutationBodies.length === 1) throw new Error("connection lost");
    return jsonResponse({ success: true });
  }));

  render(<UiProvider><AdminApp /></UiProvider>);
  fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "fixture-secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
  fireEvent.click(await screen.findByRole("button", { name: "Actions for Alpha" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Disable" }));
  const dialog = await screen.findByRole("dialog");
  fireEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
  expect(await within(dialog).findByText(/Network failure/)).toBeTruthy();
  fireEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
  await waitFor(() => expect(mutationBodies).toHaveLength(2));
  expect(mutationBodies[1].idempotency_key).toBe(mutationBodies[0].idempotency_key);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "Actions for Alpha" })));
});

test("Admin React lock aborts a dashboard request in flight", async () => {
  let pendingSignal: AbortSignal | undefined;
  let requestCount = 0;
  vi.stubGlobal("fetch", vi.fn(async (_url: string, init: RequestInit) => {
    requestCount += 1;
    if (requestCount === 1) return jsonResponse(dashboardFixture);
    pendingSignal = init.signal as AbortSignal;
    return new Promise<Response>(() => {});
  }));

  render(<UiProvider><AdminApp /></UiProvider>);
  fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "fixture-secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
  fireEvent.click(await screen.findByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(pendingSignal).toBeDefined());
  fireEvent.click(screen.getByRole("button", { name: "Lock" }));
  expect(pendingSignal?.aborted).toBe(true);
  expect(await screen.findByRole("heading", { name: "Administrator access" })).toBeTruthy();
});


test("Admin pagehide clears authentication and protected dashboard before a bfcache restore", async () => {
  const fetchMock = vi.fn(async () => jsonResponse(dashboardFixture));
  vi.stubGlobal("fetch", fetchMock);
  render(<UiProvider><AdminApp /></UiProvider>);
  fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "pagehide-secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
  await screen.findByRole("heading", { name: "Operations" });
  fireEvent(window, new Event("pagehide"));
  await waitFor(() => expect(screen.queryByRole("heading", { name: "Operations" })).toBeNull());
  expect(screen.getByRole("heading", { name: "Administrator access" })).toBeTruthy();
  expect(screen.queryByText("fixture-runner")).toBeNull();
  expect((screen.getByLabelText("Admin token") as HTMLInputElement).value).toBe("");
  fireEvent(window, new Event("pageshow"));
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("button", { name: "Refresh" })).toBeNull();
});

test("Admin can authorize a fresh mutation after an in-flight mutation loses authorization", async () => {
  let mutations = 0;
  vi.stubGlobal("fetch", vi.fn(async (url: string) => {
    if (url.endsWith("dashboard")) return jsonResponse(dashboardFixture);
    mutations += 1;
    return mutations === 1 ? new Response("{}", { status: 401 }) : jsonResponse({ success: true });
  }));
  render(<UiProvider><AdminApp /></UiProvider>);
  const unlock = async () => {
    fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "fresh-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
    await screen.findByRole("heading", { name: "Operations" });
  };
  const open = async () => {
    fireEvent.click(await screen.findByRole("button", { name: "Actions for Alpha" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Disable" }));
    return screen.findByRole("dialog");
  };
  await unlock();
  let dialog = await open();
  fireEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
  await screen.findByRole("heading", { name: "Administrator access" });
  await unlock();
  dialog = await open();
  const submit = within(dialog).getByRole("button", { name: "Continue" }) as HTMLButtonElement;
  expect(submit.disabled).toBe(false);
  fireEvent.click(submit);
  await waitFor(() => expect(mutations).toBe(2));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("Admin keeps normal, warning, and error statuses distinct with visible status text", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => jsonResponse({
    ...dashboardFixture,
    devices: [
      { client_id: "runner-online", status: "online" },
      { client_id: "runner-pending", status: "pending" },
      { client_id: "runner-offline", status: "offline" },
    ],
  })));

  render(<UiProvider><AdminApp /></UiProvider>);
  fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "fixture-secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
  const runners = await screen.findByRole("region", { name: "Connected Runners table. Scroll horizontally to view all fields." });
  expect(within(runners).getByText("online").closest(".admin-status-badge")?.getAttribute("data-tone")).toBe("good");
  expect(within(runners).getByText("pending").closest(".admin-status-badge")?.getAttribute("data-tone")).toBe("warning");
  expect(within(runners).getByText("offline").closest(".admin-status-badge")?.getAttribute("data-tone")).toBe("error");
});

test("Admin unlock shows pending state and first-request failure without a false empty dashboard", async () => {
  let finish!: (response: Response) => void;
  const fetchMock = vi.fn(() => new Promise<Response>(resolve => { finish = resolve; }));
  vi.stubGlobal("fetch", fetchMock);
  render(<UiProvider><AdminApp /></UiProvider>);
  const token = screen.getByLabelText("Admin token") as HTMLInputElement;
  const unlock = screen.getByRole("button", { name: "Unlock console" }) as HTMLButtonElement;
  fireEvent.change(token, { target: { value: "fixture-secret" } });
  fireEvent.click(unlock);
  expect(unlock.disabled).toBe(true);
  expect(token.disabled).toBe(true);
  expect(unlock.getAttribute("aria-busy")).toBe("true");
  fireEvent.submit(unlock.closest("form")!);
  expect(fetchMock).toHaveBeenCalledTimes(1);
  finish(new Response("{}", { status: 503 }));
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect(screen.getByRole("alert").textContent).toContain("Dashboard refresh failed");
  expect(screen.queryByRole("heading", { name: "Operations" })).toBeNull();
  await waitFor(() => expect(unlock.disabled).toBe(false));
  expect(token.value).toBe("");
});

test.each([80.25, 80.49])("Admin scroll navigation recognizes a fractional mobile anchor landing at %spx", async (top) => {
  vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(dashboardFixture)));
  vi.stubGlobal("innerWidth", 390);
  render(<UiProvider><AdminApp /></UiProvider>);
  fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "fixture-secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
  await screen.findByRole("heading", { name: "Operations" });
  for (const [id, sectionTop] of [["overview-section", -200], ["devices-section", top], ["projects-section", 1000], ["diagnostics-section", 1500]] as const) {
    vi.spyOn(document.getElementById(id)!, "getBoundingClientRect").mockReturnValue({ top: sectionTop } as DOMRect);
  }
  const runners = screen.getByRole("link", { name: "Runners" });
  expect(runners.getAttribute("aria-current")).toBeNull();
  fireEvent.scroll(window);
  await waitFor(() => expect(runners.getAttribute("aria-current")).toBe("location"));
  expect(screen.getByRole("link", { name: "Overview" }).getAttribute("aria-current")).toBeNull();
});

test("Admin unregisters an idle Runner from the fleet table and offers no removal for a live one", async () => {
  const mutations: { url: string; body: Record<string, unknown> }[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init: RequestInit) => {
    if (url.endsWith("dashboard")) return jsonResponse({
      ...dashboardFixture,
      devices: [
        { client_id: "runner-online", display_name: "Online host", status: "online", actions: { unregister: false } },
        { client_id: "gpu21-7ff4f11c8ef74468", display_name: "Old GPU host", status: "stale", project_count: 2, actions: { unregister: true } },
      ],
    });
    mutations.push({ url, body: JSON.parse(String(init.body)) });
    return jsonResponse({ outcome: "unregistered" });
  }));

  render(<UiProvider><AdminApp /></UiProvider>);
  fireEvent.change(screen.getByLabelText("Admin token"), { target: { value: "fixture-secret" } });
  fireEvent.click(screen.getByRole("button", { name: "Unlock console" }));
  await screen.findByRole("heading", { name: "Operations" });

  fireEvent.click(await screen.findByRole("button", { name: "Actions for Runner Online host" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Unregister" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(mutations).toHaveLength(0);

  fireEvent.click(screen.getByRole("button", { name: "Actions for Runner Old GPU host" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: "Unregister" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText(/Server-side registration record only/)).toBeTruthy();
  fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "gpu21-7ff4f11c8ef74468" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
  await waitFor(() => expect(mutations).toHaveLength(1));
  expect(mutations[0].url.endsWith("/api/admin/runners/unregister")).toBe(true);
  expect(mutations[0].body).toEqual({ client_id: "gpu21-7ff4f11c8ef74468", confirm: true });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});
