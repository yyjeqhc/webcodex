---
description: Diagnose and fix a WebPi regression by proving root cause and regression coverage.
---
Fix this WebPi bug: $@

1. Observe the real runtime/project/Git state and reproduce or establish the failing condition.
2. Trace the concrete root cause through source/config/runtime evidence; do not patch only the visible symptom.
3. Add a regression test that fails against unfixed behavior when practical.
4. Apply the narrowest fix that preserves auth, path, scope, compatibility, and recovery boundaries.
5. Prove targeted green, then run the relevant broader regression.
6. Review the final diff and verify the user-facing failure/recovery message if one changed.

If any mutation returns outcome_unknown or connection is lost mid-effect, inspect state before any retry.
Do not commit/deploy/push unless explicitly authorized.

Finish with: Root cause / Changed / Validated / Remaining risk / User action.
