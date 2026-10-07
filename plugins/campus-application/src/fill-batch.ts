import { randomUUID } from "node:crypto";
import { choiceState, isChoiceControl, type SnapshotNode } from "./form-cache.js";
import { uploadFileName, type UploadLocation } from "./upload-source.js";
import { canonicalDate, choiceReadback, dateForControl, validChoicePath } from "./widget-values.js";

export type FillAction = { mapping_id?: string; kind: string; label: string; element_id: string; value: string; confidence: number;
  desired_state?: boolean; upload?: UploadLocation; choice_path?: string[] };
export type FillScope = { client_id: string; browser_id: string; page_id: string; snapshot_generation: number; url: string };
export type Operation = { action: "input_text"; element_id: string; text: string }
  | { action: "select_option"; element_id: string; option: string }
  | { action: "set_value"; element_id: string; value: string }
  | { action: "click"; element_id: string }
  | { action: "upload_file"; element_id: string; project: string; path: string }
  | { action: "select_choice"; element_id: string; choice_path: string[] }
  | { action: "set_date"; element_id: string; value: string };
export type Attention = { mapping_id?: string; label: string; status: "mismatch" | "unresolved"; reason: string };
export type Batch = { action: "batch"; client_id: string; browser_id: string; page_id: string; operations: Operation[] };
type Field = { identity: string; action: FillAction; replayBlocked?: boolean };
type Plan = { scope: FillScope; fields: Field[]; issued: Field[]; expires: number; widgetBatchLimit: number };
export type Receipt = {
  execution_state?: string | undefined; stability?: { stable: boolean } | undefined;
  requested_count?: number | undefined; completed_count?: number | undefined;
  remaining_count?: number | undefined; stopped_at_index?: number | undefined;
  stopped_execution_state?: string | undefined;
};
const plans = new Map<string, Plan>();
const TTL = 15 * 60_000;
const MAX_PLANS = 32;

