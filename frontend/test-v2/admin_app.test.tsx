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
