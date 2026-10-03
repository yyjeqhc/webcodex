// Read-only display of already-validated Session evidence. No status inference
// from workspace cleanliness, latest historical checks, or UI visibility.
export function validationLabel(validation) {
  if (validation.unresolved_failures > 0 || validation.current_status === "failed") return "Checks need attention";
  if (validation.current_status === "passed") return "Checks passed";
  if (validation.current_status === "stale") return "Checks are out of date";
  if (validation.current_status === "not_run" || validation.current_status === "unproven") return "Checks not run";
  if (validation.current_status === "expected") return "Expected result recorded";
  if (validation.current_status === "inconclusive") return "Checks inconclusive";
  return "Check status unavailable";
}

export function renderValidation(el, safeCount, validation) {
  el("validationStatus").textContent = validationLabel(validation);
  const pieces = [];
  if (safeCount(validation.successes) && validation.successes > 0) pieces.push(`${validation.successes} passed`);
  if (validation.unresolved_failures > 0) pieces.push(`${validation.unresolved_failures} unresolved`);
  if (validation.history_partial) pieces.push("limited history");
  el("validationMeta").textContent = pieces.join(" · ");
}

export function renderReview(el, review) {
  if (!review.available) {
    el("reviewStatus").textContent = "Review status unavailable";
    el("reviewMeta").textContent = "";
  } else if (review.total > 0) {
    el("reviewStatus").textContent = "Review recorded";
    el("reviewMeta").textContent = review.history_partial ? "Recent review activity available" : "";
  } else if (review.history_partial) {
    el("reviewStatus").textContent = "Review status incomplete";
    el("reviewMeta").textContent = "Earlier activity is outside the retained history";
  } else {
    el("reviewStatus").textContent = "Review not recorded";
    el("reviewMeta").textContent = "";
  }
}
