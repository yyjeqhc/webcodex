import { act, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { SessionExecution } from "../src/runtime-v2/components/SessionExecution.js";
import { workItemFromRecent } from "../src/runtime-v2/model/work.js";
import type { SessionWorkspaceState } from "../src/runtime-v2/state/useSessionWorkspace.js";
import { useSessionWorkspace } from "../src/runtime-v2/state/useSessionWorkspace.js";
import { recentSession, sessionDetail } from "./fixtures.js";

function workspace(overrides: Partial<SessionWorkspaceState> = {}): SessionWorkspaceState {
  return {
    detailAvailability: "available",
    messagesAvailability: "available",
    detail: sessionDetail(),
    messages: {
      session_id: "wc_sess_1234567890abcdef",
      messages: [
        {
          message_id: "wc_msg_note1234567890",
          kind: "note",
          status: "open",
          priority: "normal",
          created_at: 1_790_000_000,
          message: "Mutable note",
          requires_ack: false,
        },
        {
          message_id: "wc_msg_risk1234567890",
          kind: "risk",
          status: "open",
          priority: "high",
          created_at: 1_790_000_010,
          message: "Risk is not replaceable",
          requires_ack: false,
        },
      ],
    },
    sending: false,
    mutationNotice: "",
    mutationAllowed: null,
    send: vi.fn(async () => true),
    replace: vi.fn(async () => true),
    withdraw: vi.fn(async () => true),
    refresh: vi.fn(),
    ...overrides,
  };
}

describe("Session communication parity", () => {
  it("keeps Reply available while limiting Edit/Withdraw to mutable open message kinds", async () => {
    const session = workspace();
    const recent = recentSession();
    render(
      <SessionExecution
        item={workItemFromRecent(recent)}
        location={{
          projectId: recent.project_id,
          projectName: recent.project_name || recent.project_id,
          runner: recent.client_id,
          sessionId: recent.session_id,
        }}
        session={session}
        language="en"
      />,
    );

    fireEvent.click(screen.getByText("Session communication"));
    const note = screen.getByText("Mutable note").closest("article")!;
    expect(within(note).getByRole("button", { name: "Reply" })).toBeTruthy();
    expect(within(note).getByRole("button", { name: "Edit" })).toBeTruthy();
    expect(within(note).getByRole("button", { name: "Withdraw" })).toBeTruthy();

    const risk = screen.getByText("Risk is not replaceable").closest("article")!;
    expect(within(risk).getByRole("button", { name: "Reply" })).toBeTruthy();
    expect(within(risk).queryByRole("button", { name: "Edit" })).toBeNull();
    expect(within(risk).queryByRole("button", { name: "Withdraw" })).toBeNull();

    fireEvent.click(within(note).getByRole("button", { name: "Reply" }));
    expect(screen.getByText(/Replying to: Mutable note/)).toBeTruthy();
    const composer = screen.getByRole("textbox", { name: "Send a message to this work session…" });
    fireEvent.change(composer, { target: { value: "Reply body" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(session.send).toHaveBeenCalledWith(expect.objectContaining({
      message: "Reply body",
      replyTo: "wc_msg_note1234567890",
    })));
  });

  it("hides mutable actions after collaborate authority is denied", () => {
    const session = workspace({ mutationAllowed: false });
    const recent = recentSession();
    render(
      <SessionExecution
        item={workItemFromRecent(recent)}
        location={{
          projectId: recent.project_id,
          projectName: recent.project_name || recent.project_id,
          runner: recent.client_id,
          sessionId: recent.session_id,
        }}
        session={session}
        language="en"
      />,
    );
    fireEvent.click(screen.getByText("Session communication"));
    const note = screen.getByText("Mutable note").closest("article")!;
    expect(within(note).getByRole("button", { name: "Reply" })).toBeTruthy();
    expect(within(note).queryByRole("button", { name: "Edit" })).toBeNull();
    expect(within(note).queryByRole("button", { name: "Withdraw" })).toBeNull();
  });

  it("retains an explicit recovery notice when a send transport outcome is unknown", async () => {
    const client = {
      post: vi.fn(async (path: string) => {
        if (path === "workflow-session") return { ok: true, status: 200, data: sessionDetail() };
        if (path === "workflow-session-messages") {
          return { ok: true, status: 200, data: { session_id: "wc_sess_1234567890abcdef", messages: [] } };
        }
        if (path === "workflow-session-post-message") return { ok: false, status: 0, data: null };
        throw new Error("unexpected path " + path);
      }),
    } as unknown as RuntimeV2Client;
    const location = {
      projectId: "agent:special:webcodex",
      projectName: "WebCodex",
      runner: "special",
      sessionId: "wc_sess_1234567890abcdef",
    };
    const unauthorized = vi.fn();
    const { result } = renderHook(() => useSessionWorkspace(client, true, location, unauthorized));
    await waitFor(() => expect(result.current.messagesAvailability).toBe("available"));

    let sent = true;
    await act(async () => {
      sent = await result.current.send({ message: "uncertain" });
    });
    expect(sent).toBe(false);
    expect(result.current.mutationNotice).toBe(
      "Send outcome unknown. Refresh and review retained messages before retrying.",
    );
  });
});
