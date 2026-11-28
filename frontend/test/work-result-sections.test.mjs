import test from "node:test";
import assert from "node:assert/strict";
import { createWorkResultSections } from "../src/mcp-apps/work-result/sections.mjs";
import { validationLabel } from "../src/mcp-apps/work-result/checks.mjs";
import { jobOutcomeLabel } from "../src/mcp-apps/work-result/jobs.mjs";

function fixture() {
  const nodes = new Map();
  const element = tagName => ({ tagName, textContent: "", hidden: false, children: [], className: "",
    append(...children) { this.children.push(...children); },
    replaceChildren(...children) { this.children = children; this.textContent = ""; },
  });
  const el = id => { if (!nodes.has(id)) nodes.set(id, element("div")); return nodes.get(id); };
  const env = { document: { createElement: element }, el,
    safeCount: value => Number.isSafeInteger(value) && value >= 0,
    activityIdleElapsed: value => value.idle ?? null, idleAfterMs: 60_000,
    formatAge: value => `age:${value}`, formatDuration: value => `duration:${value}`,
    formatTimestamp: value => `timestamp:${value}`,
  };
  return { el, sections: createWorkResultSections(env), nodes };
}
function frozen(value) {
  if (value && typeof value === "object") { Object.values(value).forEach(frozen); Object.freeze(value); }
  return value;
}

test("bundled section instances are immutable and keep no shared DOM state", () => {
  const first = fixture(), second = fixture();
  assert.ok(Object.isFrozen(first.sections));
  assert.deepEqual(Object.keys(first.sections), ["renderValidation", "renderReview", "renderJobs", "renderWindowCurrentActivity", "renderActivityDetail", "renderCompact"]);
  assert.throws(() => { first.sections.renderJobs = () => {}; }, TypeError);
  first.sections.renderValidation(frozen({ current_status: "passed", successes: 4, unresolved_failures: 0 }));
  assert.equal(first.el("validationStatus").textContent, "验证通过");
  assert.equal(first.el("validationMeta").textContent, "通过 4");
  assert.equal(second.nodes.size, 0);
});

test("current validation evidence takes precedence over historical successes", () => {
  for (const [status, label] of [["passed", "验证通过"], ["failed", "验证需关注"], ["stale", "验证已过期"], ["not_run", "未运行验证"], ["unproven", "未运行验证"], ["expected", "已记录预期结果"], ["inconclusive", "验证无定论"], ["unknown", "验证状态不可用"]]) {
    assert.equal(validationLabel(frozen({ current_status: status, latest_status: "passed", successes: 10 })), label);
  }
  assert.equal(validationLabel({ current_status: "passed", unresolved_failures: 1 }), "验证需关注");
  const { sections, el } = fixture();
  sections.renderValidation(frozen({ current_status: "stale", successes: -1, unresolved_failures: 2, history_partial: true }));
  assert.equal(el("validationMeta").textContent, "未解决 2 · 历史不完整");
});

test("review availability and retained-history gaps never imply completion", () => {
  const { sections, el } = fixture();
  for (const [value, status, meta] of [
    [{ available: false, total: 9 }, "审查状态不可用", ""],
    [{ available: true, total: 2, history_partial: true }, "已记录审查", "有最近的审查活动"],
    [{ available: true, total: 0, history_partial: true }, "审查状态不完整", "更早的活动不在保留的历史范围内"],
    [{ available: true, total: 0 }, "未记录审查", ""],
  ]) {
    sections.renderReview(frozen(value));
    assert.equal(el("reviewStatus").textContent, status);
    assert.equal(el("reviewMeta").textContent, meta);
  }
});

test("Jobs retain active/terminal distinction, uncertainty, bounds and clear old rows", () => {
  const { sections, el } = fixture();
  const malicious = '<img src=x onerror="alert(1)">';
  sections.renderJobs(frozen({ available: true, active: true, truncated: true, items: [
    { tool: "cargo_test", state: "active", status: "stop_requested", outcome: "passed" },
    { tool: malicious, state: "terminal", status: "completed", outcome: "inconclusive" },
    { tool: "run_process", state: "terminal", status: "lost", recovery_state: "lost_after_reconcile" },
  ] }));
  assert.equal(el("jobsList").children[0].children[0].children[1].textContent, "正在停止");
  assert.equal(el("jobResultsList").children[0].children[0].children[0].textContent, malicious);
  assert.equal(el("jobResultsList").children[0].children[0].children[0].children.length, 0);
  assert.equal(el("jobResultsSummary").textContent, "最近的后台结果 · 无定论 1 · 执行丢失 1");
  assert.match(el("jobsMeta").textContent, /最多显示 8 个/);
  assert.equal(jobOutcomeLabel({ state: "terminal", status: "new_status" }), "new status");
  sections.renderJobs({ available: false });
  assert.equal(el("jobsSection").hidden, true);
  assert.equal(el("jobResults").hidden, true);
  assert.equal(el("jobsList").children.length + el("jobResultsList").children.length, 0);
});

test("activity uses the supplied clock adapters and Window observation, not Session completion", () => {
  const { sections, el } = fixture();
  sections.renderWindowCurrentActivity(frozen({ active: true, current: { label: "Reading" } }));
  assert.equal(el("activityStatus").textContent, "Reading");
  // "当前活跃" already states liveness; the detail line stays empty instead of repeating it.
  assert.equal(el("activityDetail").textContent, "");
  assert.equal(el("activityAge").textContent, "当前活跃");
  sections.renderWindowCurrentActivity(frozen({ active: false, last: { label: "Read" }, idle: 60_000 }));
  assert.equal(el("activityStatus").textContent, "WebCodex 暂无活动");
  assert.equal(el("activityAge").textContent, "已空闲 duration:60000");
  sections.renderWindowCurrentActivity(frozen({ active: false, last: { label: "Observed" }, last_activity_at_ms: 12, last_meaningful_activity_at_ms: 8 }));
  assert.equal(el("activityAge").textContent, "最近活动于 age:12");
  sections.renderWindowCurrentActivity(frozen({ active: false, coverage_partial: true }));
  assert.equal(el("activityDetail").textContent, "当前活动无法完整观察");
  assert.equal(el("activityAge").textContent, "");
});

test("detail formatting accepts zero timings and treats every dynamic field as text", () => {
  const { sections, el } = fixture();
  const value = frozen({ server_trace_id: "<script>bad()</script>", method: "tools/call", status: "success", service_ms: 0, http_status: 200, response_handed_at_ms: 0, observed_job_ids: ["job-a"], workflow_sessions: [{ workflow_session_id: "session-a" }] });
  sections.renderActivityDetail(el("detail"), value);
  const rows = el("detail").children[0].children;
  assert.deepEqual(rows.map(row => row.children.map(child => child.textContent)), [
    ["追踪", value.server_trace_id], ["方法", "tools/call"], ["状态", "成功"],
    ["服务耗时", "duration:0"], ["HTTP", "200"], ["响应移交", "timestamp:0"],
    ["已观察任务", "job-a"], ["工作流会话", "session-a"],
  ]);
  assert.ok(rows.every(row => row.children.every(child => child.children.length === 0)));
});
