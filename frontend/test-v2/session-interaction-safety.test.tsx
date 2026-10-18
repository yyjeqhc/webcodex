import { act, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SessionCollaboration } from "../src/runtime-v2/components/SessionCollaboration.js";
import { SessionComposer } from "../src/runtime-v2/components/SessionComposer.js";
import { SessionInspector } from "../src/runtime-v2/components/SessionInspector.js";
import { workItemFromRecent } from "../src/runtime-v2/model/work.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { useSessionWorkspace, type SessionLocation, type SessionWorkspaceState } from "../src/runtime-v2/state/useSessionWorkspace.js";
import { loadDraft, saveDraft } from "../src/runtime_storage.js";
import { recentSession, sessionDetail } from "./fixtures.js";

const location: SessionLocation = { projectId: "project", sessionId: "session-a", runner: "runner", projectName: "Project" };
const secondLocation = { ...location, sessionId: "session-b" };
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
function workspace(overrides: Partial<SessionWorkspaceState> = {}): SessionWorkspaceState {
  return {
    detailAvailability: "available", messagesAvailability: "available", detail: null, messages: null,
    sending: false, mutationNotice: "", mutationAllowed: null,
    send: vi.fn(async () => true), replace: vi.fn(async () => true), withdraw: vi.fn(async () => true), refresh: vi.fn(),
    ...overrides,
  };
}
function enter() { fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" }); }
function edit(message = "Original edit") {
  act(() => window.dispatchEvent(new CustomEvent("webcodex-runtime-edit-message", { detail: { messageId: "message", message } })));
}

describe("Session composer submission safety", () => {
  it.each([false, true])("blocks repeated Enter and button submissions synchronously (editing=%s)", async (editing) => {
    const pending = deferred<boolean>();
    const mutation = vi.fn(() => pending.promise);
    const session = workspace(editing ? { replace: mutation } : { send: mutation });
    render(<SessionComposer location={location} session={session} language="en" />);
    if (editing) edit();
    else fireEvent.change(screen.getByRole("textbox"), { target: { value: "Draft" } });
    act(() => { enter(); enter(); fireEvent.click(screen.getByRole("button", { name: editing ? "Save" : "Send" })); });
    expect(mutation).toHaveBeenCalledTimes(1);
    expect((screen.getByRole("button", { name: editing ? "Save" : "Send" }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => pending.resolve(true));
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("");
  });
  it.each([false, true])("retains a failed draft and allows a retry (editing=%s)", async (editing) => {
    const mutation = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    render(<SessionComposer location={location} session={workspace(editing ? { replace: mutation } : { send: mutation })} language="en" />);
    if (editing) edit("Retained draft");
    else fireEvent.change(screen.getByRole("textbox"), { target: { value: "Retained draft" } });
    await act(async () => enter());
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Retained draft");
    if (!editing) expect(loadDraft(location.projectId, location.sessionId)).toBe("Retained draft");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: editing ? "Save" : "Send" })));
    expect(mutation).toHaveBeenCalledTimes(2);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("");
  });
  it.each([false, true])("keeps edits made while a request is pending (editing=%s)", async (editing) => {
    const pending = deferred<boolean>();
    render(<SessionComposer location={location} session={workspace(editing ? { replace: () => pending.promise } : { send: () => pending.promise })} language="en" />);
    if (editing) edit("Submitted");
    else fireEvent.change(screen.getByRole("textbox"), { target: { value: "Submitted" } });
    enter();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Later draft" } });
    await act(async () => pending.resolve(true));
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Later draft");
    if (!editing) expect(loadDraft(location.projectId, location.sessionId)).toBe("Later draft");
  });
  it("preserves both Sessions' drafts when a previous Session completes", async () => {
    const pending = deferred<boolean>();
    const session = workspace({ send: () => pending.promise });
    saveDraft(secondLocation.projectId, secondLocation.sessionId, "Session B draft");
    const view = render(<SessionComposer location={location} session={session} language="en" />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Session A draft" } });
    enter();
    view.rerender(<SessionComposer location={secondLocation} session={session} language="en" />);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Session B draft");
    await act(async () => pending.resolve(true));
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Session B draft");
    expect(loadDraft(location.projectId, location.sessionId)).toBe("Session A draft");
    expect(loadDraft(secondLocation.projectId, secondLocation.sessionId)).toBe("Session B draft");
  });
  it("preserves a remounted composer draft after an old request completes", async () => {
    const pending = deferred<boolean>();
    const view = render(<SessionComposer location={location} session={workspace({ send: () => pending.promise })} language="en" />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Submitted" } });
    enter();
    view.unmount();
    render(<SessionComposer location={location} session={workspace()} language="en" />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "New mount draft" } });
    await act(async () => pending.resolve(true));
    expect(loadDraft(location.projectId, location.sessionId)).toBe("New mount draft");
  });
  it.each(["reply", "compose"])("persists edited text when it becomes a %s draft in the current Session", async (action) => {
    const session = workspace({ send: vi.fn(async () => false) });
    saveDraft(location.projectId, location.sessionId, "Original Session draft");
    const view = render(<SessionComposer location={location} session={session} language="en" />);
    view.rerender(<SessionComposer location={secondLocation} session={session} language="en" />);
    edit("Edited retained text");
    act(() => window.dispatchEvent(new CustomEvent(`webcodex-runtime-${action}-message`, {
      detail: action === "reply" ? { messageId: "reply", message: "Target" } : { kind: "guidance" },
    })));
    await act(async () => enter());
    expect(loadDraft(secondLocation.projectId, secondLocation.sessionId)).toBe("Edited retained text");
    expect(loadDraft(location.projectId, location.sessionId)).toBe("Original Session draft");
    view.unmount();
    render(<SessionComposer location={secondLocation} session={session} language="en" />);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Edited retained text");
  });

  it("leaves Shift+Enter and IME composition available without submitting", () => {
    const session = workspace();
    render(<SessionComposer location={location} session={session} language="en" />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Draft" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter", shiftKey: true });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter", isComposing: true });
    expect(session.send).not.toHaveBeenCalled();
  });
});

