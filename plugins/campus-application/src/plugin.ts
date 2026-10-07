import { createFillBatch, reconcileFill, type Attention } from "./fill-batch.js";
import {
  definePlugin,
  errorResult,
  defineTool,
  runPlugin,
  schema,
  textResult,
} from "@yyjeqhc/webcodex-plugin-sdk";
import {
  choiceMatches,
  formStructureSignature,
  isResumeUpload,
  matchField,
  normalizeLabel,
  resolveFormMappings,
  type FormMappingHint,
  type SnapshotFormContext,
  type SnapshotNode,
} from "./form-cache.js";
import {
  loadPersistentMappingHints,
  mappingSiteIdentity,
  rememberPersistentMappingHints,
} from "./mapping-memory.js";
import {
  canonicalFields,
  canonicalProfileFromResume,
  canonicalProfileSchema,
  loadResumeProfile,
  resolveResumeValue,
  resumeProfileSchema,
  type CanonicalField,
  type ResumeProfile,
} from "./resume.js";
import {
  planNextSectionExpansion,
  type SectionExpansionAction,
} from "./section-lifecycle.js";
import {
  planApplicationFlow,
  type FlowAction,
} from "./application-flow.js";

const formContextSchema = schema.object({
  field_signature: schema.string({ maxLength: 24 }),
  dom_tag: schema.string({ maxLength: 32 }),
  input_type: schema.optional(schema.string({ maxLength: 32 })),
  html_name: schema.optional(schema.string({ maxLength: 256 })),
  placeholder: schema.optional(schema.string({ maxLength: 512 })),
  autocomplete: schema.optional(schema.string({ maxLength: 128 })),
  nearby_label: schema.optional(schema.string({ maxLength: 512 })),
  group_label: schema.optional(schema.string({ maxLength: 512 })),
  group_index: schema.optional(schema.integer()),
  group_size: schema.optional(schema.integer()),
  section_label: schema.optional(schema.string({ maxLength: 512 })),
  component_hint: schema.optional(schema.string({ maxLength: 64 })),
  aria_invalid: schema.optional(schema.boolean()),
  validation_hint: schema.optional(schema.string({ maxLength: 512 })),
  option_count: schema.optional(schema.integer()),
});

const nodeSchema = schema.object({
  role: schema.string({ maxLength: 80 }),
  name: schema.optional(schema.string({ maxLength: 1200 })),
  description: schema.optional(schema.string({ maxLength: 1200 })),
  value: schema.optional(schema.string({ maxLength: 4096 })),
  group_id: schema.optional(schema.string({ maxLength: 160 })),
  group_role: schema.optional(schema.string({ maxLength: 80 })),
  group_label: schema.optional(schema.string({ maxLength: 1200 })),
  checked: schema.optional(schema.string({ maxLength: 32 })),
  selected: schema.optional(schema.boolean()),
  required: schema.optional(schema.boolean()),
  disabled: schema.optional(schema.boolean()),
  read_only: schema.optional(schema.boolean()),
  form_context: schema.optional(formContextSchema),
  element_id: schema.optional(schema.string({ maxLength: 160 })),
  actions: schema.optional(schema.array(schema.string({ maxLength: 80 }), { maxItems: 16 })),
  actionable: schema.boolean(),
});

const mappingHintSchema = schema.object({
  mapping_id: schema.string({ maxLength: 24 }),
  label: schema.optional(schema.string({ maxLength: 1200 })),
  canonical_field: schema.optional(schema.string({ enum: canonicalFields })),
  resume_path: schema.optional(schema.string({ maxLength: 300 })),
  choice_value: schema.optional(schema.string({ maxLength: 500 })),
});

const recognizedSchema = schema.object({
  mapping_id: schema.string({ maxLength: 24 }),
  canonical_field: schema.string({ enum: canonicalFields }),
  resume_path: schema.string({ maxLength: 300 }),
  label: schema.string({ maxLength: 1200 }),
  role: schema.string({ maxLength: 80 }),
  element_id: schema.string({ maxLength: 160 }),
  current_value: schema.string({ maxLength: 4096 }),
  proposed_value: schema.string({ maxLength: 4096 }),
  confidence: schema.number(),
  support: schema.string({
    enum: ["input_text", "select_option", "set_value", "upload_file", "click", "manual_review"] as const,
  }),
});

