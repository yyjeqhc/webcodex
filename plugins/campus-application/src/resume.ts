import { schema } from "@yyjeqhc/webcodex-plugin-sdk";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export const canonicalFields = [
  "full_name",
  "first_name",
  "last_name",
  "gender",
  "birth_date",
  "id_type",
  "id_number",
  "ethnicity",
  "political_status",
  "health_status",
  "height_cm",
  "weight_kg",
  "native_place",
  "native_place_province",
  "native_place_city",
  "native_place_district",
  "household_registration",
  "household_registration_province",
  "household_registration_city",
  "household_registration_district",
  "student_origin",
  "student_origin_province",
  "student_origin_city",
  "student_origin_district",
  "is_fresh_graduate",
  "marital_status",
  "email",
  "phone",
  "current_residence_province",
  "city",
  "district",
  "address",
  "university",
  "education_province",
  "education_city",
  "degree",
  "major",
  "education_start_date",
  "graduation_date",
  "gpa",
  "current_company",
  "current_title",
  "experience_location",
  "experience_start_date",
  "experience_end_date",
  "experience_summary",
  "project_name",
  "project_role",
  "project_start_date",
  "project_end_date",
  "project_technologies",
  "project_summary",
  "project_url",
  "linkedin",
  "github",
  "portfolio",
  "cover_letter",
  "resume_path",
  "accept_transfer",
  "preferred_locations",
  "available_date",
] as const;

export type CanonicalField = (typeof canonicalFields)[number];
export type CanonicalProfile = Record<CanonicalField, string>;

const canonicalResumePaths: Record<CanonicalField, string> = {
  full_name: "identity.full_name",
  first_name: "identity.first_name",
  last_name: "identity.last_name",
  gender: "personal.gender",
  birth_date: "personal.birth_date",
  id_type: "personal.id_type",
  id_number: "personal.id_number",
  ethnicity: "personal.ethnicity",
  political_status: "personal.political_status",
  health_status: "personal.health_status",
  height_cm: "personal.height_cm",
  weight_kg: "personal.weight_kg",
  native_place: "personal.native_place",
  native_place_province: "personal.native_place_province",
  native_place_city: "personal.native_place_city",
  native_place_district: "personal.native_place_district",
  household_registration: "personal.household_registration",
  household_registration_province: "personal.household_registration_province",
  household_registration_city: "personal.household_registration_city",
  household_registration_district: "personal.household_registration_district",
  student_origin: "personal.student_origin",
  student_origin_province: "personal.student_origin_province",
  student_origin_city: "personal.student_origin_city",
  student_origin_district: "personal.student_origin_district",
  is_fresh_graduate: "personal.is_fresh_graduate",
  marital_status: "personal.marital_status",
  email: "contact.email",
  phone: "contact.phone",
  current_residence_province: "contact.province",
  city: "contact.city",
  district: "contact.district",
  address: "contact.address",
  university: "education[0].school",
  education_province: "education[0].province",
  education_city: "education[0].city",
  degree: "education[0].degree",
  major: "education[0].major",
  education_start_date: "education[0].start_date",
  graduation_date: "education[0].graduation_date",
  gpa: "education[0].gpa",
  current_company: "experience[0].company",
  current_title: "experience[0].title",
  experience_location: "experience[0].location",
  experience_start_date: "experience[0].start_date",
  experience_end_date: "experience[0].end_date",
  experience_summary: "experience[0].summary",
  project_name: "projects[0].name",
  project_role: "projects[0].role",
  project_start_date: "projects[0].start_date",
  project_end_date: "projects[0].end_date",
  project_technologies: "projects[0].technologies",
  project_summary: "projects[0].summary",
  project_url: "projects[0].url",
  linkedin: "links.linkedin",
  github: "links.github",
  portfolio: "links.portfolio",
  cover_letter: "application.cover_letter",
  resume_path: "attachments.resume_path",
  accept_transfer: "job_preferences.accept_transfer",
  preferred_locations: "job_preferences.preferred_locations",
  available_date: "job_preferences.available_date",
};

