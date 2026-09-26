import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { WindowCollaboration } from "../src/runtime-v2/components/WindowCollaboration.js";

const transcript = { available: true, can_send: true, messages: [
  { message_id: "wc_msg_operator", source: "operator", direction: "inbound", message: "Check failures", created_at_ms: 1000, kind: "guidance", priority: "normal", requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null },
  { message_id: "wc_msg_window", source: "window", direction: "outbound", message: "Reply from this Window", created_at_ms: 1500, reply_to_message_id: "wc_msg_operator", kind: "answer", priority: "normal", requires_ack: false, first_projected_at_ms: null, first_ack_observed_at_ms: null },
  { message_id: "wc_msg_peer", source: "peer", direction: "inbound", peer_id: "wc_peer_1234567890abcdef", message: "Review done", created_at_ms: 2000, kind: "progress", priority: "normal", requires_ack: false, first_projected_at_ms: 2000, first_ack_observed_at_ms: null },
  { message_id: "wc_msg_peer_out", source: "peer", direction: "outbound", peer_id: "wc_peer_fedcba0987654321", message: "Sent to peer", created_at_ms: 3000, kind: "progress", priority: "normal", requires_ack: false, first_projected_at_ms: 3000, first_ack_observed_at_ms: null },
], truncated: false };

