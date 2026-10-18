import { act, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { AgentsPanel } from "../src/runtime-v2/components/AgentsPanel.js";
import { useAgentWorkspace } from "../src/runtime-v2/state/useAgentWorkspace.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { UiProvider } from "../src/ui/UiProvider.js";

const ok = (data: unknown) => ({ ok: true, status: 200, data });
const agents = [{ agent_id: "agent-a", handle: "reviewer", display_name: "Reviewer", profile_revision: 4, queued_delivery_count: 2 }];
const conversations = [{ conversation_id: "a", title: "Release review", last_seq: 1 }, { conversation_id: "b", title: "Design review", last_seq: 1 }];
function clientFor(detail: (id: string) => unknown, mutations?: (path: string, payload: any, signal?: AbortSignal) => unknown) {
  return { post: vi.fn(async (path: string, payload: any, signal?: AbortSignal) => {
    if (path === "communication/agents") return ok({ agents });
    if (path === "communication/conversations") return ok({ conversations });
    if (path === "communication/conversation") return detail(payload.conversation_id);
    return mutations?.(path, payload, signal) ?? ok({});
  }) } as unknown as RuntimeV2Client;
}

it("loads conversation detail once and ignores a late response after switching", async () => {
  let resolveA!: (value: unknown) => void;
  const pendingA = new Promise(resolve => { resolveA = resolve; });
  const client = clientFor(id => id === "a" ? pendingA : ok({ messages: [{ message_id: "b1", body: "Current message" }] }));
  const unauthorized = vi.fn();
  const { result } = renderHook(() => useAgentWorkspace(client, true, unauthorized));
  await waitFor(() => expect(result.current.selectedConversationId).toBe("a"));
  expect(vi.mocked(client.post).mock.calls.filter(([path]) => path === "communication/conversation")).toHaveLength(1);
  act(() => result.current.selectConversation("b"));
  await waitFor(() => expect(result.current.conversationDetail?.messages?.[0].body).toBe("Current message"));
  await act(async () => resolveA(ok({ messages: [{ message_id: "a1", body: "Obsolete message" }] })));
  expect(result.current.conversationDetail?.messages?.[0].body).toBe("Current message");
});

it("lets slow Agent inventory finish across polling ticks and coalesces queued refreshes", async () => {
  vi.useFakeTimers();
  try {
    let resolveAgents!: (value: unknown) => void;
    let resolveConversations!: (value: unknown) => void;
    const slowAgents = new Promise(resolve => { resolveAgents = resolve; });
    const slowConversations = new Promise(resolve => { resolveConversations = resolve; });
    const inventorySignals: AbortSignal[] = [];
    let agentReads = 0;
    let conversationReads = 0;
    const client = {
      post: vi.fn(async (path: string, _payload: any, signal?: AbortSignal) => {
        if (path === "communication/agents") {
          if (signal) inventorySignals.push(signal);
          agentReads += 1;
          return agentReads === 1 ? slowAgents : ok({ agents });
        }
        if (path === "communication/conversations") {
          if (signal) inventorySignals.push(signal);
          conversationReads += 1;
          return conversationReads === 1 ? slowConversations : ok({ conversations });
        }
        if (path === "communication/conversation") return ok({ messages: [] });
        return ok({});
      }),
    } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useAgentWorkspace(client, true, unauthorized));

    await act(async () => {});
    expect(agentReads).toBe(1);
    expect(conversationReads).toBe(1);

    await act(async () => { await vi.advanceTimersByTimeAsync(61_000); });
    expect(agentReads).toBe(1);
    expect(conversationReads).toBe(1);
    expect(inventorySignals.every(signal => !signal.aborted)).toBe(true);

    act(() => result.current.refresh());
    expect(agentReads).toBe(1);
    expect(conversationReads).toBe(1);

    await act(async () => {
      resolveAgents(ok({ agents }));
      resolveConversations(ok({ conversations }));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(agentReads).toBe(2);
    expect(conversationReads).toBe(2);

    unmount();
  } finally {
    vi.useRealTimers();
  }
});

it("puts the inbox first, keeps global conversations separate and sends human messages to the chosen conversation", async () => {
  const client = clientFor(() => ok({ messages: [] }), path => path === "communication/message/post" ? ok({ message: {} }) : ok({}));
  render(<UiProvider><AgentsPanel client={client} language="en" onUnauthorized={vi.fn()} /></UiProvider>);
  expect(await screen.findByRole("heading", { name: "Reviewer" })).toBeTruthy();
  expect(screen.getByRole("tab", { name: "Inbox" }).getAttribute("aria-selected")).toBe("true");
  expect(screen.queryByText("Profile revision")).toBeNull();
  expect(screen.queryByText("Release review")).toBeNull();
  fireEvent.click(screen.getByRole("tab", { name: "Profile" }));
  expect(screen.getByText("Technical details").closest("details")?.open).toBe(false);
  fireEvent.click(screen.getByRole("tab", { name: "All conversations" }));
  fireEvent.click(screen.getByRole("button", { name: /Design review/ }));
  expect((screen.getByRole("checkbox", { name: /Send as/ }) as HTMLInputElement).disabled).toBe(true);
  fireEvent.change(screen.getByRole("textbox", { name: "Message" }), { target: { value: "Review the layout" } });
  fireEvent.click(screen.getByRole("button", { name: "Send message" }));
  await waitFor(() => expect(client.post).toHaveBeenCalledWith("communication/message/post", expect.objectContaining({ conversation_id: "b", author_agent_id: null, endpoint_id: null, body: "Review the layout" }), undefined));
  expect(vi.mocked(client.post).mock.calls.some(([path]) => path === "communication/endpoint/attach")).toBe(false);
});

it("keeps an unavailable conversation selected without looping or silently opening another", async () => {
  const client = clientFor(() => ({ ok: false, status: 404, data: null }));
  const unauthorized = vi.fn();
  const { result } = renderHook(() => useAgentWorkspace(client, true, unauthorized));
  await waitFor(() => expect(result.current.selectedConversationId).toBe("a"));
  await waitFor(() => expect(result.current.conversationLoading).toBe(false));
  expect(result.current.conversationDetail).toBeNull();
  expect(vi.mocked(client.post).mock.calls.filter(([path]) => path === "communication/conversation")).toHaveLength(1);
});

it("hides Agent actions when communication read access is denied", async () => {
  const client = { post: vi.fn(async () => ({ ok: false, status: 403, data: null })) } as unknown as RuntimeV2Client;
  render(<UiProvider><AgentsPanel client={client} language="en" onUnauthorized={vi.fn()} /></UiProvider>);
  expect(await screen.findByText("Durable Agent diagnostics require communication:read.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Connect as this Agent" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Send message" })).toBeNull();
});

it.each(["detached", "replaced"])("ignores renewal responses after the Endpoint is %s", async (mode) => {
  vi.useFakeTimers();
  try {
    let resolveRenew!: (value: unknown) => void;
    const renewal = new Promise(resolve => { resolveRenew = resolve; });
    let renewSignal: AbortSignal | undefined;
    let generation = 0;
    const endpoint = () => ({ endpoint_id: `endpoint-${generation}`, controller_generation: generation, lifecycle: "attached" });
    const client = clientFor(() => ok({ messages: [] }), (path) => {
      if (path === "communication/endpoint/attach") { generation++; return ok({ endpoint: endpoint() }); }
      return ok({});
    });
    const originalPost = client.post;
    client.post = vi.fn((path, payload, signal) => {
      if (path === "communication/endpoint/renew") { renewSignal = signal; return renewal; }
      return originalPost(path, payload, signal);
    }) as RuntimeV2Client["post"];
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useAgentWorkspace(client, true, unauthorized));
    await act(async () => {});
    await act(async () => { expect(await result.current.attach()).toBe(true); });
    const oldEndpoint = result.current.endpoint;
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(vi.mocked(client.post).mock.calls.filter(([path]) => path === "communication/endpoint/renew")).toHaveLength(1);
    await act(async () => { expect(await result.current.detach()).toBe(true); });
    if (mode === "replaced") {
      await act(async () => { expect(await result.current.attach()).toBe(true); });
    }
    const expectedEndpoint = result.current.endpoint;
    await act(async () => resolveRenew(mode === "detached" ? ok({ endpoint: oldEndpoint }) : { ok: false, status: 404, data: null }));
    expect(result.current.endpoint).toEqual(expectedEndpoint);
    expect(renewSignal?.aborted).toBe(true);
    expect(unauthorized).not.toHaveBeenCalled();
    unmount();
  } finally {
    vi.useRealTimers();
  }
});

it("does not overlap slow renewals and ignores their responses after disable", async () => {
  vi.useFakeTimers();
  try {
    let resolveRenew!: (value: unknown) => void;
    const renewal = new Promise(resolve => { resolveRenew = resolve; });
    const attached = { endpoint_id: "endpoint-1", controller_generation: 1, lifecycle: "attached" };
    const client = clientFor(() => ok({ messages: [] }), path => {
      if (path === "communication/endpoint/attach") return ok({ endpoint: attached });
      if (path === "communication/endpoint/renew") return renewal;
      return ok({});
    });
    const unauthorized = vi.fn();
    const { result, rerender, unmount } = renderHook(({ enabled }) => useAgentWorkspace(client, enabled, unauthorized), { initialProps: { enabled: true } });
    await act(async () => {});
    await act(async () => { await result.current.attach(); });
    await act(async () => { await vi.advanceTimersByTimeAsync(61_000); });
    expect(vi.mocked(client.post).mock.calls.filter(([path]) => path === "communication/endpoint/renew")).toHaveLength(1);
    rerender({ enabled: false });
    await act(async () => resolveRenew({ ok: false, status: 401, data: null }));
    expect(unauthorized).not.toHaveBeenCalled();
    expect(result.current.endpoint).toEqual(attached);
    unmount();
  } finally {
    vi.useRealTimers();
  }
});

it("observes a newly delivered active Agent inbox message within 10 seconds without global inventory polling", async () => {
  vi.useFakeTimers();
  let unmount = () => {};
  try {
    const attached = { endpoint_id: "endpoint-fast", controller_generation: 1, lifecycle: "attached" };
    let deliveries: Array<{ delivery_id: string }> = [];
    const client = clientFor(() => ok({ messages: [] }), path => {
      if (path === "communication/endpoint/attach") return ok({ endpoint: attached });
      if (path === "communication/inbox") return ok({ deliveries });
      return ok({});
    });
    const unauthorized = vi.fn();
    const hook = renderHook(() => useAgentWorkspace(client, true, unauthorized));
    unmount = hook.unmount;
    await act(async () => {});
    await act(async () => { expect(await hook.result.current.attach()).toBe(true); });
    const count = (path: string) => vi.mocked(client.post).mock.calls.filter(call => call[0] === path).length;
    expect(count("communication/inbox")).toBe(1);
    const inventories = count("communication/agents");
    deliveries = [{ delivery_id: "new-delivery" }];
    await act(async () => { await vi.advanceTimersByTimeAsync(9_999); });
    expect(hook.result.current.inbox).toHaveLength(0);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(count("communication/inbox")).toBe(2);
    expect(hook.result.current.inbox).toEqual(deliveries);
    expect(count("communication/agents")).toBe(inventories);
  } finally {
    unmount(); vi.useRealTimers();
  }
});

it("does not repeat an in-flight inbox request or poll without an attached Agent endpoint", async () => {
  vi.useFakeTimers();
  let unmount = () => {};
  try {
    let resolveInbox!: (value: unknown) => void;
    const pending = new Promise(resolve => { resolveInbox = resolve; });
    const attached = { endpoint_id: "endpoint-slow", controller_generation: 1, lifecycle: "attached" };
    const client = clientFor(() => ok({ messages: [] }), path => {
      if (path === "communication/endpoint/attach") return ok({ endpoint: attached });
      if (path === "communication/inbox") return pending;
      return ok({});
    });
    const unauthorized = vi.fn();
    const hook = renderHook(() => useAgentWorkspace(client, true, unauthorized));
    unmount = hook.unmount;
    await act(async () => {});
    const count = () => vi.mocked(client.post).mock.calls.filter(call => call[0] === "communication/inbox").length;
    await act(async () => { await vi.advanceTimersByTimeAsync(9_000); });
    expect(count()).toBe(0);
    await act(async () => { expect(await hook.result.current.attach()).toBe(true); });
    expect(count()).toBe(1);
    // Two fast ticks, before the independent 30s full-inventory refresh.
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    expect(count()).toBe(1);
    await act(async () => resolveInbox(ok({ deliveries: [{ delivery_id: "slow" }] })));
    expect(hook.result.current.inbox).toHaveLength(1);
  } finally {
    unmount(); vi.useRealTimers();
  }
});

it("keeps a slow inbox read alive across inventory ticks and coalesces explicit refreshes", async () => {
  vi.useFakeTimers();
  let unmount = () => {};
  try {
    let resolveInbox!: (value: unknown) => void;
    const pending = new Promise(resolve => { resolveInbox = resolve; });
    const attached = { endpoint_id: "endpoint-cross-tick", controller_generation: 1, lifecycle: "attached" };
    const signals: AbortSignal[] = [];
    let reads = 0;
    const client = clientFor(() => ok({ messages: [] }), (path, _payload, signal) => {
      if (path === "communication/endpoint/attach" || path === "communication/endpoint/renew") return ok({ endpoint: attached });
      if (path === "communication/inbox") {
        if (signal) signals.push(signal);
        reads += 1;
        if (reads === 2) return pending;
        return ok({ deliveries: reads === 1 ? [] : [{ delivery_id: "latest" }] });
      }
      return ok({});
    });
    const unauthorized = vi.fn();
    const hook = renderHook(() => useAgentWorkspace(client, true, unauthorized));
    unmount = hook.unmount;
    await act(async () => {});
    await act(async () => { expect(await hook.result.current.attach()).toBe(true); });
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(reads).toBe(2);
    // The 30s inventory tick must not abort the inbox read started at 10s.
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    expect(reads).toBe(2);
    expect(signals[1].aborted).toBe(false);
    act(() => { hook.result.current.refresh(); hook.result.current.refresh(); });
    await act(async () => {});
    expect(reads).toBe(2);
    expect(signals[1].aborted).toBe(false);
    await act(async () => resolveInbox(ok({ deliveries: [{ delivery_id: "previous-snapshot" }] })));
    // A refresh requested during the read is coalesced, not discarded.
    expect(reads).toBe(3);
    expect(hook.result.current.inbox).toEqual([{ delivery_id: "latest" }]);
  } finally {
    unmount(); vi.useRealTimers();
  }
});

it("aborts an old inbox read on Endpoint replacement and ignores its late error", async () => {
  vi.useFakeTimers();
  let unmount = () => {};
  try {
    let resolveOld!: (value: unknown) => void;
    const pending = new Promise(resolve => { resolveOld = resolve; });
    let attachments = 0;
    let oldSignal: AbortSignal | undefined;
    const client = clientFor(() => ok({ messages: [] }), (path, payload, signal) => {
      if (path === "communication/endpoint/attach") {
        attachments += 1;
        return ok({ endpoint: { endpoint_id: "endpoint-" + attachments, controller_generation: attachments, lifecycle: "attached" } });
      }
      if (path === "communication/inbox") {
        if (payload.endpoint_id === "endpoint-1") { oldSignal = signal; return pending; }
        return ok({ deliveries: [{ delivery_id: "current-endpoint" }] });
      }
      return ok({});
    });
    const unauthorized = vi.fn();
    const hook = renderHook(({ enabled }) => useAgentWorkspace(client, enabled, unauthorized), { initialProps: { enabled: true } });
    unmount = hook.unmount;
    await act(async () => {});
    await act(async () => { expect(await hook.result.current.attach()).toBe(true); });
    expect(oldSignal?.aborted).toBe(false);
    await act(async () => { expect(await hook.result.current.detach()).toBe(true); });
    expect(oldSignal?.aborted).toBe(true);
    await act(async () => { expect(await hook.result.current.attach()).toBe(true); });
    expect(hook.result.current.endpoint?.endpoint_id).toBe("endpoint-2");
    await act(async () => resolveOld({ ok: false, status: 400 }));
    expect(hook.result.current.endpoint?.endpoint_id).toBe("endpoint-2");
    expect(hook.result.current.inbox).toEqual([{ delivery_id: "current-endpoint" }]);
    hook.rerender({ enabled: false });
    await act(async () => { await vi.advanceTimersByTimeAsync(60_000); });
    expect(hook.result.current.inbox).toEqual([]);
    expect(unauthorized).not.toHaveBeenCalled();
  } finally {
    unmount(); vi.useRealTimers();
  }
});

it("does not launch a queued inbox refresh after the view becomes hidden", async () => {
  vi.useFakeTimers();
  const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  let unmount = () => {};
  try {
    let resolveInbox!: (value: unknown) => void;
    const pending = new Promise(resolve => { resolveInbox = resolve; });
    const attached = { endpoint_id: "endpoint-queued-hidden", controller_generation: 1, lifecycle: "attached" };
    let reads = 0;
    const client = clientFor(() => ok({ messages: [] }), path => {
      if (path === "communication/endpoint/attach" || path === "communication/endpoint/renew") return ok({ endpoint: attached });
      if (path === "communication/inbox") { reads += 1; return reads === 1 ? pending : ok({ deliveries: [] }); }
      return ok({});
    });
    const unauthorized = vi.fn();
    const hook = renderHook(() => useAgentWorkspace(client, true, unauthorized));
    unmount = hook.unmount;
    await act(async () => {});
    await act(async () => { expect(await hook.result.current.attach()).toBe(true); });
    act(() => hook.result.current.refresh());
    await act(async () => {});
    expect(reads).toBe(1);
    visibility.mockReturnValue("hidden");
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    await act(async () => resolveInbox(ok({ deliveries: [] })));
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(reads).toBe(1);
    visibility.mockReturnValue("visible");
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    await act(async () => { await vi.advanceTimersByTimeAsync(150); });
    expect(reads).toBe(2);
  } finally {
    unmount(); visibility.mockRestore(); vi.useRealTimers();
  }
});

it("hidden pages pause inventory observations without pausing the current Endpoint lease", async () => {
  vi.useFakeTimers();
  const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  let unmount = () => {};
  try {
    const attached = { endpoint_id: "endpoint-hidden", controller_generation: 1, lifecycle: "attached" };
    const client = clientFor(() => ok({ messages: [] }), path => {
      if (path === "communication/endpoint/attach" || path === "communication/endpoint/renew") return ok({ endpoint: attached });
      return ok({});
    });
    const unauthorized = vi.fn();
    const hook = renderHook(() => useAgentWorkspace(client, true, unauthorized));
    unmount = hook.unmount;
    await act(async () => {});
    await act(async () => { expect(await hook.result.current.attach()).toBe(true); });
    const count = (path: string) => vi.mocked(client.post).mock.calls.filter(call => call[0] === path).length;
    const inventoryReads = count("communication/agents");
    const inboxReads = count("communication/inbox");
    visibility.mockReturnValue("hidden");
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(count("communication/agents")).toBe(inventoryReads);
    expect(count("communication/inbox")).toBe(inboxReads);
    expect(count("communication/endpoint/renew")).toBe(2);
    expect(hook.result.current.endpoint?.endpoint_id).toBe(attached.endpoint_id);
    visibility.mockReturnValue("visible");
    act(() => { document.dispatchEvent(new Event("visibilitychange")); window.dispatchEvent(new Event("focus")); });
    await act(async () => { await vi.advanceTimersByTimeAsync(150); });
    expect(count("communication/agents")).toBe(inventoryReads + 1);
    expect(count("communication/inbox")).toBe(inboxReads + 1);
  } finally {
    unmount(); visibility.mockRestore(); vi.useRealTimers();
  }
});

it("continues renewing the current Endpoint after a successful heartbeat", async () => {
  vi.useFakeTimers();
  try {
    const attached = { endpoint_id: "endpoint-1", controller_generation: 1, lifecycle: "attached", lease_expires_at_unix_ms: 100 };
    let renewals = 0;
    const client = clientFor(() => ok({ messages: [] }), path => {
      if (path === "communication/endpoint/attach") return ok({ endpoint: attached });
      if (path === "communication/endpoint/renew") {
        renewals++;
        return ok({ endpoint: { ...attached, lease_expires_at_unix_ms: 100 + renewals } });
      }
      return ok({});
    });
    const unauthorized = vi.fn();
    const { result, unmount } = renderHook(() => useAgentWorkspace(client, true, unauthorized));
    await act(async () => {});
    await act(async () => { await result.current.attach(); });
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(result.current.endpoint?.lease_expires_at_unix_ms).toBe(101);
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(result.current.endpoint?.lease_expires_at_unix_ms).toBe(102);
    expect(renewals).toBe(2);
    unmount();
  } finally {
    vi.useRealTimers();
  }
});

it("distinguishes initial Agent loading, failure, empty success and stale retained data", async () => {
  let finish!: (value: unknown) => void;
  let mode: "pending" | "error" | "empty" | "ready" = "pending";
  const client = { post: vi.fn(async (path: string) => {
    if (path === "communication/agents") {
      if (mode === "pending") return new Promise(resolve => { finish = resolve; });
      if (mode === "error") return { ok: false, status: 503 };
      return ok({ agents: mode === "empty" ? [] : agents });
    }
    if (path === "communication/conversations") return ok({ conversations: [] });
    return ok({});
  }) } as unknown as RuntimeV2Client;
  const unauthorized = vi.fn();
  const { result } = renderHook(() => useAgentWorkspace(client, true, unauthorized));
  expect(result.current.availability).toBe("loading");
  await act(async () => finish({ ok: false, status: 503 }));
  await waitFor(() => expect(result.current.availability).toBe("error"));
  expect(result.current.status).not.toContain("previous data retained");
  mode = "empty";
  act(() => result.current.refresh());
  await waitFor(() => expect(result.current.availability).toBe("available"));
  expect(result.current.agents).toEqual([]);
  mode = "ready";
  act(() => result.current.refresh());
  await waitFor(() => expect(result.current.agents).toHaveLength(1));
  mode = "error";
  act(() => result.current.refresh());
  await waitFor(() => expect(result.current.availability).toBe("stale"));
  expect(result.current.agents[0].display_name).toBe("Reviewer");
});

it("does not prompt Agent creation while the first inventory read is pending or failed", async () => {
  let finish!: (value: unknown) => void;
  const client = { post: vi.fn(async (path: string) => path === "communication/agents"
    ? new Promise(resolve => { finish = resolve; }) : ok({ conversations: [] })) } as unknown as RuntimeV2Client;
  render(<UiProvider><AgentsPanel client={client} language="en" onUnauthorized={vi.fn()} /></UiProvider>);
  expect(screen.getAllByText("Loading Agents…").length).toBeGreaterThan(0);
  expect(screen.queryByText("No durable Agents are visible.")).toBeNull();
  expect(screen.queryByText("Select or create an Agent to get started.")).toBeNull();
  await act(async () => finish({ ok: false, status: 503 }));
  expect((await screen.findAllByText("Agents could not be loaded. Refresh to retry.")).length).toBeGreaterThan(0);
  expect(screen.queryByText("No durable Agents are visible.")).toBeNull();
});
