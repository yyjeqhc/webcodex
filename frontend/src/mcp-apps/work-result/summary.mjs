import { validationLabel } from "./checks.mjs";

// Display-only overview of already-validated facts. Each item navigates to the
// section that owns the evidence; it never infers task success from workspace
// cleanliness, job outcomes or UI visibility.
const FAILED_JOB_OUTCOMES = new Set(["failed", "lost", "timed_out"]);

export function summaryItems({ safeCount }, { state, hasFinalChanges }) {
  const items = [];
  const workspace = state.workspace;
  if (state.task_outputs) {
    items.push({ key: "outputs", label: "任务产物", value: `${state.task_outputs.verified_count} 个` });
  }
  if (hasFinalChanges) items.push({ key: "final", label: "最终变更", value: "已就绪" });
  if (workspace.git_available) {
    let value = workspace.clean ? "无未提交的变更" : `已更改 ${workspace.files_total} 个文件`;
    if (!workspace.clean && safeCount(workspace.additions) && safeCount(workspace.deletions)) {
      value += ` · +${workspace.additions} −${workspace.deletions}`;
    }
    items.push({ key: "workspace", label: "工作区", value });
  }
  if (state.validation) {
    const value = validationLabel(state.validation);
    items.push({ key: "checks", label: "验证", value, tone: value === "验证需关注" ? "attention" : value === "验证通过" ? "good" : "" });
  }
  if (state.jobs?.available) {
    const running = state.jobs.items.filter(job => job.state === "active").length;
    const failed = state.jobs.items.filter(job => job.state === "terminal"
      && FAILED_JOB_OUTCOMES.has(job.outcome || job.status)).length;
    const parts = [];
    if (running) parts.push(`运行 ${running}`);
    if (failed) parts.push(`失败 ${failed}`);
    if (parts.length) items.push({ key: "jobs", label: "后台任务", value: parts.join(" · "), tone: failed ? "attention" : "" });
  }
  return items;
}

export function createSummaryRenderer({ document, root, navigate }) {
  const buttons = new Map();
  return function render(items) {
    const keep = new Set();
    items.forEach((item, index) => {
      let button = buttons.get(item.key);
      let label = button && button.children[0], value = button && button.children[1];
      if (!button) {
        button = document.createElement("button");
        button.type = "button";
        button.dataset.key = item.key;
        button.setAttribute("data-key", item.key);
        label = document.createElement("span"); label.className = "summary-label";
        value = document.createElement("span"); value.className = "summary-value";
        button.append(label, value);
        button.onclick = () => navigate(item);
        buttons.set(item.key, button);
      }
      button.className = "summary-item" + (item.tone ? " tone-" + item.tone : "");
      if (label.textContent !== item.label) label.textContent = item.label;
      if (value.textContent !== item.value) value.textContent = item.value;
      if (root.children[index] !== button) root.insertBefore(button, root.children[index] || null);
      keep.add(item.key);
    });
    for (const [key, button] of buttons) if (!keep.has(key)) { button.remove(); buttons.delete(key); }
    root.hidden = items.length === 0;
  };
}
