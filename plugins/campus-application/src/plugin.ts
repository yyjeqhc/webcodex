import {
  definePlugin,
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

const nodeSchema = schema.object({
  role: schema.string({ maxLength: 80 }),
  name: schema.string({ maxLength: 1200 }),
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
});

const actionSchema = schema.object({
  kind: schema.string({
    enum: ["input_text", "select_option", "set_value", "upload_file", "click", "manual_review"] as const,
  }),
  canonical_field: schema.string({ enum: canonicalFields }),
  resume_path: schema.string({ maxLength: 300 }),
  label: schema.string({ maxLength: 1200 }),
  element_id: schema.string({ maxLength: 160 }),
  value: schema.string({ maxLength: 4096 }),
  confidence: schema.number(),
  reason: schema.string({ maxLength: 500 }),
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
    const label = node.group_label || node.name || node.description || "";
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
      mapping?.source === "group" &&
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
  async execute({ title, url, nodes, mapping_hints }) {
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

const planFill = defineTool({
  name: "plan_fill",
  description: "Convert a bounded Browser semantic snapshot into the next safe application step. If the structured resume has more repeated education/experience/project entries than the page currently exposes and a matching add-section button is available, return exactly one section-expansion click and require a fresh snapshot before field filling. Otherwise return the normal bounded fill plan. Optional stable mapping hints can teach unlabeled controls by mapping_id. It never executes browser actions or submits a form.",
  inputSchema: schema.object({
    title: schema.string({ maxLength: 1000 }),
    url: schema.string({ maxLength: 4000 }),
    nodes: schema.array(nodeSchema, { maxItems: 256 }),
    mapping_hints: schema.optional(schema.array(mappingHintSchema, { maxItems: 256 })),
  }),
  outputSchema: schema.object({
    site_kind: schema.string({ enum: ["greenhouse-like", "lever-like", "campus-cn-like", "generic"] as const }),
    phase: schema.string({ enum: ["expand_sections", "fill_fields", "advance_step", "ready_for_review"] as const }),
    section_actions: schema.array(sectionActionSchema, { maxItems: 1 }),
    flow_actions: schema.array(flowActionSchema, { maxItems: 1 }),
    step_marker: schema.string({ maxLength: 200 }),
    review_required: schema.boolean(),
    review_reason: schema.string({ maxLength: 500 }),
    actions: schema.array(actionSchema, { maxItems: 256 }),
    blockers: schema.array(blockerSchema, { maxItems: 256 }),
    blocker_count: schema.integer(),
    missing_profile_fields: schema.array(missingProfileFieldSchema, { maxItems: 128 }),
    unmapped_candidates: schema.array(unmappedCandidateSchema, { maxItems: 256 }),
    structure_signature: schema.string({ maxLength: 64 }),
    mapping_cache_hit: schema.boolean(),
    mapping_cache_entries: schema.integer(),
    mapping_memory_hit: schema.boolean(),
    mapping_memory_entries: schema.integer(),
  }),
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  async execute({ title, url, nodes, mapping_hints }) {
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
    const flowContext = planApplicationFlow(nodes);
    const sectionExpansion = planNextSectionExpansion(nodes, {
      education: resume.education.length,
      experience: resume.experience.length,
      projects: resume.projects.length,
    });
    if (sectionExpansion) {
      return textResult(
        `Expand ${sectionExpansion.collection} from ${sectionExpansion.current_count} to ${sectionExpansion.target_count}; take a fresh Browser snapshot before filling fields.`,
        {
          site_kind,
          phase: "expand_sections",
          section_actions: [sectionExpansion] as SectionExpansionAction[],
          flow_actions: [] as FlowAction[],
          step_marker: flowContext.step_marker,
          review_required: false,
          review_reason: "",
          actions: [] as never[],
          blockers: [] as Array<{ mapping_id: string; label: string; role: string; element_id: string; reason: string }>,
          blocker_count: 0,
          missing_profile_fields: analysis.missing_profile_fields,
          unmapped_candidates: analysis.unmapped_candidates,
          structure_signature: analysis.structure_signature,
          mapping_cache_hit: analysis.mapping_cache_hit,
          mapping_cache_entries: analysis.mapping_cache_entries,
          mapping_memory_hit: analysis.mapping_memory_hit,
          mapping_memory_entries: analysis.mapping_memory_entries,
        },
      );
    }

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
        kind: item.support,
        canonical_field: item.canonical_field,
        resume_path: item.resume_path,
        label: item.label,
        element_id: item.element_id,
        value: item.proposed_value,
        confidence: item.confidence,
        reason:
          item.support === "input_text"
            ? "Use Browser input_text."
            : item.support === "select_option"
              ? "Use Browser select_option with the exact native value or visible label."
              : item.support === "set_value"
                ? "Use Browser set_value for the native structured form value."
                : item.support === "click"
                  ? "Use Browser click on the option whose group semantics and label match the desired profile choice."
                  : "Use Browser upload_file with the authorized resume project and this project-relative path.",
      }));
    if (actions.length === 0 && analysis.blockers.length === 0) {
      const flow = flowContext;
      if (flow.review_required) {
        return textResult(
          flow.review_reason,
          {
            site_kind,
            phase: "ready_for_review",
            section_actions: [] as SectionExpansionAction[],
            flow_actions: [] as FlowAction[],
            step_marker: flow.step_marker,
            review_required: true,
            review_reason: flow.review_reason,
            actions: [] as never[],
            blockers: [] as Array<{ mapping_id: string; label: string; role: string; element_id: string; reason: string }>,
            blocker_count: 0,
            missing_profile_fields: analysis.missing_profile_fields,
            unmapped_candidates: analysis.unmapped_candidates,
            structure_signature: analysis.structure_signature,
            mapping_cache_hit: analysis.mapping_cache_hit,
            mapping_cache_entries: analysis.mapping_cache_entries,
            mapping_memory_hit: analysis.mapping_memory_hit,
            mapping_memory_entries: analysis.mapping_memory_entries,
          },
        );
      }
      if (flow.advance) {
        return textResult(
          `Current step is complete. Advance with "${flow.advance.label}", then take a fresh Browser snapshot before planning again.`,
          {
            site_kind,
            phase: "advance_step",
            section_actions: [] as SectionExpansionAction[],
            flow_actions: [flow.advance] as FlowAction[],
            step_marker: flow.step_marker,
            review_required: false,
            review_reason: "",
            actions: [] as never[],
            blockers: [] as Array<{ mapping_id: string; label: string; role: string; element_id: string; reason: string }>,
            blocker_count: 0,
            missing_profile_fields: analysis.missing_profile_fields,
            unmapped_candidates: analysis.unmapped_candidates,
            structure_signature: analysis.structure_signature,
            mapping_cache_hit: analysis.mapping_cache_hit,
            mapping_cache_entries: analysis.mapping_cache_entries,
            mapping_memory_hit: analysis.mapping_memory_hit,
            mapping_memory_entries: analysis.mapping_memory_entries,
          },
        );
      }
    }

    return textResult(
      `Prepared ${actions.length} proposed actions for ${site_kind}; ${analysis.blockers.length} blockers, ${analysis.missing_profile_fields.length} missing profile fields, ${analysis.unmapped_candidates.length} teachable unmapped controls remain; mapping cache ${analysis.mapping_cache_hit ? "hit" : "miss"}, persistent memory ${analysis.mapping_memory_hit ? "hit" : "miss"}.`,
      {
        site_kind,
        phase: "fill_fields",
        section_actions: [] as SectionExpansionAction[],
        flow_actions: [] as FlowAction[],
        step_marker: flowContext.step_marker,
        review_required: false,
        review_reason: "",
        actions,
        blockers: analysis.blockers,
        blocker_count: analysis.blockers.length,
        missing_profile_fields: analysis.missing_profile_fields,
        unmapped_candidates: analysis.unmapped_candidates,
        structure_signature: analysis.structure_signature,
        mapping_cache_hit: analysis.mapping_cache_hit,
        mapping_cache_entries: analysis.mapping_cache_entries,
        mapping_memory_hit: analysis.mapping_memory_hit,
        mapping_memory_entries: analysis.mapping_memory_entries,
      },
    );
  },
});

runPlugin(definePlugin({ tools: [profileGet, analyzeForm, planFill] }));