// Browser signatures carry native form provenance; section/group disambiguate
// repeated entries. Without provenance only a unique semantic identity is usable.
function identity(node: SnapshotNode): string {
  return JSON.stringify([node.form_context?.field_signature ?? "", node.role,
    node.form_context?.field_signature && !isChoiceControl(node) ? "" : node.name,
    node.group_label ?? "", node.form_context?.section_label ?? "",
    node.form_context?.group_label ?? "", node.form_context?.group_index ?? null]);
}
function indexNodes(nodes: readonly SnapshotNode[]): Map<string, SnapshotNode[]> {
  const index = new Map<string, SnapshotNode[]>();
  for (const node of nodes) {
    const key = identity(node);
    index.set(key, [...(index.get(key) ?? []), node]);
  }
  return index;
}
function isWidget(action: FillAction): boolean { return action.kind === "select_choice" || action.kind === "set_date"; }
function hasChoiceCommitEvidence(node: SnapshotNode, completedChoice = false): boolean {
  const editableInput = ["input", "textarea"].includes(node.form_context?.dom_tag.toLowerCase() ?? "")
    && node.read_only !== true;
  // Equal editable text may only be a search query before the typed operation.
  return !editableInput || node.selected === true || completedChoice;
}
export function fieldSatisfied(action: FillAction, node: SnapshotNode, completedChoice = false): boolean {
  if (node.form_context?.aria_invalid) return false;
  if (action.kind === "set_date") return node.value !== undefined && dateForControl(node.value, node) === action.value;
  if (action.kind === "select_choice" && action.choice_path) {
    if (!hasChoiceCommitEvidence(node, completedChoice)) return false;
    return choiceReadback(node, action.choice_path) === "confirmed";
  }
  if (action.kind === "click") return typeof action.desired_state === "boolean"
    && choiceState(node) === action.desired_state;
  if (action.kind === "upload_file") {
    const expected = uploadFileName(action.value);
    return expected.length > 0 && (node.value !== undefined && uploadFileName(node.value) === expected
      || node.value === undefined && node.name === expected);
  }
  return node.value !== undefined && node.value === action.value;
}
function knownReadback(action: FillAction, node?: SnapshotNode): boolean {
  if (!node) return false;
  if (action.kind === "set_date") return node.value !== undefined && canonicalDate(node.value) !== undefined;
  if (action.kind === "select_choice" && action.choice_path) return hasChoiceCommitEvidence(node)
    && choiceReadback(node, action.choice_path) !== "unresolved";
  return action.kind === "click" ? choiceState(node) !== undefined : node.value !== undefined;
}
function operation(action: FillAction, node: SnapshotNode): Operation | undefined {
  if (!node.actionable || node.disabled || (node.read_only && !isWidget(action)) || !node.element_id
    || !node.actions?.includes(action.kind) || action.confidence < 0.9
    || !action.value || action.value.includes("\0")
    || action.kind !== "select_choice" && Buffer.byteLength(action.value) > 4096) return;
  const element_id = node.element_id;
  if (action.kind === "set_date" && canonicalDate(action.value) === action.value) return {
    action: "set_date", element_id, value: action.value,
  };
  if (action.kind === "select_choice" && validChoicePath(action.choice_path)) return {
    action: "select_choice", element_id, choice_path: [...action.choice_path],
  };
  if (action.kind === "click" && isChoiceControl(node)
    && typeof action.desired_state === "boolean" && choiceState(node) !== undefined
    && choiceState(node) !== action.desired_state
    && (action.desired_state || ["checkbox", "switch", "menuitemcheckbox"].includes(node.role.toLowerCase())
      || node.form_context?.input_type?.toLowerCase() === "checkbox")) return { action: "click", element_id };
  if (action.kind === "upload_file" && action.upload) return {
    action: "upload_file", element_id, ...action.upload,
  };
  if (action.kind === "input_text" && node.value === ""
    && ["input", "textarea"].includes(node.form_context?.dom_tag.toLowerCase() ?? "")) return { action: "input_text", element_id, text: action.value };
  if (action.kind === "select_option" && node.form_context?.dom_tag.toLowerCase() === "select") {
    return { action: "select_option", element_id, option: action.value };
  }
  if (action.kind === "set_value" && ["input", "textarea"].includes(node.form_context?.dom_tag.toLowerCase() ?? "")) {
    return { action: "set_value", element_id, value: action.value };
  }
}
function prune(): void {
  for (const [key, plan] of plans) if (plan.expires <= Date.now()) plans.delete(key);
  while (plans.size >= MAX_PLANS) plans.delete(plans.keys().next().value!);
}
function issue(plan: Plan, nodes: readonly SnapshotNode[]): Batch | undefined {
  const indexed = indexNodes(nodes);
  const candidates = plan.fields.flatMap(field => {
    const matches = indexed.get(field.identity);
    const node = matches?.length === 1 ? matches[0] : undefined;
    const op = !field.replayBlocked && node && operation(field.action, node);
    return op ? [{ field, op }] : [];
  });
  // Native operations share the ordinary 32-field batch. Custom widgets can
  // rerender siblings: batch them separately and only widen the default one-op
  // boundary when the caller has verified sibling authority on this form.
  const native = candidates.filter(item => !isWidget(item.field.action));
  const chosen = native.length ? native.slice(0, 32) : candidates.slice(0, plan.widgetBatchLimit);
  plan.issued = chosen.map(item => item.field);
  const operations = chosen.map(item => item.op);
  if (!operations.length) return;
  return { action: "batch", client_id: plan.scope.client_id, browser_id: plan.scope.browser_id,
    page_id: plan.scope.page_id, operations };
}
export function createFillBatch(scope: FillScope, nodes: readonly SnapshotNode[], actions: readonly FillAction[], blockers: readonly Attention[] = [], widgetBatchLimit = 1) {
  if (!Number.isSafeInteger(scope.snapshot_generation) || scope.snapshot_generation < 1
    || nodes.length > 256 || actions.length > 256
    || !Number.isInteger(widgetBatchLimit) || widgetBatchLimit < 1 || widgetBatchLimit > 8) throw new Error("Invalid bounded snapshot or widget batch limit");
  prune();
  const indexed = indexNodes(nodes);
  const fields: Field[] = [];
  const needs_attention: Attention[] = [...blockers];
  const seen = new Set<string>();
  for (const action of actions) {
    const matches = nodes.filter(node => node.element_id === action.element_id);
    const node = matches.length === 1 ? matches[0] : undefined;
    const key = node && identity(node);
    if (node && fieldSatisfied(action, node)) continue;
    if (!node || !key || indexed.get(key)?.length !== 1 || seen.has(key) || !operation(action, node)) {
      needs_attention.push({ ...(action.mapping_id ? { mapping_id: action.mapping_id } : {}), label: action.label, status: "unresolved", reason: "Requires a supported, uniquely identified admitted control, explicit choice state, or upload_source project/path." });
      continue;
    }
    seen.add(key);
    fields.push({ identity: key, action });
  }
  const plan: Plan = { scope, fields, issued: [], expires: Date.now() + TTL, widgetBatchLimit };
  const batch = issue(plan, nodes);
  if (!batch) return { needs_attention };
  const plan_id = randomUUID();
  plans.set(plan_id, plan);
  return { plan_id, batch, ...(fields.length > plan.issued.length ? { deferred_count: fields.length - plan.issued.length } : {}), needs_attention };
}

