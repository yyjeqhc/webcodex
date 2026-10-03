// Format a bounded, validated Job snapshot without observing or controlling Jobs.
export function jobOutcomeLabel(job) {
  const state = job.state === "active" ? job.status : job.outcome || job.status;
  return ({ running: "Running", queued: "Queued", completed: "Completed", passed: "Passed", failed: "Failed", recovering: "Reconnecting", lost: "Execution lost", stopped: "Stopped", cancelled: "Cancelled", timed_out: "Timed out", inconclusive: "Inconclusive", stop_requested: "Stopping" })[state] || state.replaceAll("_", " ");
}

export function renderJobs(document, el, jobs) {
  const root = el("jobsList"); root.replaceChildren();
  const results = el("jobResultsList"); results.replaceChildren();
  el("jobsSection").hidden = !jobs?.available || (!jobs.items.length && !jobs.active);
  el("jobsMeta").textContent = jobs?.truncated ? "Showing up to 8 background executions; more may be active or finished." : "";
  el("jobResults").hidden = true;
  if (!jobs?.available) return;
  const outcomes = new Map();
  for (const job of jobs.items) {
    const row = document.createElement("article"); row.className = "workflow-row";
    const header = document.createElement("header");
    const label = document.createElement("strong"); label.textContent = job.tool.replaceAll("_", " ");
    const status = document.createElement("span"); status.className = "secondary";
    const outcome = jobOutcomeLabel(job);
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
  el("jobResultsSummary").textContent = "Recent background results · "
    + [...outcomes].map(([outcome, count]) => count + " " + outcome.toLowerCase()).join(" · ");
}