export type ResumeProfile = {
  schema_version: number;
  identity: {
    full_name: string;
    first_name: string;
    last_name: string;
  };
  personal?: {
    gender: string;
    birth_date: string;
    id_type: string;
    id_number: string;
    ethnicity: string;
    political_status: string;
    health_status?: string;
    height_cm?: string;
    weight_kg?: string;
    native_place: string;
    native_place_province?: string;
    native_place_city?: string;
    native_place_district?: string;
    household_registration: string;
    household_registration_province?: string;
    household_registration_city?: string;
    household_registration_district?: string;
    student_origin?: string;
    student_origin_province?: string;
    student_origin_city?: string;
    student_origin_district?: string;
    is_fresh_graduate?: string;
    marital_status: string;
  };
  contact: {
    email: string;
    phone: string;
    province?: string;
    city: string;
    district?: string;
    address: string;
  };
  links: {
    linkedin: string;
    github: string;
    portfolio: string;
  };
  education: Array<{
    school: string;
    degree: string;
    major: string;
    province?: string;
    city?: string;
    start_date: string;
    graduation_date: string;
    gpa: string;
    education_type: string;
    advisor: string;
    laboratory: string;
  }>;
  experience: Array<{
    company: string;
    title: string;
    location: string;
    start_date: string;
    end_date: string;
    summary: string;
  }>;
  projects: Array<{
    name: string;
    role: string;
    start_date: string;
    end_date: string;
    technologies: string[];
    summary: string;
    url: string;
  }>;
  campus_experience: Array<{
    organization: string;
    role: string;
    start_date: string;
    end_date: string;
    summary: string;
  }>;
  skills: string[];
  languages: Array<{
    name: string;
    level: string;
    score: string;
  }>;
  job_preferences: {
    accept_transfer: string;
    preferred_locations: string[];
    target_roles: string[];
    employment_type: string;
    available_date: string;
  };
  application: {
    cover_letter: string;
  };
  attachments: {
    resume_path: string;
  };
};

const educationSchema = schema.object({
  school: schema.string({ maxLength: 300 }),
  degree: schema.string({ maxLength: 200 }),
  major: schema.string({ maxLength: 300 }),
  province: schema.optional(schema.string({ maxLength: 200 })),
  city: schema.optional(schema.string({ maxLength: 200 })),
  start_date: schema.string({ maxLength: 100 }),
  graduation_date: schema.string({ maxLength: 100 }),
  gpa: schema.string({ maxLength: 100 }),
  education_type: schema.string({ maxLength: 100 }),
  advisor: schema.string({ maxLength: 200 }),
  laboratory: schema.string({ maxLength: 300 }),
});

const experienceSchema = schema.object({
  company: schema.string({ maxLength: 300 }),
  title: schema.string({ maxLength: 300 }),
  location: schema.string({ maxLength: 200 }),
  start_date: schema.string({ maxLength: 100 }),
  end_date: schema.string({ maxLength: 100 }),
  summary: schema.string({ maxLength: 4000 }),
});

const projectSchema = schema.object({
  name: schema.string({ maxLength: 300 }),
  role: schema.string({ maxLength: 300 }),
  start_date: schema.string({ maxLength: 100 }),
  end_date: schema.string({ maxLength: 100 }),
  technologies: schema.array(schema.string({ maxLength: 100 }), { maxItems: 64 }),
  summary: schema.string({ maxLength: 4000 }),
  url: schema.string({ maxLength: 1000 }),
});

const campusExperienceSchema = schema.object({
  organization: schema.string({ maxLength: 300 }),
  role: schema.string({ maxLength: 300 }),
  start_date: schema.string({ maxLength: 100 }),
  end_date: schema.string({ maxLength: 100 }),
  summary: schema.string({ maxLength: 4000 }),
});

const languageSchema = schema.object({
  name: schema.string({ maxLength: 100 }),
  level: schema.string({ maxLength: 100 }),
  score: schema.string({ maxLength: 100 }),
});

