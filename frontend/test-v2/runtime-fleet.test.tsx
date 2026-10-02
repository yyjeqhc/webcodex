import { act, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeV2Client } from "../src/runtime-v2/api/client.js";
import { RuntimeView } from "../src/runtime-v2/views/RuntimeView.js";
import { runtimeOverview } from "./fixtures.js";

vi.mock("../src/runtime-v2/state/useWindowWorkspace.js", () => ({
  useWindowWorkspace: () => ({ windows: [], total: 0, availability: "available", detail: null }),
}));
vi.mock("../src/runtime-v2/state/useAgentInventory.js", () => ({
  useAgentInventory: () => ({ available: true, count: 0 }),
}));

const client = {} as RuntimeV2Client;
const base = { client, language: "en-US" as const, projects: [], onUnauthorized: vi.fn() };

describe("Server-authorized Runner fleet", () => {
  it("shows remote machines even when this viewer has no local project", () => {
    const runner = runtimeOverview().runners[0];
    const overview = runtimeOverview({ runner_count: 2, runners: [
      { ...runner, client_id: "remote-B", computer_session_availability: true },
      { ...runner, client_id: "remote-C", computer_session_availability: false },
    ] });
    const { rerender } = render(<RuntimeView {...base} overview={overview} overviewAvailability="available" />);
    const fleet = within(screen.getByRole("heading", { name: "Runner fleet" }).closest("section")!);
    expect(fleet.getByText("remote-B")).toBeTruthy();
    expect(fleet.getByText("remote-C")).toBeTruthy();
    expect(screen.getByText(/GUI session available/)).toBeTruthy();
    expect(screen.getByText(/GUI session unavailable/)).toBeTruthy();
    act(() => rerender(<RuntimeView {...base} overview={overview} overviewAvailability="stale" />));
    expect(screen.queryByText(/GUI session available/)).toBeNull();
    expect(screen.getAllByText(/GUI session unavailable/)).toHaveLength(2);
  });

  it("distinguishes an authorized empty fleet from an unavailable Server", () => {
    const { rerender } = render(<RuntimeView {...base} overview={runtimeOverview({ runners: [], runner_count: 0 })} overviewAvailability="available" />);
    expect(screen.getByText("No authorized Runners yet")).toBeTruthy();
    rerender(<RuntimeView {...base} overview={null} overviewAvailability="loading" />);
    const fleet = within(screen.getByRole("heading", { name: "Runner fleet" }).closest("section")!);
    expect(fleet.getByText("Loading…")).toBeTruthy();
    expect(screen.queryByText("No authorized Runners yet")).toBeNull();
    rerender(<RuntimeView {...base} overview={null} overviewAvailability="error" />);
    expect(screen.queryByText("No authorized Runners yet")).toBeNull();
    expect(screen.getAllByText("Runtime overview unavailable").length).toBeGreaterThan(0);
  });

  it("shows reported queue and capacity while preserving incomplete inventories", () => {
    const runner = runtimeOverview().runners[0];
    const overview = runtimeOverview({
      detail_level: "primary",
      projects_available: false,
      visible_projects: 0,
      runners: [{ ...runner, jobs_queued: 3, job_concurrency_limit: 4, projects_scan_partial: true }],
    });
    const { rerender } = render(<RuntimeView {...base} overview={overview} overviewAvailability="available" onOpenWork={vi.fn()} />);
    expect(screen.getByText("Session summary is loading…")).toBeTruthy();
    expect(screen.queryByText("0 running Sessions")).toBeNull();
    expect(screen.getByText("Project inventory unavailable")).toBeTruthy();
    expect(screen.getByText("Projects").parentElement?.querySelector("strong")?.textContent).toBe("—");
    expect(screen.getByText(/3 Queued jobs/)).toBeTruthy();
    expect(screen.getByText(/Concurrency limit: 4/).textContent).toContain("Partial inventory");
    expect(screen.getByRole("button", { name: "Copy Runner" })).toBeTruthy();
    rerender(<RuntimeView {...base} overview={{ ...overview, detail_level: "full", projects_available: true, projects_truncated: true, visible_projects: 7 }} overviewAvailability="available" onOpenWork={vi.fn()} />);
    expect(screen.queryByText("Session summary is loading…")).toBeNull();
    expect(screen.getByText("0 running Sessions")).toBeTruthy();
    expect(screen.getByText("Projects").parentElement?.querySelector("strong")?.textContent).toBe("7");
    expect(screen.getByText("Partial inventory")).toBeTruthy();
  });

  it("does not show an endless loading message after an initial failure", () => {
    render(<RuntimeView {...base} overview={null} overviewAvailability="error" onOpenWork={vi.fn()} />);
    expect(screen.getByText("Not synced yet")).toBeTruthy();
    expect(screen.queryByText("Loading…")).toBeNull();
  });

});
