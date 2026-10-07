import {
  choiceMatches, choiceState, booleanChoice, isChoiceControl, normalizeLabel,
  type ResolvedStructureNode,
} from "./form-cache.js";

type ChoiceEntry = ResolvedStructureNode & { mapping: NonNullable<ResolvedStructureNode["mapping"]> };
export type ChoiceTarget = ChoiceEntry & { desired_state: boolean };

// Group ids fence choices in this observation only; they are never persisted as
// readback identity. Without group provenance, a duplicate mapped field is ambiguous.
export function choiceGroupKey(entry: ChoiceEntry): string {
  const { node, mapping } = entry;
  return JSON.stringify([mapping.resumePath, node.group_id ?? "",
    normalizeLabel(node.group_label ?? node.form_context?.group_label ?? ""),
    node.form_context?.section_label ?? ""]);
}

export function planChoiceGroup(
  entry: ChoiceEntry, resolved: readonly ResolvedStructureNode[], desired: string,
): { targets: ChoiceTarget[]; reason?: string } {
  const key = choiceGroupKey(entry);
  const peers = resolved.filter((item): item is ChoiceEntry =>
    item.mapping !== undefined && isChoiceControl(item.node)
    && choiceGroupKey(item as ChoiceEntry) === key);
  const toggle = (item: ChoiceEntry) =>
    ["checkbox", "switch", "menuitemcheckbox"].includes(item.node.role.toLowerCase())
    || item.node.form_context?.input_type?.toLowerCase() === "checkbox";
  const option = (item: ChoiceEntry) => item.mapping.choiceValue ?? item.node.name;
  let targets: ChoiceTarget[];
  if (peers.length === 1 && toggle(entry)) {
    const desiredBoolean = booleanChoice(desired);
    const offeredBoolean = booleanChoice(option(entry));
    const desired_state = desiredBoolean !== undefined
      ? offeredBoolean !== undefined ? desiredBoolean === offeredBoolean
        : entry.mapping.choiceValue === undefined ? desiredBoolean : undefined
      : choiceMatches(option(entry), desired) ? true : undefined;
    if (desired_state === undefined) return { targets: [], reason: "Checkbox/switch requires an explicit boolean state or a uniquely mapped choice value." };
    targets = [{ ...entry, desired_state }];
  } else {
    const matching = peers.filter(item => choiceMatches(option(item), desired)
      || (!item.mapping.choiceValue && choiceMatches(item.node.value ?? "", desired)));
    if (matching.length !== 1) return { targets: [], reason: matching.length
      ? "Multiple choices match the desired value in this group; teach the exact data-bearing option."
      : "No unique choice matches the profile value in this group; inspect or teach its choice_value." };
    const target = matching[0]!;
    // A scalar profile value identifies one option. A checkbox group with another
    // selected option is not silently treated as a single-choice radio group.
    if (toggle(target) && peers.some(item => item !== target && choiceState(item.node) === true)) {
      return { targets: [], reason: "Another checkbox choice is selected; the intended set of choices needs an explicit mapping." };
    }
    targets = [{ ...target, desired_state: true }];
  }
  if (targets.some(item => choiceState(item.node) === undefined)) return {
    targets: [], reason: "Choice state is missing, mixed, or contradictory; obtain checked/selected readback before toggling.",
  };
  return { targets };
}
