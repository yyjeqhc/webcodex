import { act, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { SessionExecution } from "../src/runtime-v2/components/SessionExecution.js";
import { workItemFromRecent } from "../src/runtime-v2/model/work.js";
import type { SessionWorkspaceState } from "../src/runtime-v2/state/useSessionWorkspace.js";
import { useSessionWorkspace } from "../src/runtime-v2/state/useSessionWorkspace.js";
import { recentSession, sessionDetail, sparseActivitySessionDetail } from "./fixtures.js";

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

    expect(screen.getByRole("tab", { name: "Workflow" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: /Collaboration/ }));
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

  it("shows ACK, resolution, direct reply evidence, and Workflow activity time", () => {
    const recent = recentSession();
    const session = workspace({
      messages: {
        session_id: recent.session_id,
        messages: [
          {
            message_id: "wc_msg_guidance123456",
            kind: "guidance",
            status: "resolved",
            priority: "high",
            created_at: 1_790_000_000,
            message: "Please make the workflow easier to read.",
            requires_ack: true,
            first_ack_observed_at: 1_790_000_010,
            resolved_at: 1_790_000_020,
            resolution: "Workflow labels and details were enlarged.",
          },
          {
            message_id: "wc_msg_answer12345678",
            kind: "answer",
            status: "open",
            priority: "normal",
            created_at: 1_790_000_021,
            message: "I also kept the activity timestamps visible.",
            requires_ack: false,
            author_session_id: "wc_sess_agent123456789",
            reply_to: "wc_msg_guidance123456",
          },
        ],
      },
    });
    const rendered = render(
      <SessionExecution
        item={workItemFromRecent(recent)}
        location={{ projectId: recent.project_id, projectName: recent.project_name || recent.project_id, runner: recent.client_id, sessionId: recent.session_id }}
        session={session}
        language="en"
      />,
    );

    const activityTime = rendered.container.querySelector(".tool-cluster-time");
    expect(activityTime?.textContent).toBeTruthy();
    expect(activityTime?.textContent).not.toBe("—");

    fireEvent.click(screen.getByRole("tab", { name: /Collaboration/ }));
    expect(screen.getByText("ACK observed")).toBeTruthy();
    expect(screen.getByText("Agent resolution")).toBeTruthy();
    expect(screen.getByText("Workflow labels and details were enlarged.")).toBeTruthy();
    expect(screen.getByText("Agent / Session")).toBeTruthy();
    expect(screen.getByText("I also kept the activity timestamps visible.")).toBeTruthy();
    expect(screen.getByText("Reply to")).toBeTruthy();
  });

  it("keeps Window liveness distinct from sparse Session, Workspace, and Job evidence", () => {
    const detail = sparseActivitySessionDetail();
    const recent = recentSession({
      session_id: detail.session_id,
      title: detail.title,
      updated_at: detail.updated_at,
      running_jobs: 1,
    });
    const session = workspace({ detail });
    const rendered = render(
      <SessionExecution
        item={workItemFromRecent(recent)}
        location={{ projectId: recent.project_id, projectName: recent.project_name || recent.project_id, runner: recent.client_id, sessionId: recent.session_id }}
        session={session}
        language="en"
      />,
    );

    expect(screen.getByTestId("activity-signal-window").textContent).toContain("Last WebCodex call");
    expect(screen.getByTestId("activity-signal-session").textContent).toContain("Sparse activity");
    expect(screen.getByTestId("activity-signal-workspace").textContent).toContain("Last action");
    expect(screen.getByTestId("activity-signal-job").textContent).toContain("Running");
    expect(rendered.container.textContent).not.toMatch(/\bIdle\b/);
    expect(rendered.container.textContent).not.toMatch(/\bStopped\b/);
    expect(rendered.container.querySelectorAll(".activity-source-badge.window").length).toBeGreaterThan(0);
    expect(rendered.container.querySelectorAll(".activity-source-badge.session").length).toBeGreaterThan(0);
    expect(rendered.container.querySelectorAll(".activity-source-badge.workspace").length).toBeGreaterThan(0);
    expect(rendered.container.querySelectorAll(".activity-source-badge.job").length).toBeGreaterThan(0);
  });

  it("keeps retained unfinished calls from masquerading as live execution", () => {
    const staleCall = recentSession({
      running_call: true,
      running_jobs: 0,
      current_activity: {
        kind: "search",
        tool: "search_project_texts",
        state: "running",
        execution_state: "running",
        job_handoff: false,
        summary: "Old unmatched call",
        paths: [],
      },
    });
    const session = workspace({
      detail: sessionDetail({
        running_call: true,
        running_jobs: 0,
        current_activity: staleCall.current_activity,
      }),
    });
    render(
      <SessionExecution
        item={workItemFromRecent(staleCall)}
        location={{ projectId: staleCall.project_id, projectName: staleCall.project_name || staleCall.project_id, runner: staleCall.client_id, sessionId: staleCall.session_id }}
        session={session}
        language="en"
      />,
    );

    expect(screen.queryByText("Current execution")).toBeNull();
  });

  it("makes guidance a first-class visible collaboration action", async () => {
    const session = workspace();
    const recent = recentSession();
    render(
      <SessionExecution
        item={workItemFromRecent(recent)}
        location={{ projectId: recent.project_id, projectName: recent.project_name || recent.project_id, runner: recent.client_id, sessionId: recent.session_id }}
        session={session}
        language="en"
      />,
    );

    act(() => {
      window.dispatchEvent(new CustomEvent("webcodex-runtime-compose-message", { detail: { kind: "guidance" } }));
    });
    expect(screen.getByRole("tab", { name: /Collaboration/ }).getAttribute("aria-selected")).toBe("true");
    const composer = screen.getByRole("textbox", { name: "Send a message to this work session…" });
    fireEvent.change(composer, { target: { value: "Please prioritize the current blocker." } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(session.send).toHaveBeenCalledWith(expect.objectContaining({
      message: "Please prioritize the current blocker.",
      kind: "guidance",
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
    fireEvent.click(screen.getByRole("tab", { name: /Collaboration/ }));
    const note = screen.getByText("Mutable note").closest("article")!;
    expect(within(note).getByRole("button", { name: "Reply" })).toBeTruthy();
    expect(within(note).queryByRole("button", { name: "Edit" })).toBeNull();
    expect(within(note).queryByRole("button", { name: "Withdraw" })).toBeNull();
  });

  it("returns to Workflow when the selected Session changes", () => {
    const session = workspace();
    const recent = recentSession();
    const props = {
      item: workItemFromRecent(recent),
      location: { projectId: recent.project_id, projectName: recent.project_name || recent.project_id, runner: recent.client_id, sessionId: recent.session_id },
      session,
      language: "en" as const,
    };
    const rendered = render(<SessionExecution {...props} />);
    fireEvent.click(screen.getByRole("tab", { name: /Collaboration/ }));
    expect(screen.getByRole("tab", { name: /Collaboration/ }).getAttribute("aria-selected")).toBe("true");

    rendered.rerender(<SessionExecution {...props} location={{ ...props.location, sessionId: "wc_sess_abcdef0123456789" }} />);
    expect(screen.getByRole("tab", { name: "Workflow" }).getAttribute("aria-selected")).toBe("true");
  });

  it("clears prior Session evidence immediately when selection changes", async () => {
    const first = {
      projectId: "agent:special:webcodex",
      projectName: "WebCodex",
      runner: "special",
      sessionId: "wc_sess_1234567890abcdef",
    };
    const second = { ...first, sessionId: "wc_sess_abcdef0123456789" };
    const pending = new Promise<never>(() => {});
    const client = {
      post: vi.fn(async (path: string, payload: any) => {
        if (payload.session_id === second.sessionId) return pending;
        if (path === "workflow-session") return { ok: true, status: 200, data: sessionDetail({ session_id: first.sessionId }) };
        if (path === "workflow-session-messages") return { ok: true, status: 200, data: { session_id: first.sessionId, messages: [] } };
        throw new Error("unexpected path " + path);
      }),
    } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, rerender } = renderHook(
      ({ location }) => useSessionWorkspace(client, true, location, unauthorized),
      { initialProps: { location: first } },
    );
    await waitFor(() => expect(result.current.detail?.session_id).toBe(first.sessionId));

    rerender({ location: second });
    await waitFor(() => expect(result.current.detail).toBeNull());
    expect(result.current.messages).toBeNull();
    expect(result.current.detailAvailability).toBe("loading");
    expect(result.current.messagesAvailability).toBe("loading");
  });

  it("ignores stale mutation completion after switching Sessions", async () => {
    const first = {
      projectId: "agent:special:webcodex",
      projectName: "WebCodex",
      runner: "special",
      sessionId: "wc_sess_1234567890abcdef",
    };
    const second = { ...first, sessionId: "wc_sess_abcdef0123456789" };
    let resolveFirstSend!: (value: { ok: boolean; status: number; data: any }) => void;
    let resolveSecondSend!: (value: { ok: boolean; status: number; data: any }) => void;
    const firstSend = new Promise<{ ok: boolean; status: number; data: any }>((resolve) => {
      resolveFirstSend = resolve;
    });
    const secondSend = new Promise<{ ok: boolean; status: number; data: any }>((resolve) => {
      resolveSecondSend = resolve;
    });
    const client = {
      post: vi.fn(async (path: string, payload: any) => {
        if (path === "workflow-session") {
          return { ok: true, status: 200, data: sessionDetail({ session_id: payload.session_id }) };
        }
        if (path === "workflow-session-messages") {
          return { ok: true, status: 200, data: { session_id: payload.session_id, messages: [] } };
        }
        if (path === "workflow-session-post-message") {
          return payload.session_id === first.sessionId ? firstSend : secondSend;
        }
        throw new Error("unexpected path " + path);
      }),
    } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const { result, rerender } = renderHook(
      ({ location }) => useSessionWorkspace(client, true, location, unauthorized),
      { initialProps: { location: first } },
    );
    await waitFor(() => expect(result.current.messagesAvailability).toBe("available"));

    let firstResult!: Promise<boolean>;
    act(() => {
      firstResult = result.current.send({ message: "first Session" });
    });
    await waitFor(() => expect(result.current.sending).toBe(true));

    rerender({ location: second });
    await waitFor(() => expect(result.current.messages?.session_id).toBe(second.sessionId));
    expect(result.current.sending).toBe(false);
    expect(result.current.mutationNotice).toBe("");
    expect(result.current.mutationAllowed).toBeNull();

    let secondResult!: Promise<boolean>;
    act(() => {
      secondResult = result.current.send({ message: "second Session" });
    });
    await waitFor(() => expect(result.current.sending).toBe(true));

    await act(async () => {
      resolveFirstSend({ ok: false, status: 403, data: null });
      expect(await firstResult).toBe(false);
    });
    expect(result.current.sending).toBe(true);
    expect(result.current.mutationNotice).toBe("");
    expect(result.current.mutationAllowed).toBeNull();

    await act(async () => {
      resolveSecondSend({ ok: true, status: 200, data: {} });
      expect(await secondResult).toBe(true);
    });
    expect(result.current.sending).toBe(false);
    expect(result.current.mutationNotice).toBe("");
    expect(result.current.mutationAllowed).toBe(true);
    expect(unauthorized).not.toHaveBeenCalled();
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
