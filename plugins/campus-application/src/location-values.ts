import { resolveResumeValue, type CanonicalField, type ResumeProfile } from "./resume.js";
import type { SnapshotNode } from "./form-cache.js";
import { normalizeChoice, validChoicePath } from "./widget-values.js";

type PathResult = { path: string[]; reason?: never } | { path?: never; reason: string };
const compositePersonal = ["native_place", "household_registration", "student_origin"] as const;
const pathSeparators = /\s*(?:\/|>|→|›|»)\s*/u;

export function resolveChoicePath(
  resume: ResumeProfile, field: CanonicalField, resumePath: string,
  value: string, node: SnapshotNode, taughtValue?: string,
): PathResult {
  const hierarchical = /cascad|tree|hierarch/iu.test(node.form_context?.component_hint ?? "");
  const personalComposite = compositePersonal.some(name => resumePath === "personal." + name);
  const residenceComposite = ["contact.city", "contact.address", "contact.district"].includes(resumePath)
    && (hierarchical || /现居住地|现居地|居住地|current\s*location/iu.test(node.name));
  const preference = field === "preferred_locations";
  const pathFor = (parts: string[]): PathResult => {
    const path = parts.map(part => part.trim());
    return validChoicePath(path) ? { path } : {
      reason: "Choice path needs 1..4 nonempty exact labels, each at most 4096 UTF-8 bytes.",
    };
  };
  if (taughtValue !== undefined) return pathFor(
    personalComposite || residenceComposite || preference || hierarchical
      ? taughtValue.split(pathSeparators) : [taughtValue],
  );
  if (personalComposite || residenceComposite) {
    const paths = personalComposite
      ? ["province", "city", "district"].map(axis => resumePath + "_" + axis)
      : ["contact.province", "contact.city", "contact.district"];
    const parts = paths.map(path => resolveResumeValue(resume, path).value.trim());
    if (parts[0] && parts[1]) return pathFor(parts[2] ? parts : parts.slice(0, 2));
    if (parts.some(Boolean)) {
      if (parts[0] && !parts[1] && !parts[2] && normalizeChoice(value) === normalizeChoice(parts[0])) return pathFor([parts[0]]);
      return { reason: "Location has an incomplete province/city path; complete its own profile fields or teach an exact choice_value path." };
    }
  }
  if (preference && resumePath === "job_preferences.preferred_locations") {
    const alternatives = resume.job_preferences.preferred_locations.filter(item => item.trim());
    if (alternatives.length !== 1) return {
      reason: "Several preferred locations are alternatives, not hierarchy levels; teach the intended indexed resume_path or exact choice_value.",
    };
    return pathFor(alternatives[0]!.split(pathSeparators));
  }
  return pathFor(personalComposite || residenceComposite || preference || hierarchical ? value.split(pathSeparators) : [value]);
}
