import { act, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CopyIdentity } from "../src/runtime-v2/components/ui/CopyIdentity.js";
import { WindowActivityFeed } from "../src/runtime-v2/components/WindowActivityFeed.js";
import { RuntimeView } from "../src/runtime-v2/views/RuntimeView.js";
import { useRuntimeOverview } from "../src/runtime-v2/state/useRuntimeOverview.js";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { runtimeOverview, windowDetail } from "./fixtures.js";

describe("Runtime usability", () => {
  it("copies the full identity and explains clipboard failures", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    const value = "window-" + "abcdef".repeat(10);
    render(<CopyIdentity value={value} label="Window" language="en" />);
    expect(screen.getByText(value)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Copy Window" }));
    await screen.findByText("Copied");
    expect(writeText).toHaveBeenCalledWith(value);
    writeText.mockRejectedValue(new Error("blocked"));
    fireEvent.click(screen.getByRole("button", { name: "Copy Window" }));
    await screen.findByText("Copy unavailable; select the text to copy.");
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it("keeps linked Sessions selectable without retained call evidence", () => {
    const detail = windowDetail({ client_window_key: "window-a" });
    detail.activity = [];
    detail.active_requests = [];
    detail.linked_sessions = [{ workflow_session_id: "session-without-calls", project: "agent:runner:project", title: "Retained work", relations: ["active"], relation_count: 1, first_linked_at_ms: 1, last_linked_at_ms: 2 }];
    const openRecord = vi.fn();
    render(<WindowActivityFeed detail={detail} projects={[]} language="en" selectedSessionId="session-without-calls" onOpenSessionRecord={openRecord} />);
    fireEvent.click(screen.getByRole("button", { name: "View Session record" }));
    expect(openRecord).toHaveBeenCalledWith("agent:runner:project", "session-without-calls");
    expect(screen.getByRole("option", { name: /Retained work/ })).toBeTruthy();
    expect(screen.getByText("This Session is linked to the Window but has no retained calls.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy Session" })).toBeTruthy();
  });

  it("refreshes on focus without cancelling a slow overview request", async () => {
    let resolve!: (value: unknown) => void;
    const post = vi.fn().mockImplementationOnce(() => new Promise(done => { resolve = done; }))
      .mockResolvedValue({ ok: true, status: 200, data: runtimeOverview() });
    const client = { post } as unknown as RuntimeV2Client;
    const unauthorized = vi.fn();
    const hook = renderHook(() => useRuntimeOverview(client, true, unauthorized));
    act(() => { window.dispatchEvent(new Event("focus")); hook.result.current.refresh(); });
    expect(post).toHaveBeenCalledTimes(1);
    expect(post.mock.calls[0][2].aborted).toBe(false);
    await act(async () => resolve({ ok: true, status: 200, data: runtimeOverview() }));
    expect(hook.result.current.updatedAt).toBeGreaterThan(0);
    act(() => window.dispatchEvent(new Event("focus")));
    await waitFor(() => expect(post).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(hook.result.current.refreshing).toBe(false));
  });

  it("shows effective configuration with readable labels and units", () => {
    const overview = runtimeOverview();
    overview.effective_config = { auth: { shared_key_enabled: true, anonymous_enabled: false }, mcp_host: { profile: "default", host_budget_secs: 60, initial_job_handoff_secs: 2, max_sync_wait_secs: 10, continuation_wait_secs: 5 }, tool_request_trace_mode: "off" };
    render(<RuntimeView client={{} as RuntimeV2Client} language="en" overview={overview} overviewAvailability="available" onOpenWork={vi.fn()} onUnauthorized={vi.fn()} />);
    expect(screen.getByText("Server configuration")).toBeTruthy();
    expect(screen.getByText("Shared key authentication")).toBeTruthy();
    expect(screen.getByText("60 seconds")).toBeTruthy();
    expect(screen.getByText("Disabled")).toBeTruthy();
  });
});