const blockerSchema = schema.object({
  mapping_id: schema.string({ maxLength: 24 }),
  label: schema.string({ maxLength: 1200 }),
  role: schema.string({ maxLength: 80 }),
  element_id: schema.string({ maxLength: 160 }),
  reason: schema.string({ maxLength: 500 }),
});

const missingProfileFieldSchema = schema.object({
  canonical_field: schema.string({ enum: canonicalFields }),
  resume_path: schema.string({ maxLength: 300 }),
  occurrences: schema.integer(),
  mapping_ids: schema.array(schema.string({ maxLength: 24 }), { maxItems: 16 }),
});

const unmappedCandidateSchema = schema.object({
  mapping_id: schema.string({ maxLength: 24 }),
  label: schema.string({ maxLength: 1200 }),
  role: schema.string({ maxLength: 80 }),
  element_id: schema.string({ maxLength: 160 }),
  actions: schema.array(schema.string({ maxLength: 80 }), { maxItems: 16 }),
  form_context: schema.optional(formContextSchema),
});

const sectionActionSchema = schema.object({
  kind: schema.string({ enum: ["click"] as const }),
  collection: schema.string({ enum: ["education", "experience", "projects"] as const }),
  resume_path: schema.string({ maxLength: 300 }),
  label: schema.string({ maxLength: 1200 }),
  element_id: schema.string({ maxLength: 160 }),
  current_count: schema.integer(),
  target_count: schema.integer(),
  reason: schema.string({ maxLength: 500 }),
});

const flowActionSchema = schema.object({
  kind: schema.string({ enum: ["click"] as const }),
  intent: schema.string({ enum: ["advance_step"] as const }),
  label: schema.string({ maxLength: 1200 }),
  element_id: schema.string({ maxLength: 160 }),
  reason: schema.string({ maxLength: 500 }),
});

type FillSupport =
  | "input_text"
  | "select_option"
  | "set_value"
  | "upload_file"
  | "click"
  | "manual_review";

function supportFor(node: SnapshotNode): FillSupport {
  const role = node.role.toLowerCase();
  if (node.actions?.includes("upload_file") === true || isResumeUpload(node)) {
    return "upload_file";
  }
  if (node.actions?.includes("set_value")) return "set_value";
  if (role === "combobox" || role === "listbox") return "select_option";
  if (role === "datetime" || role === "date" || role === "time") return "set_value";
  if (role === "textbox" || role === "searchbox" || role === "spinbutton") return "input_text";
  return "manual_review";
}

function adaptValue(
  siteKind: "greenhouse-like" | "lever-like" | "campus-cn-like" | "generic",
  field: CanonicalField,
  value: string,
): string {
  if (siteKind === "campus-cn-like" && field === "degree") {
    const degreeMap: Record<string, string> = {
      Bachelor: "本科",
      Master: "硕士",
      PhD: "博士",
    };
    return degreeMap[value] ?? value;
  }
  return value;
}

function classifySite(title: string, url: string, nodes: readonly SnapshotNode[]) {
  const raw = [
    title,
    url,
    ...nodes.flatMap((node) => [node.name, node.group_label ?? ""]),
  ].join(" ");
  const haystack = normalizeLabel(raw);
  const mappedLanguageEvidence = [
    title,
    ...nodes
      .flatMap((node) => [node.name, node.group_label ?? ""])
      .filter((label) => matchField(label)),
  ].join(" ");
  if (/[㐀-鿿]/u.test(mappedLanguageEvidence)) return "campus-cn-like" as const;
  if (haystack.includes("firstname") && haystack.includes("lastname") && (haystack.includes("school") || haystack.includes("degree"))) {
    return "greenhouse-like" as const;
  }
  if ((haystack.includes("fullname") || haystack.includes("name")) && (haystack.includes("linkedin") || haystack.includes("currentcompany"))) {
    return "lever-like" as const;
  }
  return "generic" as const;
}

