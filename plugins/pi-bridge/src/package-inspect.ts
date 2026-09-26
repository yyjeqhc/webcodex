const REGISTRY_ORIGIN = "https://registry.npmjs.org";
const FETCH_TIMEOUT_MS = 10_000;
const MAX_DEPENDENCIES = 512;
const MAX_SCRIPT_CHARS = 4096;
const MAX_MANIFEST_JSON_CHARS = 16 * 1024;
const MAX_REGISTRY_RESPONSE_BYTES = 1024 * 1024;

const LIFECYCLE_SCRIPT_NAMES = [
  "preinstall",
  "install",
  "postinstall",
  "prepare",
  "prepack",
  "postpack",
] as const;

export interface NpmSource {
  source: string;
  name: string;
  selector: string;
}

export interface PackageDependencyEntry {
  name: string;
  range: string;
}

export interface PackageScriptEntry {
  name: string;
  command: string;
}

export interface NpmPackageInspection {
  source: string;
  registry: typeof REGISTRY_ORIGIN;
  name: string;
  requestedSelector: string;
  resolvedVersion: string;
  description: string;
  license: string;
  homepage: string;
  repository: string;
  deprecated: string;
  enginesNode: string;
  distTarballHost: string;
  dependencies: PackageDependencyEntry[];
  peerDependencies: PackageDependencyEntry[];
  optionalDependencies: PackageDependencyEntry[];
  lifecycleScripts: PackageScriptEntry[];
  hasLifecycleScripts: boolean;
  piManifestJson: string;
  piManifestTruncated: boolean;
  sourceReviewComplete: false;
  notes: string[];
}

function validPackageName(value: string): boolean {
  if (!value || value.length > 214) return false;
  if (value.startsWith("@")) {
    return /^@[a-z0-9][a-z0-9._~-]*\/[a-z0-9][a-z0-9._~-]*$/iu.test(value);
  }
  return /^[a-z0-9][a-z0-9._~-]*$/iu.test(value);
}

export function parseNpmPackageSource(input: string): NpmSource {
  const source = input.trim();
  if (!source.startsWith("npm:")) {
    throw new Error("pi_package_inspect currently supports only explicit npm: package sources");
  }
  const spec = source.slice(4);
  if (!spec) throw new Error("npm package source is missing a package name");

  let name = spec;
  let selector = "latest";
  if (spec.startsWith("@")) {
    const slash = spec.indexOf("/");
    if (slash < 2) throw new Error("scoped npm package source is invalid");
    const versionSeparator = spec.indexOf("@", slash + 1);
    if (versionSeparator >= 0) {
      name = spec.slice(0, versionSeparator);
      selector = spec.slice(versionSeparator + 1) || "latest";
    }
  } else {
    const versionSeparator = spec.lastIndexOf("@");
    if (versionSeparator > 0) {
      name = spec.slice(0, versionSeparator);
      selector = spec.slice(versionSeparator + 1) || "latest";
    }
  }

  if (!validPackageName(name)) throw new Error("npm package name is invalid");
  if (!selector || selector.length > 256 || /[\u0000-\u001f\u007f]/u.test(selector)) {
    throw new Error("npm package selector is invalid");
  }
  if (/^(?:https?:|git\+|file:)/iu.test(selector)) {
    throw new Error("npm package selector must be a version or dist-tag, not a URL");
  }
  return { source, name, selector };
}

function dependencyEntries(value: unknown): PackageDependencyEntry[] {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return [];
  return Object.entries(value as Record<string, unknown>)
    .filter((entry): entry is [string, string] => typeof entry[1] === "string")
    .sort(([a], [b]) => a.localeCompare(b))
    .slice(0, MAX_DEPENDENCIES)
    .map(([name, range]) => ({ name, range: range.slice(0, 1024) }));
}

function lifecycleScripts(value: unknown): PackageScriptEntry[] {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return [];
  const scripts = value as Record<string, unknown>;
  return LIFECYCLE_SCRIPT_NAMES.flatMap((name) => {
    const command = scripts[name];
    return typeof command === "string"
      ? [{ name, command: command.slice(0, MAX_SCRIPT_CHARS) }]
      : [];
  });
}

function boundedString(value: unknown, max = 4000): string {
  return typeof value === "string" ? value.slice(0, max) : "";
}