export const resumeProfileSchema = schema.object({
  schema_version: schema.integer(),
  identity: schema.object({
    full_name: schema.string({ maxLength: 200 }),
    first_name: schema.string({ maxLength: 100 }),
    last_name: schema.string({ maxLength: 100 }),
  }),
  personal: schema.optional(schema.object({
    gender: schema.string({ maxLength: 100 }),
    birth_date: schema.string({ maxLength: 100 }),
    id_type: schema.string({ maxLength: 100 }),
    id_number: schema.string({ maxLength: 200 }),
    ethnicity: schema.string({ maxLength: 100 }),
    political_status: schema.string({ maxLength: 100 }),
    health_status: schema.optional(schema.string({ maxLength: 100 })),
    height_cm: schema.optional(schema.string({ maxLength: 100 })),
    weight_kg: schema.optional(schema.string({ maxLength: 100 })),
    native_place: schema.string({ maxLength: 300 }),
    native_place_province: schema.optional(schema.string({ maxLength: 200 })),
    native_place_city: schema.optional(schema.string({ maxLength: 200 })),
    native_place_district: schema.optional(schema.string({ maxLength: 200 })),
    household_registration: schema.string({ maxLength: 300 }),
    household_registration_province: schema.optional(schema.string({ maxLength: 200 })),
    household_registration_city: schema.optional(schema.string({ maxLength: 200 })),
    household_registration_district: schema.optional(schema.string({ maxLength: 200 })),
    student_origin: schema.optional(schema.string({ maxLength: 300 })),
    student_origin_province: schema.optional(schema.string({ maxLength: 200 })),
    student_origin_city: schema.optional(schema.string({ maxLength: 200 })),
    student_origin_district: schema.optional(schema.string({ maxLength: 200 })),
    is_fresh_graduate: schema.optional(schema.string({ maxLength: 100 })),
    marital_status: schema.string({ maxLength: 100 }),
  })),
  contact: schema.object({
    email: schema.string({ maxLength: 320 }),
    phone: schema.string({ maxLength: 100 }),
    province: schema.optional(schema.string({ maxLength: 200 })),
    city: schema.string({ maxLength: 200 }),
    district: schema.optional(schema.string({ maxLength: 200 })),
    address: schema.string({ maxLength: 500 }),
  }),
  links: schema.object({
    linkedin: schema.string({ maxLength: 1000 }),
    github: schema.string({ maxLength: 1000 }),
    portfolio: schema.string({ maxLength: 1000 }),
  }),
  education: schema.array(educationSchema, { maxItems: 16 }),
  experience: schema.array(experienceSchema, { maxItems: 32 }),
  projects: schema.array(projectSchema, { maxItems: 32 }),
  campus_experience: schema.array(campusExperienceSchema, { maxItems: 32 }),
  skills: schema.array(schema.string({ maxLength: 200 }), { maxItems: 128 }),
  languages: schema.array(languageSchema, { maxItems: 32 }),
  job_preferences: schema.object({
    accept_transfer: schema.string({ maxLength: 100 }),
    preferred_locations: schema.array(schema.string({ maxLength: 200 }), { maxItems: 32 }),
    target_roles: schema.array(schema.string({ maxLength: 300 }), { maxItems: 32 }),
    employment_type: schema.string({ maxLength: 100 }),
    available_date: schema.string({ maxLength: 100 }),
  }),
  application: schema.object({
    cover_letter: schema.string({ maxLength: 4000 }),
  }),
  attachments: schema.object({
    resume_path: schema.string({ maxLength: 4096 }),
  }),
});

export const canonicalProfileSchema = schema.object({
  full_name: schema.string({ maxLength: 200 }),
  first_name: schema.string({ maxLength: 100 }),
  last_name: schema.string({ maxLength: 100 }),
  gender: schema.string({ maxLength: 100 }),
  birth_date: schema.string({ maxLength: 100 }),
  id_type: schema.string({ maxLength: 100 }),
  id_number: schema.string({ maxLength: 200 }),
  ethnicity: schema.string({ maxLength: 100 }),
  political_status: schema.string({ maxLength: 100 }),
  health_status: schema.string({ maxLength: 100 }),
  height_cm: schema.string({ maxLength: 100 }),
  weight_kg: schema.string({ maxLength: 100 }),
  native_place: schema.string({ maxLength: 300 }),
  native_place_province: schema.string({ maxLength: 200 }),
  native_place_city: schema.string({ maxLength: 200 }),
  native_place_district: schema.string({ maxLength: 200 }),
  household_registration: schema.string({ maxLength: 300 }),
  household_registration_province: schema.string({ maxLength: 200 }),
  household_registration_city: schema.string({ maxLength: 200 }),
  household_registration_district: schema.string({ maxLength: 200 }),
  student_origin: schema.string({ maxLength: 300 }),
  student_origin_province: schema.string({ maxLength: 200 }),
  student_origin_city: schema.string({ maxLength: 200 }),
  student_origin_district: schema.string({ maxLength: 200 }),
  is_fresh_graduate: schema.string({ maxLength: 100 }),
  marital_status: schema.string({ maxLength: 100 }),
  email: schema.string({ maxLength: 320 }),
  phone: schema.string({ maxLength: 100 }),
  current_residence_province: schema.string({ maxLength: 200 }),
  city: schema.string({ maxLength: 200 }),
  district: schema.string({ maxLength: 200 }),
  address: schema.string({ maxLength: 500 }),
  university: schema.string({ maxLength: 300 }),
  education_province: schema.string({ maxLength: 200 }),
  education_city: schema.string({ maxLength: 200 }),
  degree: schema.string({ maxLength: 200 }),
  major: schema.string({ maxLength: 300 }),
  education_start_date: schema.string({ maxLength: 100 }),
  graduation_date: schema.string({ maxLength: 100 }),
  gpa: schema.string({ maxLength: 100 }),
  current_company: schema.string({ maxLength: 300 }),
  current_title: schema.string({ maxLength: 300 }),
  experience_location: schema.string({ maxLength: 200 }),
  experience_start_date: schema.string({ maxLength: 100 }),
  experience_end_date: schema.string({ maxLength: 100 }),
  experience_summary: schema.string({ maxLength: 4000 }),
  project_name: schema.string({ maxLength: 300 }),
  project_role: schema.string({ maxLength: 300 }),
  project_start_date: schema.string({ maxLength: 100 }),
  project_end_date: schema.string({ maxLength: 100 }),
  project_technologies: schema.string({ maxLength: 2000 }),
  project_summary: schema.string({ maxLength: 4000 }),
  project_url: schema.string({ maxLength: 1000 }),
  linkedin: schema.string({ maxLength: 1000 }),
  github: schema.string({ maxLength: 1000 }),
  portfolio: schema.string({ maxLength: 1000 }),
  cover_letter: schema.string({ maxLength: 4000 }),
  resume_path: schema.string({ maxLength: 4096 }),
  accept_transfer: schema.string({ maxLength: 100 }),
  preferred_locations: schema.string({ maxLength: 6462 }),
  available_date: schema.string({ maxLength: 100 }),
});

