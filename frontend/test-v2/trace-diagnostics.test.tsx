import { act, fireEvent, render as renderRaw, screen, waitFor } from "@testing-library/react";
import type { ReactElement } from "react";
import { expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { TraceCallDetails } from "../src/runtime-v2/components/TraceCallDetails.js";
import { RuntimeView } from "../src/runtime-v2/views/RuntimeView.js";
import { TraceSearch } from "../src/runtime-v2/components/TraceSearch.js";
import { UiProvider } from "../src/ui/UiProvider.js";

const render = (ui: ReactElement) => renderRaw(<UiProvider>{ui}</UiProvider>);
const ok = (data: unknown) => ({ ok: true, status: 200, data });
function client(post: ReturnType<typeof vi.fn>): RuntimeV2Client { return { post } as unknown as RuntimeV2Client; }

it("reads metadata only on expansion, caches it, pages and requires explicit full-payload read", async () => {
  const post = vi.fn(async (_path, request) => request.payload_index === 0
    ? ok({ payload_available: true, payload: { full_body: "explicit only" } })
    : request.offset ? ok({ status: "available", events: [{ event: "handler_returned" }], next_offset: null })
    : ok({ status: "available", trace_mode: "metadata", capture_mode: "off", events: [
      { event: "tool_trace_diagnostic", diagnostic: { command: "cargo check" } },
      { event: "tool_trace_payload_captured", payload_index: 0, payload_bytes: 20 },
    ], next_offset: 2 }));
  render(<TraceCallDetails client={client(post)} traceRef="exact-trace" language="en" />);
  expect(post).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Inspect call diagnostics" }));
  await screen.findByText(/cargo check/);
  expect(post).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "Hide call diagnostics" }));
  fireEvent.click(screen.getByRole("button", { name: "Inspect call diagnostics" }));
  expect(post).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "More trace events" }));
  await screen.findByText("handler_returned");
  expect(post.mock.calls[1][1]).toEqual({ trace_ref: "exact-trace", offset: 2, limit: 12 });
  expect(screen.queryByRole("button", { name: /Read retained full payload/ })).toBeNull();
  fireEvent.click(screen.getByText("tool_trace_payload_captured"));
  fireEvent.click(await screen.findByRole("button", { name: /Read retained full payload/ }));
  await screen.findByText(/explicit only/);
  expect(post.mock.calls[2][1]).toEqual({ trace_ref: "exact-trace", payload_index: 0 });
});

it("caps the diagnostic view at 240 events even when the last page crosses the boundary", async () => {
  const initial = Array.from({ length: 239 }, (_, index) => ({ event: `event-${index}` }));
  const overflow = Array.from({ length: 12 }, (_, index) => ({ event: `overflow-${index}` }));
  const post = vi.fn(async (_path, request) => request.offset
    ? ok({ status: "available", events: overflow, next_offset: 251 })
    : ok({ status: "available", events: initial, next_offset: 239 }));
  const { container } = render(<TraceCallDetails client={client(post)} traceRef="bounded-trace" language="en" />);
  fireEvent.click(screen.getByRole("button", { name: "Inspect call diagnostics" }));
  await screen.findByText("event-238");
  fireEvent.click(screen.getByRole("button", { name: "More trace events" }));
  await screen.findByText("overflow-0");
  expect(container.querySelectorAll(".trace-event")).toHaveLength(240);
  expect(screen.queryByText("overflow-1")).toBeNull();
  expect(screen.queryByRole("button", { name: "More trace events" })).toBeNull();
  expect(screen.getByText("Diagnostic view limit reached. Use the exact trace reader for more.")).toBeTruthy();
});

