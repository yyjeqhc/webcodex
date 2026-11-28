export function renderCompact({ el, formatDuration, formatElapsed = formatDuration, formatAge }, snapshot) {
  const dot = el("compactDot"), status = el("compactStatus"), meta = el("compactMeta");
  const bar = el("activityBar");
  const setBar = live => { if (bar) bar.hidden = !live; };
  if (!snapshot) {
    dot.className = "compact-dot";
    status.textContent = "等待活动…";
    meta.textContent = "";
    setBar(false);
    return;
  }
  const activity = snapshot.activity ?? {};
  const jobs = snapshot.jobs ?? {};
  const activeJobs = (jobs.items ?? []).filter(job => job.state === "active").length;
  const failedJobs = (jobs.items ?? []).filter(job => job.state === "terminal"
    && ["failed", "lost", "timed_out"].includes(job.outcome || job.status)).length;
  const live = !!(activity.active && activity.current) || activeJobs > 0;
  dot.className = "compact-dot" + (failedJobs ? " is-failed" : live ? " is-live" : "");
  setBar(live && !failedJobs);

  const lastAt = activity.last_activity_at_ms ?? activity.last_meaningful_activity_at_ms;
  const lastLabel = activity.last?.label;
  if (activity.active && activity.current) {
    status.textContent = activity.current.label;
    meta.textContent = activity.current.started_at_ms == null
      ? "" : formatElapsed(Math.max(0, Date.now() - activity.current.started_at_ms));
    return;
  }
  if (activeJobs) {
    status.textContent = `后台运行 ${activeJobs} 项`;
    meta.textContent = lastLabel ? "最近：" + lastLabel : "";
    return;
  }
  status.textContent = lastLabel ? "空闲" : "等待活动…";
  meta.textContent = lastLabel
    ? "最近：" + lastLabel + (lastAt == null ? "" : " · " + formatAge(lastAt))
    : "";
}
