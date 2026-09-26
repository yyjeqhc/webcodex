import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { WindowCollaboration } from "../src/runtime-v2/components/WindowCollaboration.js";

const transcript = { available: true, can_send: true, messages: [
  { message_id: "wc_msg_operator", source: "operator", direction: "inbound", message: "Check failures", created_at_ms: 1000, requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null },
  { message_id: "wc_msg_peer", source: "peer", direction: "inbound", peer_id: "wc_peer_1234567890abcdef", message: "Review done", created_at_ms: 2000, requires_ack: false, first_projected_at_ms: 2000, first_ack_observed_at_ms: null },
  { message_id: "wc_msg_peer_out", source: "peer", direction: "outbound", peer_id: "wc_peer_fedcba0987654321", message: "Sent to peer", created_at_ms: 3000, requires_ack: false, first_projected_at_ms: 3000, first_ack_observed_at_ms: null },
], truncated: false };

describe("Window collaboration", () => {
  it("sends before a Session exists and keeps Window identity when context changes", async () => {
    const post = vi.fn(async (path: string) => ({ ok: true, status: 200, data: path === "window-collaboration" ? transcript : { message_id: "wc_msg_new" } }));
    const client = { post } as unknown as RuntimeV2Client;
    const onUnauthorized = vi.fn();
    const view = render(<WindowCollaboration client={client} windowKey="exact-window" selectedSessionId="" language="en" onUnauthorized={onUnauthorized} />);
    await screen.findByText("Review done");
    expect(screen.getByText(/ · Sent$/)).toBeTruthy();
    expect(screen.getByText(/ · Delivered$/)).toBeTruthy();
    const inboundPeer = screen.getByText("Review done").closest("article");
    expect(inboundPeer?.textContent).not.toMatch(/Sent|Delivered|Acknowledged/);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "No Session needed" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("window-collaboration-post", expect.objectContaining({ client_window_key: "exact-window", context_session_id: null, message: "No Session needed" })));
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

  it("retains the exact uncertain payload across retries and context changes", async () => {
    const writes: unknown[] = [];
    const post = vi.fn(async (path: string, payload: unknown) => {
      if (path === "window-collaboration") return { ok: true, status: 200, data: transcript };
      writes.push(payload); return { ok: false, status: 503, data: null };
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