describe("Session workspace mutation guard", () => {
  function clientWithMutation(pending: Promise<{ ok: boolean; status: number; data: object }>) {
    const post = vi.fn(async (path: string, payload: { session_id: string }) => {
      if (path === "workflow-session") return { ok: true, status: 200, data: sessionDetail({ session_id: payload.session_id }) };
      if (path === "workflow-session-messages") return { ok: true, status: 200, data: { session_id: payload.session_id, messages: [] } };
      return pending;
    });
    return { post } as unknown as RuntimeV2Client;
  }
  it("shares one synchronous guard between send and replacement", async () => {
    const pending = deferred<{ ok: boolean; status: number; data: object }>();
    const client = clientWithMutation(pending.promise);
    const unauthorized = vi.fn();
    const { result } = renderHook(() => useSessionWorkspace(client, true, location, unauthorized));
    await waitFor(() => expect(result.current.messagesAvailability).toBe("available"));
    let replacement!: Promise<boolean>;
    await act(async () => {
      replacement = result.current.replace("message", "Edit");
      expect(await result.current.send({ message: "Duplicate" })).toBe(false);
      expect(await result.current.replace("message", "Duplicate edit")).toBe(false);
    });
    expect(result.current.sending).toBe(true);
    expect(vi.mocked(client.post).mock.calls.filter(([path]) => path.includes("replace-message"))).toHaveLength(1);
    await act(async () => { pending.resolve({ ok: true, status: 200, data: {} }); expect(await replacement).toBe(true); });
    expect(result.current.sending).toBe(false);
  });
  it("rejects retained mutation callbacks for a Session that is no longer selected", async () => {
    const client = clientWithMutation(Promise.resolve({ ok: true, status: 200, data: {} }));
    const unauthorized = vi.fn();
    const { result, rerender } = renderHook(({ selected }) => useSessionWorkspace(client, true, selected, unauthorized), { initialProps: { selected: location } });
    await waitFor(() => expect(result.current.messagesAvailability).toBe("available"));
    const original = result.current;
    rerender({ selected: secondLocation });
    await waitFor(() => expect(result.current.messages?.session_id).toBe(secondLocation.sessionId));
    const calls = vi.mocked(client.post).mock.calls.length;
    await act(async () => {
      expect(await original.send({ message: "Old Session" })).toBe(false);
      expect(await original.replace("message", "Old edit")).toBe(false);
      expect(await original.withdraw("message")).toBe(false);
    });
    expect(vi.mocked(client.post).mock.calls).toHaveLength(calls);
    expect(result.current.mutationAllowed).toBeNull();
  });

  it("fences old responses even after switching back to the original Session", async () => {
    const pending = deferred<{ ok: boolean; status: number; data: object }>();
    const client = clientWithMutation(pending.promise);
    const unauthorized = vi.fn();
    const { result, rerender } = renderHook(({ selected }) => useSessionWorkspace(client, true, selected, unauthorized), { initialProps: { selected: location } });
    await waitFor(() => expect(result.current.messagesAvailability).toBe("available"));
    let original!: Promise<boolean>;
    act(() => { original = result.current.send({ message: "A" }); });
    rerender({ selected: secondLocation });
    rerender({ selected: location });
    await act(async () => { pending.resolve({ ok: false, status: 403, data: {} }); expect(await original).toBe(false); });
    expect(result.current.mutationAllowed).toBeNull();
    expect(result.current.mutationNotice).toBe("");
    expect(result.current.sending).toBe(false);
  });
});

describe("Session inspector tabs", () => {
  it("links both panels and manages roving focus with arrows and Home/End", () => {
    render(<SessionInspector item={workItemFromRecent(recentSession())} location={location} detail={null} detailAvailability="available" language="en" />);
    const context = screen.getByRole("tab", { name: "Context" });
    const evidence = screen.getByRole("tab", { name: "Evidence" });
    for (const tab of [context, evidence]) {
      const panel = document.getElementById(tab.getAttribute("aria-controls")!)!;
      expect(panel.getAttribute("role")).toBe("tabpanel");
      expect(panel.getAttribute("aria-labelledby")).toBe(tab.id);
    }
    expect(context.tabIndex).toBe(0);
    expect(evidence.tabIndex).toBe(-1);
    context.focus();
    for (const [key, selected] of [["ArrowRight", evidence], ["ArrowRight", context], ["ArrowLeft", evidence], ["Home", context], ["End", evidence]] as const) {
      fireEvent.keyDown(document.activeElement!, { key });
      expect(document.activeElement).toBe(selected);
      expect(selected.getAttribute("aria-selected")).toBe("true");
      expect(selected.tabIndex).toBe(0);
      expect(screen.getByRole("tabpanel").id).toBe(selected.getAttribute("aria-controls"));
    }
    fireEvent.click(context);
    expect(context.getAttribute("aria-selected")).toBe("true");
  });
});


it.each(["error", "stale"] as const)("explains Session message %s and retries through the existing refresh", (messagesAvailability) => {
  const session = workspace({ messagesAvailability });
  render(<SessionCollaboration location={location} session={session} language="en" />);
  expect(screen.getByRole("status").textContent).toContain(messagesAvailability === "stale"
    ? "Message refresh failed; previous messages retained." : "Session messages could not be loaded.");
  expect(screen.queryByText("No retained Session messages.")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  expect(session.refresh).toHaveBeenCalledTimes(1);
});
