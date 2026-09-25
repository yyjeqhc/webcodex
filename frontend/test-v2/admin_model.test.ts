import assert from "node:assert/strict";
import { test } from "vitest";
import { capabilityLabels, emptyDashboard, mergeDashboard, semanticStatus } from "../src/admin-react/model.js";

function populated() {
  return {
    section_status: {
      overview: { status: "ok" }, devices: { status: "ok" },
      projects: { status: "ok" }, activity: { status: "ok" },
    },
    overview: { version: "1", runners_online: 2 },
    diagnostics: { server_transport: "ready" },
    devices: [{ client_id: "a", capabilities: { shell: true, patch: false, git: true } }],
    projects: [{ id: "agent:a:p", compatibility: "compatible" }],
    activity: [{ kind: "run_shell", status: "ok" }],
  };
}

test("admin dashboard retains only failed sections and accepts fresh siblings", () => {
  const first = mergeDashboard(emptyDashboard, populated());
  const next = populated();
  next.section_status.devices = { status: "error", error: "devices unavailable" };
  next.devices = [];
  next.projects.push({ id: "agent:b:p", compatibility: "unknown" });
  const merged = mergeDashboard(first, next);
  assert.deepEqual(merged.devices, first.devices);
  assert.equal(merged.errors.devices, "devices unavailable");
  assert.equal(merged.projects.length, 2);
});

test("admin status and capability display is defensive", () => {
  assert.deepEqual(capabilityLabels(["shell", 1, "git"]), ["shell", "git"]);
  assert.deepEqual(capabilityLabels({ shell: true, patch: false, git: true }), ["git", "shell"]);
  assert.deepEqual(capabilityLabels(null), []);
  assert.equal(semanticStatus("version_mismatch"), "error");
  assert.equal(semanticStatus("ready"), "good");
});
