import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { WindowCollaboration } from "../src/runtime-v2/components/WindowCollaboration.js";

const transcript = { available: true, can_send: true, messages: [
  { message_id: "wc_msg_operator", source: "operator", direction: "inbound", message: "Check failures", created_at_ms: 1000, requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null },
  { message_id: "wc_msg_peer", source: "peer", direction: "inbound", peer_id: "wc_peer_1234567890abcdef", message: "Review done", created_at_ms: 2000, requires_ack: false, first_projected_at_ms: 2000, first_ack_observed_at_ms: null },
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
});