function repositoryString(value: unknown): string {
  if (typeof value === "string") return value.slice(0, 4000);
  if (typeof value !== "object" || value === null || Array.isArray(value)) return "";
  const repo = value as Record<string, unknown>;
  return boundedString(repo.url);
}

function safeManifestJson(value: unknown): { json: string; truncated: boolean } {
  if (value === undefined) return { json: "", truncated: false };
  try {
    const text = JSON.stringify(value);
    if (text.length <= MAX_MANIFEST_JSON_CHARS) {
      return { json: text, truncated: false };
    }
    return {
      json: JSON.stringify({ truncated: true, originalChars: text.length }),
      truncated: true,
    };
  } catch {
    return { json: "", truncated: false };
  }
}

async function boundedRegistryJson(response: Response): Promise<Record<string, unknown>> {
  const declaredLength = response.headers.get("content-length");
  if (declaredLength !== null) {
    const parsed = Number.parseInt(declaredLength, 10);
    if (Number.isFinite(parsed) && parsed > MAX_REGISTRY_RESPONSE_BYTES) {
      throw new Error("npm registry metadata response exceeds the 1 MiB preview limit");
    }
  }
  if (response.body === null) {
    throw new Error("npm registry returned an empty metadata response");
  }

  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > MAX_REGISTRY_RESPONSE_BYTES) {
      await reader.cancel().catch(() => undefined);
      throw new Error("npm registry metadata response exceeds the 1 MiB preview limit");
    }
    chunks.push(value);
  }

  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new Error("npm registry returned invalid JSON metadata");
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    throw new Error("npm registry returned an invalid package manifest shape");
  }
  return parsed as Record<string, unknown>;
}

function tarballHost(value: unknown): string {
  if (typeof value !== "string") return "";
  try {
    const url = new URL(value);
    return url.protocol === "https:" ? url.host : "";
  } catch {
    return "";
  }
}

export async function inspectNpmPackage(
  input: string,
  fetchImpl: typeof fetch = fetch,
): Promise<NpmPackageInspection> {
  const parsed = parseNpmPackageSource(input);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);
  try {
    const encodedName = encodeURIComponent(parsed.name);
    const encodedSelector = encodeURIComponent(parsed.selector);
    const response = await fetchImpl(`${REGISTRY_ORIGIN}/${encodedName}/${encodedSelector}`, {
      method: "GET",
      headers: {
        accept: "application/json",
        "user-agent": "webpi-pi-bridge/package-inspect",
      },
      redirect: "error",
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`npm registry returned HTTP ${response.status}`);
    }
    const manifest = await boundedRegistryJson(response);
    if (manifest.name !== parsed.name || typeof manifest.version !== "string") {
      throw new Error("npm registry manifest did not match the requested package");
    }

    const scripts = lifecycleScripts(manifest.scripts);
    const dist =
      typeof manifest.dist === "object" && manifest.dist !== null && !Array.isArray(manifest.dist)
        ? (manifest.dist as Record<string, unknown>)
        : {};
    const engines =
      typeof manifest.engines === "object" && manifest.engines !== null && !Array.isArray(manifest.engines)
        ? (manifest.engines as Record<string, unknown>)
        : {};

    const piManifest = safeManifestJson(manifest.pi);

    return {
      source: parsed.source,
      registry: REGISTRY_ORIGIN,
      name: parsed.name,
      requestedSelector: parsed.selector,
      resolvedVersion: manifest.version.slice(0, 256),
      description: boundedString(manifest.description),
      license: boundedString(manifest.license, 512),
      homepage: boundedString(manifest.homepage),
      repository: repositoryString(manifest.repository),
      deprecated: boundedString(manifest.deprecated),
      enginesNode: boundedString(engines.node, 512),
      distTarballHost: tarballHost(dist.tarball),
      dependencies: dependencyEntries(manifest.dependencies),
      peerDependencies: dependencyEntries(manifest.peerDependencies),
      optionalDependencies: dependencyEntries(manifest.optionalDependencies),
      lifecycleScripts: scripts,
      hasLifecycleScripts: scripts.length > 0,
      piManifestJson: piManifest.json,
      piManifestTruncated: piManifest.truncated,
      sourceReviewComplete: false,
      notes: [
        "Registry metadata only: no tarball was downloaded and no lifecycle script was executed.",
        "This preview does not review package source code or transitive dependency source.",
        "Package installation and executable-extension fingerprint approval remain separate consequential decisions.",
      ],
    };
  } finally {
    clearTimeout(timer);
  }
}