export function reconcileFill(plan_id: string, scope: FillScope, nodes: readonly SnapshotNode[], receipt: Receipt) {
  const plan = plans.get(plan_id);
  if (!plan || plan.expires <= Date.now()) throw new Error("Fill plan expired; observe current state and plan anew, never replay the old batch");
  if (nodes.length > 256 || scope.client_id !== plan.scope.client_id || scope.browser_id !== plan.scope.browser_id
    || scope.page_id !== plan.scope.page_id || scope.url !== plan.scope.url
    || !Number.isSafeInteger(scope.snapshot_generation) || scope.snapshot_generation <= plan.scope.snapshot_generation) {
    throw new Error("Fresh same-target readback required; old element ids must not be reused");
  }
  // Consume once. Repeated reconciliation cannot produce the same retry effect.
  plans.delete(plan_id);
  const indexed = indexNodes(nodes);
  const count = plan.issued.length;
  const complete = receipt.execution_state === "completed" && receipt.stability?.stable === true
    && receipt.requested_count === count && receipt.completed_count === count
    && receipt.remaining_count === 0 && receipt.stopped_at_index === undefined
    && receipt.stopped_execution_state === undefined;
  const knownCounts = receipt.requested_count === count
    && Number.isInteger(receipt.completed_count) && receipt.completed_count! >= 0 && receipt.completed_count! <= count
    && Number.isInteger(receipt.remaining_count) && receipt.remaining_count! >= 0
    && receipt.completed_count! + receipt.remaining_count! <= count;
  let confirmed = 0;
  const needs_attention: Attention[] = [];
  const pending: Field[] = [];
  const unresolved: Field[] = [];
  for (const field of plan.fields) {
    const matches = indexed.get(field.identity);
    const node = matches?.length === 1 ? matches[0] : undefined;
    const issuedIndex = plan.issued.indexOf(field);
    if (node && fieldSatisfied(field.action, node, complete && issuedIndex >= 0)) {
      confirmed++;
      continue;
    }
    const clipped = field.action.kind !== "click" && field.action.kind !== "upload_file"
      && node?.value !== undefined && Buffer.byteLength(field.action.value) > 512
      && field.action.value.startsWith(node.value);
    const status = !knownReadback(field.action, node) || clipped ? "unresolved" : "mismatch";
    const requiresSemanticReadback = isWidget(field.action) || field.action.kind === "select_option";
    // Never infer execution from readback. Interrupted/missing receipts retain all
    // mismatches as attention, including definitely unstarted fields. A new plan
    // may use fresh observations after the caller resolves the stopped boundary.
    if (!complete) {
      const progress = !knownCounts ? "Execution progress unknown"
        : issuedIndex >= 0 && issuedIndex < receipt.completed_count! ? "Effect completed"
        : issuedIndex < 0 || issuedIndex >= count - receipt.remaining_count! ? "Definitely unstarted"
        : "Effect uncertain";
      needs_attention.push({ ...(field.action.mapping_id ? { mapping_id: field.action.mapping_id } : {}), label: field.action.label, status, reason: `${progress}; inspect fresh state before a new plan; do not replay the batch.` });
    } else if (!field.replayBlocked && issuedIndex < 0 && node && !node.form_context?.aria_invalid && operation(field.action, node)) {
      pending.push(field);
    } else if (field.replayBlocked || !node || requiresSemanticReadback || !knownReadback(field.action, node) || clipped || node.form_context?.aria_invalid || !operation(field.action, node)) {
      // Native selects may accept option.value while AX returns the selected label.
      // Neither that difference nor a widget's incomplete path proves a retry is needed.
      if (requiresSemanticReadback && issuedIndex >= 0) field.replayBlocked = true;
      unresolved.push(field);
      needs_attention.push({ ...(field.action.mapping_id ? { mapping_id: field.action.mapping_id } : {}), label: field.action.label, status: requiresSemanticReadback ? "unresolved" : status, reason: clipped ? "Readback value may be clipped; do not repeat the effect."
          : requiresSemanticReadback ? "Completed selection/date requires matching semantic value/path readback; inspect this field without repeating the effect."
          : node?.form_context?.aria_invalid ? "Page reports validation failure; resolve this field before another effect."
          : "No unique fresh admitted control/value; inspect only this field." });
    } else {
      pending.push(field);
      // Deferred fields are not failed attempts; only report actual mismatches.
      if (issuedIndex >= 0) needs_attention.push({ ...(field.action.mapping_id ? { mapping_id: field.action.mapping_id } : {}), label: field.action.label, status, reason: "Fresh value differs; retry uses fresh element authority." });
    }
  }
  if (!pending.length) return { confirmed, needs_attention };
  const next: Plan = { scope, fields: [...pending], issued: [], expires: Date.now() + TTL, widgetBatchLimit: plan.widgetBatchLimit };
  const batch = issue(next, nodes);
  next.fields.push(...unresolved);
  if (!batch) return { confirmed, needs_attention };
  prune();
  const next_id = randomUUID();
  plans.set(next_id, next);
  return { confirmed, needs_attention, plan_id: next_id, batch, ...(pending.length > next.issued.length ? { deferred_count: pending.length - next.issued.length } : {}) };
}
