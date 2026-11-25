// Read-only display of already-validated Session evidence. No status inference
// from workspace cleanliness, latest historical checks, or UI visibility.
export function validationLabel(validation) {
  if (validation.unresolved_failures > 0 || validation.current_status === "failed") return "验证需关注";
  if (validation.current_status === "passed") return "验证通过";
  if (validation.current_status === "stale") return "验证已过期";
  if (validation.current_status === "not_run" || validation.current_status === "unproven") return "未运行验证";
  if (validation.current_status === "expected") return "已记录预期结果";
  if (validation.current_status === "inconclusive") return "验证无定论";
  return "验证状态不可用";
}

const TONE = {
  "验证通过": "tone-good", "验证需关注": "tone-attention", "验证状态不可用": "tone-unavailable",
};

export function renderValidation(el, safeCount, validation) {
  const label = validationLabel(validation);
  el("validationStatus").textContent = label;
  el("validationStatus").className = "primary" + (TONE[label] ? " " + TONE[label] : "");
  const pieces = [];
  if (safeCount(validation.successes) && validation.successes > 0) pieces.push(`通过 ${validation.successes}`);
  if (validation.unresolved_failures > 0) pieces.push(`未解决 ${validation.unresolved_failures}`);
  if (validation.history_partial) pieces.push("历史不完整");
  el("validationMeta").textContent = pieces.join(" · ");
}

export function renderReview(el, review) {
  if (!review.available) {
    el("reviewStatus").textContent = "审查状态不可用";
    el("reviewMeta").textContent = "";
  } else if (review.total > 0) {
    el("reviewStatus").textContent = "已记录审查";
    el("reviewMeta").textContent = review.history_partial ? "有最近的审查活动" : "";
  } else if (review.history_partial) {
    el("reviewStatus").textContent = "审查状态不完整";
    el("reviewMeta").textContent = "更早的活动不在保留的历史范围内";
  } else {
    el("reviewStatus").textContent = "未记录审查";
    el("reviewMeta").textContent = "";
  }
}
