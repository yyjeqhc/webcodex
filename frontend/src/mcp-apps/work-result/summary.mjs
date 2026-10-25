import { validationLabel } from "./checks.mjs";

// Display-only overview of already-validated facts. Each item navigates to the
// section that owns the evidence; it never infers task success from workspace
// cleanliness, job outcomes or UI visibility.
const FAILED_JOB_OUTCOMES = new Set(["failed", "lost", "timed_out"]);

export function summaryItems({ safeCount }, { state, hasFinalChanges }) {
  const items = [];
  const workspace = state.workspace;
  if (state.task_outputs) {
    const count = state.task_outputs.verified_count;
    items.push({ key: "outputs", text: `${count} task ${count === 1 ? "output" : "outputs"}` });
  }
  if (hasFinalChanges) items.push({ key: "final", text: "Final changes ready" });
  if (workspace.git_available) {
    let text = workspace.clean ? "No uncommitted changes"
      : `${workspace.files_total} ${workspace.files_total === 1 ? "file" : "files"} changed`;
    if (!workspace.clean && safeCount(workspace.additions) && safeCount(workspace.deletions)) {
      text += ` · +${workspace.additions} −${workspace.deletions}`;
    }
    items.push({ key: "workspace", text });
  }
  if (state.validation) {
    const text = validationLabel(state.validation);
    items.push({ key: "checks", text, tone: text === "Checks need attention" ? "attention" : text === "Checks passed" ? "good" : "" });
  }
  if (state.jobs?.available) {
    const running = state.jobs.items.filter(job => job.state === "active").length;
    const failed = state.jobs.items.filter(job => job.state === "terminal"
      && FAILED_JOB_OUTCOMES.has(job.outcome || job.status)).length;
    if (running) items.push({ key: "jobs", text: `${running} running in background` });
    if (failed) items.push({ key: "failedJobs", text: `${failed} background ${failed === 1 ? "job" : "jobs"} failed`, tone: "attention" });
  }
  return items;
}

export function createSummaryRenderer({ document, root, navigate }) {
  const buttons = new Map();
  return function render(items) {
    const keep = new Set();
    items.forEach((item, index) => {
      let button = buttons.get(item.key);
      if (!button) {
        button = document.createElement("button");
        button.type = "button";
        button.onclick = () => navigate(item.key);
        buttons.set(item.key, button);
      }
      button.className = "summary-item" + (item.tone ? " tone-" + item.tone : "");
      if (button.textContent !== item.text) button.textContent = item.text;
      if (root.children[index] !== button) root.insertBefore(button, root.children[index] || null);
      keep.add(item.key);
    });
    for (const [key, button] of buttons) if (!keep.has(key)) { button.remove(); buttons.delete(key); }
    root.hidden = items.length === 0;
  };
}