describe("Window collaboration", () => {
  it("polls only while visible and refreshes immediately on return", async () => {
    vi.useFakeTimers();
    let visibility: DocumentVisibilityState = "visible";
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
    const post = vi.fn(async () => ({ ok: true, status: 200, data: transcript }));
    const client = { post } as unknown as RuntimeV2Client;
    const onUnauthorized = vi.fn();
    const panel = (active: boolean) => <WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={onUnauthorized} active={active} />;
    const view = render(panel(false));
    try {
      await act(async () => { await vi.advanceTimersByTimeAsync(9000); });
      expect(post).not.toHaveBeenCalled();
      await act(async () => { view.rerender(panel(true)); });
      expect(post).toHaveBeenCalledTimes(1);
      fireEvent.change(screen.getByRole("textbox"), { target: { value: "Keep my draft" } });
      await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
      expect(post).toHaveBeenCalledTimes(2);
      view.rerender(panel(false));
      await act(async () => { await vi.advanceTimersByTimeAsync(9000); });
      expect(post).toHaveBeenCalledTimes(2);
      await act(async () => { view.rerender(panel(true)); });
      expect(post).toHaveBeenCalledTimes(3);
      expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Keep my draft");
      visibility = "hidden";
      fireEvent(document, new Event("visibilitychange"));
      await act(async () => { await vi.advanceTimersByTimeAsync(9000); });
      expect(post).toHaveBeenCalledTimes(3);
      visibility = "visible";
      await act(async () => { fireEvent(document, new Event("visibilitychange")); });
      expect(post).toHaveBeenCalledTimes(4);
    } finally {
      view.unmount();
      vi.useRealTimers();
    }
  });

  it("aborts hidden reads and ignores their late responses after reopening", async () => {
    const reads: { signal?: AbortSignal; resolve: (response: any) => void }[] = [];
    const post = vi.fn((_path: string, _payload: unknown, signal?: AbortSignal) =>
      new Promise(resolve => { reads.push({ signal, resolve }); }));
    const client = { post } as unknown as RuntimeV2Client;
    const onUnauthorized = vi.fn();
    const panel = (active: boolean) => <WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={onUnauthorized} active={active} />;
    const view = render(panel(true));
    expect(reads).toHaveLength(1);
    view.rerender(panel(false));
    expect(reads[0].signal?.aborted).toBe(true);
    view.rerender(panel(true));
    expect(reads).toHaveLength(2);
    await act(async () => { reads[1].resolve({ ok: true, status: 200, data: transcript }); });
    expect(screen.getByText("Review done")).toBeTruthy();
    await act(async () => { reads[0].resolve({ ok: false, status: 403, data: null }); });
    expect(screen.getByText("Review done")).toBeTruthy();
    view.unmount();
  });

  it("ignores a read that resolves after the page becomes hidden but before the visibility event", async () => {
    let visibility: DocumentVisibilityState = "visible";
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
    const reads: { signal?: AbortSignal; resolve: (response: any) => void }[] = [];
    const post = vi.fn((_path: string, _payload: unknown, signal?: AbortSignal) =>
      new Promise(resolve => { reads.push({ signal, resolve }); }));
    const client = { post } as unknown as RuntimeV2Client;
    const view = render(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={vi.fn()} />);
    expect(reads).toHaveLength(1);

    visibility = "hidden";
    expect(reads[0].signal?.aborted).toBe(false);
    await act(async () => { reads[0].resolve({ ok: true, status: 200, data: transcript }); });
    expect(screen.queryByText("Review done")).toBeNull();

    visibility = "visible";
    fireEvent(document, new Event("visibilitychange"));
    expect(reads).toHaveLength(2);
    await act(async () => { reads[1].resolve({ ok: true, status: 200, data: transcript }); });
    expect(screen.getByText("Review done")).toBeTruthy();
    view.unmount();
  });

  it("retains an in-flight send and its retry identity while hidden", async () => {
    let finishSend!: (value: any) => void;
    const writes: unknown[] = [];
    const post = vi.fn(async (path: string, payload: unknown) => {
      if (path === "window-collaboration") return { ok: true, status: 200, data: transcript };
      writes.push(payload);
      if (writes.length === 1) return new Promise(resolve => { finishSend = resolve; });
      return { ok: true, status: 200, data: { message_id: "wc_msg_retry" } };
    });
    const client = { post } as unknown as RuntimeV2Client;
    const onUnauthorized = vi.fn();
    const panel = (active: boolean) => <WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={onUnauthorized} active={active} />;
    const view = render(panel(true));
    await screen.findByText("Review done");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Retry exactly" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    view.rerender(panel(false));
    await act(async () => { finishSend({ ok: false, status: 503, data: null }); });
    view.rerender(panel(true));
    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    await waitFor(() => expect(writes).toHaveLength(2));
    expect(writes[1]).toEqual(writes[0]);
    await waitFor(() => expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(""));
    view.unmount();
  });

  it("sends before a Session exists and keeps Window identity when context changes", async () => {
    const post = vi.fn(async (path: string) => ({ ok: true, status: 200, data: path === "window-collaboration" ? transcript : { message_id: "wc_msg_new" } }));
    const client = { post } as unknown as RuntimeV2Client;
    const onUnauthorized = vi.fn();
    const view = render(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={onUnauthorized} />);
    await screen.findByText("Review done");
    expect(screen.getByText("Saved")).toBeTruthy();
    expect(screen.getByText("Included in tool result")).toBeTruthy();
    expect(screen.getByText("4 messages")).toBeTruthy();
    const windowReply = screen.getByText("Reply from this Window").closest("article");
    expect(windowReply?.textContent).toContain("This Window");
    expect(windowReply?.textContent).not.toMatch(/Saved|Included in tool result|Acknowledged/);
    expect(screen.getByText("⌘/Ctrl + Enter to send")).toBeTruthy();
    const inboundPeer = screen.getByText("Review done").closest("article");
    expect(inboundPeer?.textContent).not.toMatch(/Saved|Included in tool result|Acknowledged/);
    fireEvent.change(screen.getByLabelText("Message type"), { target: { value: "question" } });
    fireEvent.change(screen.getByLabelText("Message priority"), { target: { value: "high" } });
    fireEvent.click(screen.getByLabelText("Require ACK"));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "No Session needed" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("window-collaboration-post", expect.objectContaining({
      client_window_key: "exact-window",
      context_session_id: null,
      message: "No Session needed",
      kind: "question",
      priority: "high",
      requires_ack: false,
    })));
    await waitFor(() => expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(""));
    view.rerender(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="wc_sess_context" language="en" onUnauthorized={onUnauthorized} />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Context only" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("window-collaboration-post", expect.objectContaining({ client_window_key: "exact-window", context_session_id: "wc_sess_context" })));
    view.unmount();
  });

  it("clears deterministic conflict identity and uses a new key on the next explicit send", async () => {
    const writes: any[] = [];
    const post = vi.fn(async (path: string, payload: any) => {
      if (path === "window-collaboration") return { ok: true, status: 200, data: transcript };
      writes.push(payload);
      return { ok: false, status: 409, data: { success: false, output: { failure_kind: "conflict", state_changed: false } } };
    });
    const client = { post } as unknown as RuntimeV2Client;
    const view = render(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="wc_sess_context" language="en" onUnauthorized={vi.fn()} />);
    await screen.findByText("Review done");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Keep draft" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText(/Message could not be sent/);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Keep draft");
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(writes.length).toBe(2));
    expect(writes[1].delivery_key).not.toBe(writes[0].delivery_key);
    view.unmount();
  });

  it("preserves an invalid-context draft but rebuilds context only on the next explicit send", async () => {
    const writes: any[] = [];
    let attempts = 0;
    const post = vi.fn(async (path: string, payload: any) => {
      if (path === "window-collaboration") return { ok: true, status: 200, data: transcript };
      writes.push(payload);
      attempts += 1;
      return attempts === 1
        ? { ok: false, status: 400, data: { success: false, output: { failure_kind: "invalid_context", error_kind: "session_context_unlinked" } } }
        : { ok: true, status: 200, data: { message_id: "wc_msg_new" } };
    });
    const client = { post } as unknown as RuntimeV2Client;
    const view = render(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="wc_sess_old" language="en" onUnauthorized={vi.fn()} />);
    await screen.findByText("Review done");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Keep context draft" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText(/Context is no longer available/);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Keep context draft");
    view.rerender(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="wc_sess_new" language="en" onUnauthorized={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(writes.length).toBe(2));
    expect(writes[0].context_session_id).toBe("wc_sess_old");
    expect(writes[1].context_session_id).toBe("wc_sess_new");
    expect(writes[1].delivery_key).not.toBe(writes[0].delivery_key);
    await waitFor(() => expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(""));
    view.unmount();
  });

  it("keeps ordinary deterministic 400 failures distinct from stale context", async () => {
    const post = vi.fn(async (path: string) => path === "window-collaboration"
      ? { ok: true, status: 200, data: transcript }
      : { ok: false, status: 400, data: { success: false, error: "invalid message or delivery_key" } });
    const view = render(<WindowCollaboration client={{ post } as unknown as RuntimeV2Client} windowKey="exact-window" selectedSessionId="wc_sess_context" language="en" onUnauthorized={vi.fn()} />);
    await screen.findByText("Review done");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Keep draft" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText("Message could not be sent.");
    expect(screen.queryByText(/Context is no longer available/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    view.unmount();
  });

  it.each([
    { ok: false, status: 0, data: null },
    { ok: false, status: 500, data: null },
    { ok: false, status: 502, data: null },
    { ok: false, status: 503, data: null },
    { ok: false, status: 504, data: null },
    { ok: true, status: 200, data: null },
    { ok: true, status: 200, data: { message_id: 123 } },
    null,
  ])("retains the exact uncertain payload across retries and context changes: %j", async response => {
    const writes: unknown[] = [];
    const post = vi.fn(async (path: string, payload: unknown) => {
      if (path === "window-collaboration") return { ok: true, status: 200, data: transcript };
      writes.push(payload); return response;
    });
    const client = { post } as unknown as RuntimeV2Client;
    const onUnauthorized = vi.fn();
    const view = render(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={onUnauthorized} />);
    await screen.findByText("Review done");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Keep this" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByRole("button", { name: "Retry" });
    view.rerender(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="wc_sess_later" language="en" onUnauthorized={onUnauthorized} />);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(writes.length).toBe(2));
    expect(writes[1]).toEqual(writes[0]);
    view.unmount();
  });

  it("does not offer exact retry when the Window is unavailable", async () => {
    const post = vi.fn(async (path: string) => path === "window-collaboration"
      ? { ok: true, status: 200, data: transcript }
      : { ok: false, status: 404, data: null });
    const view = render(<WindowCollaboration client={{ post } as unknown as RuntimeV2Client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={vi.fn()} />);
    await screen.findByText("Review done");
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Unavailable" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByText("Collaboration is unavailable for this Window.");
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
    view.unmount();
  });
});

