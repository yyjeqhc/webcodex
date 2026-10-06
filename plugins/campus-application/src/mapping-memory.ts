import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { canonicalFields, type CanonicalField } from "./resume.js";
import type { FormMappingHint } from "./form-cache.js";

type PersistedMappingHint = {
  mapping_id: string;
  canonical_field: CanonicalField;
  resume_path: string;
  label?: string;
  choice_value?: string;
  updated_at: number;
};

type PersistentEntry = {
  site: string;
  structure_signature: string;
  updated_at: number;
  hints: PersistedMappingHint[];
};

type PersistentDocument = {
  version: 1;
  entries: Record<string, PersistentEntry>;
};

export type PersistableMappingHint = {
  mapping_id: string;
  canonicalField: CanonicalField;
  resumePath: string;
  label?: string | undefined;
  choiceValue?: string | undefined;
};

const MAX_MEMORY_ENTRIES = 256;
const MAX_HINTS_PER_ENTRY = 256;
const canonicalFieldSet = new Set<string>(canonicalFields);

function emptyDocument(): PersistentDocument {
  return { version: 1, entries: {} };
}

function configuredPath(): string {
  const configured = process.env.WEBCODEX_CAMPUS_APPLICATION_MAPPING_MEMORY?.trim();
  return configured && configured.length > 0
    ? resolve(configured)
    : resolve(process.cwd(), "mapping-memory.json");
}

function memoryKey(site: string, signature: string): string {
  return `${site}\u0000${signature}`;
}

export function mappingSiteIdentity(url: string): string {
  try {
    const parsed = new URL(url);
    return parsed.hostname.toLowerCase() || "unknown";
  } catch {
    return "unknown";
  }
}

function readDocument(path: string): PersistentDocument {
  try {
    const raw = JSON.parse(readFileSync(path, "utf8")) as unknown;
    if (
      typeof raw !== "object" ||
      raw === null ||
      (raw as { version?: unknown }).version !== 1 ||
      typeof (raw as { entries?: unknown }).entries !== "object" ||
      (raw as { entries?: unknown }).entries === null
    ) {
      return emptyDocument();
    }
    return raw as PersistentDocument;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return emptyDocument();
    }
    return emptyDocument();
  }
}

function writeDocument(path: string, document: PersistentDocument): void {
  mkdirSync(dirname(path), { recursive: true });
  const tempPath = `${path}.${process.pid}.tmp`;
  writeFileSync(tempPath, `${JSON.stringify(document, null, 2)}\n`, {
    encoding: "utf8",
    mode: 0o600,
  });
  renameSync(tempPath, path);
}

function validPersistedHint(value: unknown): value is PersistedMappingHint {
  if (typeof value !== "object" || value === null) return false;
  const hint = value as Partial<PersistedMappingHint>;
  return (
    typeof hint.mapping_id === "string" &&
    hint.mapping_id.length > 0 &&
    typeof hint.canonical_field === "string" &&
    canonicalFieldSet.has(hint.canonical_field) &&
    typeof hint.resume_path === "string" &&
    hint.resume_path.length > 0 &&
    typeof hint.updated_at === "number"
  );
}

export function loadPersistentMappingHints(
  url: string,
  structureSignature: string,
  path = configuredPath(),
): { hit: boolean; entries: number; hints: FormMappingHint[] } {
  const document = readDocument(path);
  const site = mappingSiteIdentity(url);
  const entry = document.entries[memoryKey(site, structureSignature)];
  if (!entry || !Array.isArray(entry.hints)) {
    return {
      hit: false,
      entries: Object.keys(document.entries).length,
      hints: [],
    };
  }
  const hints = entry.hints
    .filter(validPersistedHint)
    .slice(0, MAX_HINTS_PER_ENTRY)
    .map((hint) => ({
      mapping_id: hint.mapping_id,
      canonicalField: hint.canonical_field,
      resumePath: hint.resume_path,
      ...(hint.label === undefined ? {} : { label: hint.label }),
      ...(hint.choice_value === undefined ? {} : { choiceValue: hint.choice_value }),
    }));
  return {
    hit: hints.length > 0,
    entries: Object.keys(document.entries).length,
    hints,
  };
}

export function rememberPersistentMappingHints(
  url: string,
  structureSignature: string,
  hints: readonly PersistableMappingHint[],
  path = configuredPath(),
): number {
  if (hints.length === 0) {
    return Object.keys(readDocument(path).entries).length;
  }

  const document = readDocument(path);
  const site = mappingSiteIdentity(url);
  const key = memoryKey(site, structureSignature);
  const previous = document.entries[key];
  const merged = new Map<string, PersistedMappingHint>();
  for (const hint of previous?.hints ?? []) {
    if (validPersistedHint(hint)) merged.set(hint.mapping_id, hint);
  }

  const now = Date.now();
  for (const hint of hints.slice(0, MAX_HINTS_PER_ENTRY)) {
    merged.set(hint.mapping_id, {
      mapping_id: hint.mapping_id,
      canonical_field: hint.canonicalField,
      resume_path: hint.resumePath,
      ...(hint.label === undefined ? {} : { label: hint.label }),
      ...(hint.choiceValue === undefined ? {} : { choice_value: hint.choiceValue }),
      updated_at: now,
    });
  }

  document.entries[key] = {
    site,
    structure_signature: structureSignature,
    updated_at: now,
    hints: [...merged.values()]
      .sort((a, b) => b.updated_at - a.updated_at)
      .slice(0, MAX_HINTS_PER_ENTRY),
  };

  const keys = Object.keys(document.entries);
  if (keys.length > MAX_MEMORY_ENTRIES) {
    keys
      .sort(
        (a, b) =>
          (document.entries[a]?.updated_at ?? 0) -
          (document.entries[b]?.updated_at ?? 0),
      )
      .slice(0, keys.length - MAX_MEMORY_ENTRIES)
      .forEach((oldest) => {
        delete document.entries[oldest];
      });
  }

  writeDocument(path, document);
  return Object.keys(document.entries).length;
}