it("ignores a delayed response after the exact trace identity changes", async () => {
  let resolve: (value: unknown) => void = () => {};
  const post = vi.fn((_path, request) => request.trace_ref === "old"
    ? new Promise(done => { resolve = done; })
    : Promise.resolve(ok({ status: "available", events: [{ diagnostic: { marker: "new-evidence" } }] })));
  const api = client(post);
  const view = render(<TraceCallDetails client={api} traceRef="old" language="en" />);
  fireEvent.click(screen.getByRole("button", { name: "Inspect call diagnostics" }));
  view.rerender(<UiProvider><TraceCallDetails client={api} traceRef="new" language="en" /></UiProvider>);
  fireEvent.click(screen.getByRole("button", { name: "Inspect call diagnostics" }));
  await screen.findByText(/new-evidence/);
  await act(async () => resolve(ok({ status: "available", events: [{ diagnostic: { marker: "stale-evidence" } }] })));
  expect(screen.queryByText(/stale-evidence/)).toBeNull();
  expect(screen.getByText(/new-evidence/)).toBeTruthy();
});

it("keeps permission denials explicit without automatic retries", async () => {
  const post = vi.fn(async () => ({ ok: false, status: 403, data: null }));
  render(<TraceCallDetails client={client(post)} traceRef="denied" language="zh-CN" />);
  fireEvent.click(screen.getByRole("button", { name: "查看调用诊断" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("alert").textContent).toContain("需要管理员诊断权限");
  expect(post).toHaveBeenCalledTimes(1);
});

it("queries without a Window hash and pages with the returned effective time range", async () => {
  const query = { since_ms: 100, until_ms: 200, tool_name: "read_files" };
  const post = vi.fn(async () => ok({ status: "available", query, next_offset: 20, calls: [
    { trace_ref: "a", window_key: null, tool_name: "read_files", project: "agent:xa:p", observed_at_ms: 100 },
    { trace_ref: "b", window_key: "b".repeat(64), tool_name: "read_files", observed_at_ms: 200 },
  ] }));
  const selected = vi.fn();
  render(<TraceSearch client={client(post)} language="en" onSelectWindow={selected} />);
  fireEvent.click(screen.getByText("Find calls by time or Project"));
  expect(post).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Search retained calls" }));
  await screen.findByText("Window identity unavailable");
  fireEvent.click(screen.getByRole("button", { name: /Open Window/ }));
  expect(selected).toHaveBeenCalledWith("b".repeat(64));
  fireEvent.click(screen.getByRole("button", { name: "Next call page" }));
  await waitFor(() => expect(post).toHaveBeenCalledTimes(2));
  expect(post.mock.calls[1][1]).toEqual({ query, offset: 20, limit: 20 });
});

it("places collapsed call diagnostics under Runtime Overview and opens the exact Window", async () => {
  const post = vi.fn(async () => ok({ status: "available", calls: [{ trace_ref: "trace", window_key: "target", observed_at_ms: 100 }] }));
  const open = vi.fn();
  render(<RuntimeView client={client(post)} language="en" overview={null} overviewAvailability="available" onOpenWork={vi.fn()} onOpenWindow={open} onUnauthorized={vi.fn()} />);
  const summary = screen.getByText("Call diagnostics");
  expect(summary.closest("details")?.open).toBe(false);
  fireEvent.click(summary);
  fireEvent.click(screen.getByRole("button", { name: "Search retained calls" }));
  fireEvent.click(await screen.findByRole("button", { name: /Open Window/ }));
  expect(open).toHaveBeenCalledWith("target");
});

it("shows incomplete capture even when response and handler finish were observed", async () => {
  const post = vi.fn(async () => ok({ status: "available", coverage: "incomplete", known_dropped_events: 71,
    response_handoff_observed: true, handler_finished_observed: true,
    events: [{ event: "mcp_tool_handler_finished" }] }));
  render(<TraceCallDetails client={client(post)} traceRef="lossy" language="en" expanded />);
  expect((await screen.findByRole("alert")).textContent).toContain("Trace is incomplete");
  expect(screen.getByRole("alert").textContent).toContain("71");
  expect(screen.getByText("Capture completeness is unverified, even when a Handler finish is present.")).toBeTruthy();
});