function analyzeNodes(
  nodes: readonly SnapshotNode[],
  resume: ResumeProfile,
  url: string,
  hints: readonly FormMappingHint[] = [],
) {
  const recognized: Array<{
    mapping_id: string;
    canonical_field: CanonicalField;
    resume_path: string;
    label: string;
    role: string;
    element_id: string;
    current_value: string;
    proposed_value: string;
    confidence: number;
    support: FillSupport;
  }> = [];
  const blockers: Array<{
    mapping_id: string;
    label: string;
    role: string;
    element_id: string;
    reason: string;
  }> = [];
  const missingProfileFields = new Map<string, {
    canonical_field: CanonicalField;
    resume_path: string;
    occurrences: number;
    mapping_ids: string[];
  }>();
  const unmappedCandidates: Array<{
    mapping_id: string;
    label: string;
    role: string;
    element_id: string;
    actions: string[];
    form_context?: SnapshotFormContext;
  }> = [];
  const structureSignature = formStructureSignature(nodes);
  const persistent = loadPersistentMappingHints(url, structureSignature);
  const mergedHints = new Map<string, FormMappingHint>();
  for (const hint of persistent.hints) mergedHints.set(hint.mapping_id, hint);
  for (const hint of hints) mergedHints.set(hint.mapping_id, hint);

  const resolved = resolveFormMappings(
    nodes,
    [...mergedHints.values()],
    mappingSiteIdentity(url),
  );

  let mappingMemoryEntries = persistent.entries;
  if (hints.length > 0) {
    const hintedIds = new Set(hints.map((hint) => hint.mapping_id));
    const persistable = resolved.nodes
      .filter(({ mapping_id, mapping }) => hintedIds.has(mapping_id) && mapping !== undefined)
      .map(({ mapping_id, mapping }) => ({
        mapping_id,
        canonicalField: mapping!.canonicalField,
        resumePath: mapping!.resumePath,
        label: hints.find((hint) => hint.mapping_id === mapping_id)?.label,
        choiceValue: mapping!.choiceValue,
      }));
    mappingMemoryEntries = rememberPersistentMappingHints(
      url,
      resolved.signature,
      persistable,
    );
  }

  for (const { node, mapping, mapping_id } of resolved.nodes) {
    const role = node.role.toLowerCase();
    const label =
      node.group_label ||
      node.name ||
      node.form_context?.placeholder ||
      node.form_context?.html_name ||
      node.form_context?.section_label ||
      node.description ||
      "";
    const resolvedValue = mapping
      ? resolveResumeValue(resume, mapping.resumePath)
      : { found: false, value: "" };

    if (mapping && (!resolvedValue.found || resolvedValue.value.trim().length === 0)) {
      const existing = missingProfileFields.get(mapping.resumePath);
      if (existing) {
        existing.occurrences += 1;
        if (existing.mapping_ids.length < 16 && !existing.mapping_ids.includes(mapping_id)) {
          existing.mapping_ids.push(mapping_id);
        }
      } else {
        missingProfileFields.set(mapping.resumePath, {
          canonical_field: mapping.canonicalField,
          resume_path: mapping.resumePath,
          occurrences: 1,
          mapping_ids: [mapping_id],
        });
      }
      blockers.push({
        mapping_id,
        label,
        role: node.role,
        element_id: node.element_id ?? "",
        reason: resolvedValue.found
          ? `Mapped resume path ${mapping.resumePath} is empty in the structured resume resource.`
          : `Mapped resume path ${mapping.resumePath} is unavailable in the structured resume resource.`,
      });
      continue;
    }

    if (
      (
        mapping?.source === "group" ||
        mapping?.source === "form_context" ||
        mapping?.source === "hint"
      ) &&
      (role === "radio" || role === "checkbox")
    ) {
      const desired = resolvedValue.value;
      const observedChoice = mapping.choiceValue ?? node.name;
      if (choiceMatches(observedChoice, desired)) {
        const checked = node.checked === "true" || node.selected === true;
        recognized.push({
          mapping_id,
          canonical_field: mapping.canonicalField,
          resume_path: mapping.resumePath,
          label: node.group_label || label || observedChoice,
          role: node.role,
          element_id: node.element_id ?? "",
          current_value: checked ? desired : "",
          proposed_value: desired,
          confidence: Math.min(1, mapping.confidence + 0.01),
          support: "click",
        });
      }
      continue;
    }

    if (mapping?.source === "resume_upload") {
      recognized.push({
        mapping_id,
        canonical_field: mapping.canonicalField,
        resume_path: mapping.resumePath,
        label,
        role: node.role,
        element_id: node.element_id ?? "",
        current_value: node.value ?? "",
        proposed_value: resolvedValue.value,
        confidence: mapping.confidence,
        support: "upload_file",
      });
      continue;
    }

    if (!node.actionable) {
      if (mapping && ["datetime", "date", "time"].includes(role)) {
        recognized.push({
          mapping_id,
          canonical_field: mapping.canonicalField,
          resume_path: mapping.resumePath,
          label,
          role: node.role,
          element_id: "",
          current_value: node.value ?? "",
          proposed_value: resolvedValue.value,
          confidence: mapping.confidence,
          support: "manual_review",
        });
        blockers.push({
          mapping_id,
          label,
          role: node.role,
          element_id: "",
          reason: "The resume field is visible in the semantic snapshot but has no actionable Browser element identity.",
        });
      }
      continue;
    }

    if (!mapping) {
      const unmappedUpload = node.actions?.includes("upload_file") === true;
      unmappedCandidates.push({
        mapping_id,
        label,
        role: node.role,
        element_id: node.element_id ?? "",
        actions: node.actions ?? [],
        ...(node.form_context === undefined
          ? {}
          : { form_context: node.form_context }),
      });
      if (
        unmappedUpload ||
        !["button", "link", "option", "radio", "checkbox"].includes(role)
      ) {
        blockers.push({
          mapping_id,
          label,
          role: node.role,
          element_id: node.element_id ?? "",
          reason: unmappedUpload
            ? "Upload control is actionable but has no safe semantic mapping. Teach it with a stable mapping hint before uploading."
            : "Actionable control is not mapped to the current resume schema.",
        });
      }
      continue;
    }

    const support = supportFor(node);
    recognized.push({
      mapping_id,
      canonical_field: mapping.canonicalField,
      resume_path: mapping.resumePath,
      label,
      role: node.role,
      element_id: node.element_id ?? "",
      current_value: node.value ?? "",
      proposed_value: resolvedValue.value,
      confidence: mapping.confidence,
      support,
    });
    if (support === "manual_review") {
      blockers.push({
        mapping_id,
        label,
        role: node.role,
        element_id: node.element_id ?? "",
        reason: "The field is recognized, but its control semantics still require human review.",
      });
    }
  }

  return {
    recognized,
    blockers,
    missing_profile_fields: [...missingProfileFields.values()],
    unmapped_candidates: unmappedCandidates,
    structure_signature: resolved.signature,
    mapping_cache_hit: resolved.cacheHit,
    mapping_cache_entries: resolved.cacheEntries,
    mapping_memory_hit: persistent.hit,
    mapping_memory_entries: mappingMemoryEntries,
  };
}
const profileGet = defineTool({
  name: "profile_get",
  description: "Return the configured structured resume resource plus the bounded canonical fill view used by campus-application. Read-only; never controls a browser or submits an application.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    resume: resumeProfileSchema,
    profile: canonicalProfileSchema,
  }),
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  async execute() {
    const resume = loadResumeProfile();
    const profile = canonicalProfileFromResume(resume);
    return textResult("Loaded configured campus-application resume profile and canonical fill view.", {
      resume,
      profile,
    });
  },
});

