import { act, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { AgentsPanel } from "../src/runtime-v2/components/AgentsPanel.js";
import { useAgentWorkspace } from "../src/runtime-v2/state/useAgentWorkspace.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { UiProvider } from "../src/ui/UiProvider.js";

const ok = (data: unknown) => ({ ok: true, status: 200, data });
const agents = [{ agent_id: "agent-a", handle: "reviewer", display_name: "Reviewer", profile_revision: 4, queued_delivery_count: 2 }];
const conversations = [{ conversation_id: "a", title: "Release review", last_seq: 1 }, { conversation_id: "b", title: "Design review", last_seq: 1 }];
function clientFor(detail: (id: string) => unknown, mutations?: (path: string, payload: any) => unknown) {
  return { post: vi.fn(async (path: string, payload: any) => {
    if (path === "communication/agents") return ok({ agents });
    if (path === "communication/conversations") return ok({ conversations });
    if (path === "communication/conversation") return detail(payload.conversation_id);
    return mutations?.(path, payload) ?? ok({});
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
