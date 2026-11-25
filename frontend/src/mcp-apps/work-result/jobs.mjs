// Format a bounded, validated Job snapshot without observing or controlling Jobs.
export function jobOutcomeLabel(job) {
  const state = job.state === "active" ? job.status : job.outcome || job.status;
  return ({ running: "运行中", queued: "排队中", completed: "已完成", passed: "通过", failed: "失败", recovering: "重新连接中", lost: "执行丢失", stopped: "已停止", cancelled: "已取消", timed_out: "超时", inconclusive: "无定论", stop_requested: "正在停止" })[state] || state.replaceAll("_", " ");
}

const JOB_STATE = {
  running: "tone-active", queued: "tone-pending", recovering: "tone-active", stop_requested: "tone-active",
  passed: "tone-passed", completed: "tone-passed",
  failed: "tone-failed", lost: "tone-failed", timed_out: "tone-failed", cancelled: "tone-failed",
};

export function renderJobs(document, el, jobs) {
  const root = el("jobsList"); root.replaceChildren();
  const results = el("jobResultsList"); results.replaceChildren();
  el("jobsSection").hidden = !jobs?.available || (!jobs.items.length && !jobs.active);
  el("jobsMeta").textContent = jobs?.truncated ? "最多显示 8 个后台执行；可能还有更多正在运行或已结束。" : "";
  el("jobResults").hidden = true;
  if (!jobs?.available) return;
  const outcomes = new Map();
  for (const job of jobs.items) {
    const row = document.createElement("article"); row.className = "workflow-row";
    const header = document.createElement("header");
    const label = document.createElement("strong"); label.textContent = job.tool.replaceAll("_", " ");
    const status = document.createElement("span");
    const outcome = jobOutcomeLabel(job);
    status.className = "secondary job-state" + (JOB_STATE[job.state === "active" ? job.status : job.outcome || job.status]
      ? " " + JOB_STATE[job.state === "active" ? job.status : job.outcome || job.status] : "");
    status.textContent = outcome;
    if (job.recovery_state) status.textContent += " · " + job.recovery_state.replaceAll("_", " ");
    header.append(label, status); row.append(header);
    if (job.state === "active") root.append(row);
    else {
      results.append(row);
      outcomes.set(outcome, (outcomes.get(outcome) || 0) + 1);
    }
  }
  el("jobResults").hidden = !results.children.length;
  el("jobResultsSummary").textContent = "最近的后台结果 · "
    + [...outcomes].map(([outcome, count]) => outcome + " " + count).join(" · ");
}