const analyzeForm = defineTool({
  name: "analyze_form",
  description: "Analyze a bounded Browser semantic snapshot, classify the ATS-like form, reuse or derive a structure-signature mapping, and report mapped controls plus blockers. Optional stable mapping hints can teach unlabeled controls by mapping_id without depending on volatile element ids. It never controls the browser.",
  inputSchema: schema.object({
    title: schema.string({ maxLength: 1000 }),
    url: schema.string({ maxLength: 4000 }),
    nodes: schema.array(nodeSchema, { maxItems: 256 }),
    mapping_hints: schema.optional(schema.array(mappingHintSchema, { maxItems: 256 })),
  }),
  outputSchema: schema.object({
    site_kind: schema.string({ enum: ["greenhouse-like", "lever-like", "campus-cn-like", "generic"] as const }),
    recognized: schema.array(recognizedSchema, { maxItems: 256 }),
    blockers: schema.array(blockerSchema, { maxItems: 256 }),
    missing_profile_fields: schema.array(missingProfileFieldSchema, { maxItems: 128 }),
    unmapped_candidates: schema.array(unmappedCandidateSchema, { maxItems: 256 }),
    structure_signature: schema.string({ maxLength: 64 }),
    mapping_cache_hit: schema.boolean(),
    mapping_cache_entries: schema.integer(),
    mapping_memory_hit: schema.boolean(),
    mapping_memory_entries: schema.integer(),
  }),
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  async execute({ title, url, nodes: rawNodes, mapping_hints }) {
    const nodes = rawNodes.map(node => ({ ...node, name: node.name ?? "" }));
    const resume = loadResumeProfile();
    const hints: FormMappingHint[] = (mapping_hints ?? []).map((hint) => ({
      mapping_id: hint.mapping_id,
      label: hint.label,
      canonicalField: hint.canonical_field,
      resumePath: hint.resume_path,
      choiceValue: hint.choice_value,
    }));
    const analysis = analyzeNodes(nodes, resume, url, hints);
    const site_kind = classifySite(title, url, nodes);
    return textResult(
      `Detected ${site_kind}: ${analysis.recognized.length} mapped controls, ${analysis.blockers.length} blockers, ${analysis.missing_profile_fields.length} missing profile fields, ${analysis.unmapped_candidates.length} teachable unmapped controls; mapping cache ${analysis.mapping_cache_hit ? "hit" : "miss"}, persistent memory ${analysis.mapping_memory_hit ? "hit" : "miss"}.`,
      { site_kind, ...analysis },
    );
  },
});

