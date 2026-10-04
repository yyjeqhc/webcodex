import { renderValidation, renderReview } from "./checks.mjs";
import { renderJobs } from "./jobs.mjs";
import { renderWindowCurrentActivity, renderActivityDetail } from "./activity.mjs";
// Async owners are separate exports; synchronous renderers below still get no RPC.
export { createReadCache } from "./read-cache.mjs";
export { createActivityDetails } from "./activity-details.mjs";


// Static bundled composition, not a third-party UI sandbox. The mounted App
// validates snapshots and owns async loading; renderers receive only display
// dependencies and already-validated values, never send/refresh/identity state.
export function createWorkResultSections(environment) {
  const { document, el, safeCount } = environment;
  return Object.freeze({
    renderValidation: value => renderValidation(el, safeCount, value),
    renderReview: value => renderReview(el, value),
    renderJobs: value => renderJobs(document, el, value),
    renderWindowCurrentActivity: value => renderWindowCurrentActivity(environment, value),
    renderActivityDetail: (root, value) => renderActivityDetail(environment, root, value),
  });
}