it("keeps the reader's scroll position when a bounded transcript gains a new message", async () => {
  vi.useFakeTimers();
  try {
    let current = transcript;
    const post = vi.fn(async () => ({ ok: true, status: 200, data: current }));
    const client = { post } as unknown as RuntimeV2Client;
    const onUnauthorized = vi.fn();
    const view = render(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={onUnauthorized} />);

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByText("Review done")).toBeTruthy();

    const thread = view.container.querySelector(".window-collaboration-thread") as HTMLDivElement;
    Object.defineProperties(thread, {
      scrollHeight: { value: 1000, configurable: true },
      clientHeight: { value: 200, configurable: true },
    });
    thread.scrollTop = 100;
    fireEvent.scroll(thread);

    current = {
      ...transcript,
      messages: [
        ...transcript.messages.slice(1),
        { ...transcript.messages[0], message_id: "wc_msg_latest", message: "New reply" },
      ],
    };
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });

    expect(screen.getByText("New reply")).toBeTruthy();
    expect(thread.scrollTop).toBe(100);
    fireEvent.click(screen.getByRole("button", { name: "View new messages" }));
    expect(thread.scrollTop).toBe(1000);
    expect(screen.queryByRole("button", { name: "View new messages" })).toBeNull();
    view.unmount();
  } finally {
    vi.useRealTimers();
  }
});

it("does not send while an input method is composing", async () => {
  const post = vi.fn(async () => ({ ok: true, status: 200, data: transcript }));
  render(<WindowCollaboration client={{ post } as unknown as RuntimeV2Client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={vi.fn()} />);
  await screen.findByText("Review done");
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "正在输入" } });
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter", ctrlKey: true, isComposing: true });
  expect(post.mock.calls.some(([path]) => path === "window-collaboration-post")).toBe(false);
});