const scopeProperties = {
  client_id: schema.string({ minLength: 1, maxLength: 160 }),
  browser_id: schema.string({ minLength: 1, maxLength: 160 }),
  page_id: schema.string({ minLength: 1, maxLength: 160 }),
  snapshot_generation: schema.integer(),
  url: schema.string({ maxLength: 4000 }),
};
const batchSchema = schema.object({
  action: schema.string({ enum: ["batch"] as const }),
  client_id: scopeProperties.client_id,
  browser_id: scopeProperties.browser_id,
  page_id: scopeProperties.page_id,
  operations: schema.array(schema.object({
    action: schema.string({ enum: ["input_text", "select_option", "set_value"] as const }),
    element_id: schema.string({ maxLength: 160 }),
    text: schema.optional(schema.string({ maxLength: 4096 })),
    option: schema.optional(schema.string({ maxLength: 4096 })),
    value: schema.optional(schema.string({ maxLength: 4096 })),
  }), { minItems: 1, maxItems: 32 }),
});
const attentionSchema = schema.array(schema.object({
  mapping_id: schema.optional(schema.string({ maxLength: 24 })),
  label: schema.string({ maxLength: 1200 }),
  status: schema.string({ enum: ["mismatch", "unresolved"] as const }),
  reason: schema.string({ maxLength: 500 }),
}), { maxItems: 256 });
const fillResultProperties = {
  plan_id: schema.optional(schema.string({ maxLength: 64 })),
  batch: schema.optional(batchSchema),
  deferred_count: schema.optional(schema.integer()),
  needs_attention: attentionSchema,
};
const planFill = defineTool({
  name: "plan_fill",
  description: "Plan one executable Browser batch from a fresh semantic query (prefer query.fields_only=true, optionally section/group; up to 256 nodes). Copy batch unchanged into control_browser; 1..32 admitted native field operations, no per-field calls. Then take ONE fresh query and pass it with the batch receipt to reconcile_fill. plan_id keeps expectations only in bounded provider memory for 15 minutes; restart/expiry requires fresh planning, never replay. Deferred fields continue after reconciliation. Uncertain/custom controls stay in needs_attention. Full snapshots can also propose one section-expansion or next-step click; each requires fresh observation. No Browser effects or final submission.",
  inputSchema: schema.object({
    ...scopeProperties,
    title: schema.string({ maxLength: 1000 }),
    nodes: schema.array(nodeSchema, { maxItems: 256 }),
    mapping_hints: schema.optional(schema.array(mappingHintSchema, { maxItems: 256 })),
  }),
  outputSchema: schema.object({
    phase: schema.string({ enum: ["expand_sections", "fill_fields", "advance_step", "ready_for_review"] as const }),
    ...fillResultProperties,
    section_actions: schema.optional(schema.array(sectionActionSchema, { maxItems: 1 })),
    flow_actions: schema.optional(schema.array(flowActionSchema, { maxItems: 1 })),
    review_reason: schema.optional(schema.string({ maxLength: 500 })),
  }),
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: false, openWorldHint: false },
  async execute({ title, nodes: rawNodes, mapping_hints, ...scope }) {
    const nodes = rawNodes.map(node => ({ ...node, name: node.name ?? "" }));
    const { url } = scope;
    const resume = loadResumeProfile();
    const hints: FormMappingHint[] = (mapping_hints ?? []).map((hint) => ({
      mapping_id: hint.mapping_id, label: hint.label, canonicalField: hint.canonical_field,
      resumePath: hint.resume_path, choiceValue: hint.choice_value,
    }));
    const analysis = analyzeNodes(nodes, resume, url, hints);
    const site_kind = classifySite(title, url, nodes);
    const flow = planApplicationFlow(nodes);
    const sectionExpansion = planNextSectionExpansion(nodes, {
      education: resume.education.length, experience: resume.experience.length, projects: resume.projects.length,
    });
    if (sectionExpansion) return textResult("Expand once, then observe the changed section.", {
      phase: "expand_sections", section_actions: [sectionExpansion] as SectionExpansionAction[], needs_attention: [] as Attention[],
    });
    const actions = analysis.recognized
      .map((item) => ({
        ...item,
        proposed_value: adaptValue(site_kind, item.canonical_field, item.proposed_value),
      }))
      .filter((item) => {
        const expectedFileName = item.proposed_value.split(/[\\/]/).pop() ?? item.proposed_value;
        const alreadySatisfied =
          item.current_value === item.proposed_value ||
          (item.support === "upload_file" && item.current_value === expectedFileName);
        return (
          item.support !== "manual_review" &&
          item.proposed_value.length > 0 &&
          !alreadySatisfied
        );
      })
      .map((item) => ({
        mapping_id: item.mapping_id,
        kind: item.support,
        label: item.label,
        element_id: item.element_id,
        value: item.proposed_value,
        confidence: item.confidence,
      }));

    if (!actions.length && !analysis.blockers.length) {
      if (flow.review_required) return textResult("Review required; never submit automatically.", {
        phase: "ready_for_review", review_reason: flow.review_reason, needs_attention: [] as Attention[],
      });
      if (flow.advance) return textResult("Advance once, then observe the new step.", {
        phase: "advance_step", flow_actions: [flow.advance] as FlowAction[], needs_attention: [] as Attention[],
      });
    }
    const result = createFillBatch(scope, nodes, actions, analysis.blockers.map(item => ({
      mapping_id: item.mapping_id, label: item.label, status: "unresolved" as const, reason: item.reason,
    })));
    return textResult("Execute batch once, then fresh semantic readback → reconcile_fill.", {
      phase: "fill_fields", ...result,
    });
  },
});
const reconcile = defineTool({
  name: "reconcile_fill",
  description: "Consume plan_id once with ONE fresh same-target semantic readback and the previous Browser batch receipt. Forward receipt counts/certainty unchanged, omit missing facts. Returns confirmed count and only mismatch/unresolved exceptions; never returns profile or confirmed nodes. A completed stable batch may yield a fresh bounded batch for remaining fields. Partial, uncertain or unstable receipts never generate retry effects. Target/URL changes and non-newer generations require fresh planning; old ids are never reused. In code orchestration pass readback directly here and expose only this compact result to the model.",
  inputSchema: schema.object({
    plan_id: schema.string({ maxLength: 64 }),
    ...scopeProperties,
    nodes: schema.array(nodeSchema, { maxItems: 256 }),
    receipt: schema.object({
      execution_state: schema.optional(schema.string({ maxLength: 40 })),
      requested_count: schema.optional(schema.integer()),
      completed_count: schema.optional(schema.integer()),
      remaining_count: schema.optional(schema.integer()),
      stopped_at_index: schema.optional(schema.integer()),
      stopped_execution_state: schema.optional(schema.string({ maxLength: 40 })),
      stability: schema.optional(schema.object({
        stable: schema.boolean(),
        waited_ms: schema.optional(schema.integer()),
        reason: schema.optional(schema.string({ maxLength: 500 })),
      })),
    }, { additionalProperties: true }),
  }),
  outputSchema: schema.object({ confirmed: schema.integer(), ...fillResultProperties }),
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: false, openWorldHint: false },
  async execute({ plan_id, nodes: rawNodes, receipt, ...scope }) {
    const nodes = rawNodes.map(node => ({ ...node, name: node.name ?? "" }));
    try {
      return textResult("Reconciled fresh field values.", reconcileFill(plan_id, scope, nodes, receipt));
    } catch (error) {
      return errorResult(error instanceof Error ? error.message : "Readback unavailable", { confirmed: 0, needs_attention: [] as Attention[] });
    }
  },
});

runPlugin(definePlugin({ tools: [profileGet, analyzeForm, planFill, reconcile] }));