export function loadResumeProfileFromFile(path: string | URL): ResumeProfile {
  const raw = JSON.parse(readFileSync(path, "utf8")) as unknown;
  if (
    typeof raw !== "object" ||
    raw === null ||
    !("schema_version" in raw) ||
    (raw as { schema_version?: unknown }).schema_version !== 1
  ) {
    throw new Error("campus-application profile must use schema_version 1");
  }
  const resume = raw as ResumeProfile;
  if (
    !resume.identity ||
    !resume.contact ||
    !resume.links ||
    !Array.isArray(resume.education) ||
    !Array.isArray(resume.experience) ||
    !Array.isArray(resume.projects) ||
    !Array.isArray(resume.campus_experience) ||
    !Array.isArray(resume.skills) ||
    !Array.isArray(resume.languages) ||
    !resume.job_preferences ||
    !resume.application ||
    !resume.attachments
  ) {
    throw new Error("campus-application structured profile is incomplete");
  }
  return resume;
}

export function loadResumeProfile(): ResumeProfile {
  const configured = process.env.WEBCODEX_CAMPUS_APPLICATION_PROFILE?.trim();
  const path =
    configured && configured.length > 0
      ? resolve(configured)
      : resolve(process.cwd(), "profile.json");
  return loadResumeProfileFromFile(path);
}

export function canonicalProfileFromResume(resume: ResumeProfile): CanonicalProfile {
  const profile = {} as CanonicalProfile;
  for (const field of canonicalFields) {
    profile[field] = resolveResumeValue(resume, canonicalResumePaths[field]).value;
  }
  return profile;
}

export function resumePathForCanonicalField(field: CanonicalField): string {
  return canonicalResumePaths[field];
}

export function resolveResumeValue(
  resume: ResumeProfile,
  path: string,
): { found: boolean; value: string } {
  const tokens: Array<string | number> = [];
  let cursor = 0;
  const pattern = /([A-Za-z_][A-Za-z0-9_]*)|\[(\d+)\]/g;
  for (const match of path.matchAll(pattern)) {
    if (match.index !== cursor) return { found: false, value: "" };
    if (match[1] !== undefined) {
      tokens.push(match[1]);
    } else if (match[2] !== undefined) {
      tokens.push(Number(match[2]));
    }
    cursor = match.index + match[0].length;
    if (path[cursor] === ".") cursor += 1;
  }
  if (cursor !== path.length || tokens.length === 0) {
    return { found: false, value: "" };
  }

  let value: unknown = resume;
  for (const token of tokens) {
    if (typeof token === "number") {
      if (!Array.isArray(value) || token < 0 || token >= value.length) {
        return { found: false, value: "" };
      }
      value = value[token];
      continue;
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
      return { found: false, value: "" };
    }
    const record = value as Record<string, unknown>;
    if (!(token in record)) return { found: false, value: "" };
    value = record[token];
  }

  if (typeof value === "string") return { found: true, value };
  if (Array.isArray(value) && value.every((item) => typeof item === "string")) {
    return { found: true, value: value.join(", ") };
  }
  return { found: false, value: "" };
}
