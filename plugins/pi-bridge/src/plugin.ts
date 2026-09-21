import { randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { opendir } from "node:fs/promises";
import path from "node:path";

import {
  VERSION as PI_VERSION,
} from "@earendil-works/pi-coding-agent";
import {
  definePlugin,
  defineTool,
  errorResult,
  result as pluginResult,
  runPlugin,
  schema,
  textResult,
} from "@yyjeqhc/webcodex-plugin-sdk";
import type { ToolResult } from "@yyjeqhc/webcodex-plugin-sdk";
import { PiRuntimeHost } from "./pi-runtime-host.js";
import type { ExtensionCandidateSummary } from "./pi-runtime-host.js";
import { checkedProjectPath, createGuardedFindTool, createGuardedGrepTool, createGuardedLsTool, createGuardedReadTool, readProjectFile, safeRelative } from "./project-files.js";

const TEXT_MAX_CHARS = 64 * 1024;
const PATH_MAX_CHARS = 4096;
const EXTENSION_LIMIT = 64;
const EXTENSION_NAME_MAX_CHARS = 256;
const TOOL_NAME_MAX_CHARS = 256;
const DESCRIPTION_MAX_CHARS = 4000;
const DETAILS_JSON_MAX_CHARS = 32 * 1024;

let runtimeHostPromise: Promise<PiRuntimeHost> | undefined;

function runtimeHost(): Promise<PiRuntimeHost> {
  runtimeHostPromise ??= PiRuntimeHost.create(process.cwd());
  return runtimeHostPromise;
}

function emptyResourceInventory() {
  return {
    projectTrusted: false,
    generation: 0,
    extensions: {
      candidates: 0,
      approved: 0,
      pending: 0,
      loaded: 0,
      errors: [] as Array<{ path: string; error: string }>,
    },
    skills: [] as Array<{
      name: string;
      description: string;
      path: string;
      disableModelInvocation: boolean;
    }>,
    prompts: [] as Array<{ name: string; description: string; path: string }>,
    contextFiles: [] as Array<{ path: string; chars: number }>,
    themes: [] as Array<{ name: string }>,
    packages: [] as Array<{
      source: string;
      scope: "user" | "project";
      filtered: boolean;
      installed: boolean;
    }>,
  };
}

interface TextStructured {
  readonly text: string;
  readonly truncated: boolean;
  readonly engine: "pi";
}

interface ExtensionEntry {
  readonly name: string;
  readonly kind: "file" | "package";
  readonly entrypoints: string[];
}

interface ExtensionInventoryStructured {
  readonly extensions: ExtensionEntry[];
  readonly totalCount: number;
  readonly truncated: boolean;
  readonly directoryPresent: boolean;
}

function safePackageSource(value: string): string {
  const source = value.trim();
  if (!source || source.length > PATH_MAX_CHARS || [...source].some((character) => character < " ")) {
    throw new Error("Pi package source is invalid");
  }
  if (path.isAbsolute(source) || /^file:/iu.test(source)) {
    throw new Error("remote Pi package mutation does not accept absolute/file package sources");
  }
  if (source === ".." || source.startsWith(`..${path.sep}`) || source.startsWith("../")) {
    throw new Error("Pi package source escapes the configured project root");
  }
  if (source.startsWith(".")) {
    const root = path.resolve(process.cwd());
    const absolute = path.resolve(root, source);
    const prefix = root.endsWith(path.sep) ? root : root + path.sep;
    if (absolute !== root && !absolute.startsWith(prefix)) {
      throw new Error("Pi package source escapes the configured project root");
    }
  }
  if (/^https?:\/\//iu.test(source)) {
    const parsed = new URL(source);
    if (parsed.username || parsed.password) {
      throw new Error("Pi package source URL must not embed credentials");
    }
  }
  return source;
}

function requireLifecycleConfirmation(confirmed: boolean): void {
  if (!confirmed) {
    throw new Error(
      "Pi package mutation may execute npm/git lifecycle scripts; set confirmLifecycleScripts=true only after reviewing the exact package source",
    );
  }
}

function textFromPi(result: {
  readonly content?: ReadonlyArray<{ readonly type?: string; readonly text?: string }>;
}): string {
  return (result.content ?? [])
    .filter((item) => item.type === "text" && typeof item.text === "string")
    .map((item) => item.text ?? "")
    .join("\n");
}

function boundedPiResult(text: string): ToolResult<TextStructured> {
  const truncated = text.length > TEXT_MAX_CHARS;
  const value = truncated ? text.slice(0, TEXT_MAX_CHARS) : text;
  return textResult(
    truncated ? "Pi tool result returned with a bounded text prefix." : "Pi tool result returned.",
    { text: value, truncated, engine: "pi" },
  );
}

function piFailure(error: unknown): ToolResult<TextStructured, true> {
  const message = error instanceof Error ? error.message : String(error);
  return errorResult(message.slice(0, 2000), { text: "", truncated: false, engine: "pi" });
}

const textOutputSchema = schema.object({
  text: schema.string({ maxLength: TEXT_MAX_CHARS }),
  truncated: schema.boolean(),
  engine: schema.string({ enum: ["pi"] as const, maxLength: 16 }),
});

const piRead = defineTool({
  name: "pi_read",
  title: "Pi read",
  description:
    "Read project-relative text or an image through native Pi using a bounded file snapshot. Sensitive paths, traversal, device aliases, symbolic links/junctions and hardlinks are rejected. Files are limited to 8 MiB and returned images to 320 KiB; use canonical paged reads for larger files. This tool is read-only.",
  inputSchema: schema.object({
    path: schema.string({ maxLength: PATH_MAX_CHARS }),
    offset: schema.optional(schema.integer()),
    limit: schema.optional(schema.integer()),
  }),
  outputSchema: textOutputSchema,
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute({ path: inputPath, offset, limit }) {
    try {
      const checked = safeRelative(inputPath);
      const tool = createGuardedReadTool(process.cwd());
      const result = await tool.execute(
        randomUUID(),
        {
          path: checked ?? ".",
          ...(offset === undefined ? {} : { offset }),
          ...(limit === undefined ? {} : { limit }),
        },
        undefined,
        undefined,
      );
      const text = textFromPi(result);
      const image = result.content.find((block) => block.type === "image");
      if (image?.type === "image") {
        if (Buffer.byteLength(image.data, "base64") > 320 * 1024) {
          return errorResult("Pi image exceeds the bounded plugin image limit; resize it before reading", { text: "", truncated: false, engine: "pi" as const });
        }
        return pluginResult([
          { type: "text", text: text.slice(0, 4000) },
          { type: "image", data: image.data, mimeType: image.mimeType },
        ], { text: text.slice(0, 4000), truncated: text.length > 4000, engine: "pi" as const });
      }
      return boundedPiResult(text);
    } catch (error) {
      return piFailure(error);
    }
  },
});

const piGrep = defineTool({
  name: "pi_grep",
  title: "Pi grep",
  description:
    "Search project text with the guarded Pi ripgrep adapter. Sensitive paths and links are excluded, returned lines are reread through bounded file guards, context is capped at 20 lines and results at 1000 matches. No search-runtime downloads occur during tool calls.",
  inputSchema: schema.object({
    pattern: schema.string({ maxLength: 4000 }),
    path: schema.optional(schema.string({ maxLength: PATH_MAX_CHARS })),
    glob: schema.optional(schema.string({ maxLength: 1000 })),
    literal: schema.optional(schema.boolean()),
    ignoreCase: schema.optional(schema.boolean()),
    context: schema.optional(schema.integer()),
    limit: schema.optional(schema.integer()),
  }),
  outputSchema: textOutputSchema,
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute({ pattern, path: inputPath, glob, literal, ignoreCase, context, limit }) {
    try {
      const checked = safeRelative(inputPath);
      const tool = createGuardedGrepTool(process.cwd());
      const result = await tool.execute(
        randomUUID(),
        {
          pattern,
          ...(checked === undefined ? {} : { path: checked }),
          ...(glob === undefined ? {} : { glob }),
          ...(literal === undefined ? {} : { literal }),
          ...(ignoreCase === undefined ? {} : { ignoreCase }),
          ...(context === undefined ? {} : { context }),
          ...(limit === undefined ? {} : { limit }),
        },
        undefined,
        undefined,
      );
      return boundedPiResult(textFromPi(result));
    } catch (error) {
      return piFailure(error);
    }
  },
});

const piFind = defineTool({
  name: "pi_find",
  title: "Pi find",
  description:
    "Find project files using Pi's native result formatter and a guarded ripgrep enumeration backend. Ignore rules apply; sensitive paths, symlinks, junctions, and hard-linked files are omitted. Results are capped at 1000 files.",
  inputSchema: schema.object({
    pattern: schema.string({ maxLength: 1000 }),
    path: schema.optional(schema.string({ maxLength: PATH_MAX_CHARS })),
    limit: schema.optional(schema.integer()),
  }),
  outputSchema: textOutputSchema,
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute({ pattern, path: inputPath, limit }) {
    try {
      const checked = safeRelative(inputPath);
      const tool = createGuardedFindTool(process.cwd());
      const result = await tool.execute(
        randomUUID(),
        {
          pattern,
          ...(checked === undefined ? {} : { path: checked }),
          ...(limit === undefined ? {} : { limit }),
        },
        undefined,
        undefined,
      );
      return boundedPiResult(textFromPi(result));
    } catch (error) {
      return piFailure(error);
    }
  },
});

const piLs = defineTool({
  name: "pi_ls",
  title: "Pi list directory",
  description:
    "List a project-relative directory with Pi's ls tool. The configured provider cwd is the only root; sensitive and escaping paths are rejected. This tool is read-only.",
  inputSchema: schema.object({
    path: schema.optional(schema.string({ maxLength: PATH_MAX_CHARS })),
    limit: schema.optional(schema.integer()),
  }),
  outputSchema: textOutputSchema,
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute({ path: inputPath, limit }) {
    try {
      const checked = safeRelative(inputPath);
      const tool = createGuardedLsTool(process.cwd());
      const result = await tool.execute(
        randomUUID(),
        {
          ...(checked === undefined ? {} : { path: checked }),
          ...(limit === undefined ? {} : { limit }),
        },
        undefined,
        undefined,
      );
      return boundedPiResult(textFromPi(result));
    } catch (error) {
      return piFailure(error);
    }
  },
});

const extensionEntrySchema = schema.object({
  name: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
  kind: schema.string({ enum: ["file", "package"] as const, maxLength: 16 }),
  entrypoints: schema.array(schema.string({ maxLength: PATH_MAX_CHARS }), { maxItems: 64 }),
});

const piExtensionInventory = defineTool({
  name: "pi_extension_inventory",
  title: "Pi project extension inventory",
  description:
    "Discover only project-local .pi/extensions entries without executing them. It reads package metadata or conventional entrypoint names, never loads extension code, never inspects global Pi extensions, and never installs packages.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    extensions: schema.array(extensionEntrySchema, { maxItems: EXTENSION_LIMIT }),
    totalCount: schema.integer(),
    truncated: schema.boolean(),
    directoryPresent: schema.boolean(),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    const extensionRoot = path.join(process.cwd(), ".pi", "extensions");
    if (!existsSync(extensionRoot)) {
      const structured: ExtensionInventoryStructured = {
        extensions: [],
        totalCount: 0,
        truncated: false,
        directoryPresent: false,
      };
      return textResult("No project-local Pi extension directory is present.", structured);
    }

    try {
      const checkedRoot = await checkedProjectPath(process.cwd(), path.join(".pi", "extensions"));
      const dirents = [];
      for await (const entry of await opendir(checkedRoot)) {
        if (dirents.length >= 4096) throw new Error("extension discovery exceeds its directory-entry bound; narrow the installed resource set");
        dirents.push(entry);
      }
      const all: ExtensionEntry[] = [];
      for (const entry of dirents.sort((a, b) => a.name.localeCompare(b.name))) {
        if (entry.name.length > EXTENSION_NAME_MAX_CHARS) continue;
        try { await checkedProjectPath(process.cwd(), path.join(".pi", "extensions", entry.name)); }
        catch { continue; }
        if (entry.isFile() && /\.(?:ts|js)$/u.test(entry.name)) {
          all.push({ name: entry.name, kind: "file", entrypoints: [entry.name] });
          continue;
        }
        if (!entry.isDirectory()) continue;

        const directory = path.join(extensionRoot, entry.name);
        const manifestPath = path.join(directory, "package.json");
        let entrypoints: string[] = [];
        if (existsSync(manifestPath)) {
          try {
            const bytes = await readProjectFile(process.cwd(), path.join(".pi", "extensions", entry.name, "package.json"));
            if (bytes.length > 128 * 1024) throw new Error("extension manifest exceeds discovery bound");
            const manifest = JSON.parse(bytes.toString("utf8")) as {
              readonly pi?: { readonly extensions?: unknown };
            };
            const declared = manifest.pi?.extensions;
            if (Array.isArray(declared)) {
              entrypoints = declared
                .filter((value): value is string => typeof value === "string")
                .slice(0, 64)
                .filter((value) => value.length <= PATH_MAX_CHARS)
                .map((value) => safeRelative(value));
            }
          } catch {
            entrypoints = [];
          }
        }
        if (entrypoints.length === 0) {
          for (const conventional of ["index.ts", "index.js"]) {
            if (existsSync(path.join(directory, conventional))) entrypoints.push(conventional);
          }
        }
        all.push({ name: entry.name, kind: "package", entrypoints });
      }

      const truncated = all.length > EXTENSION_LIMIT;
      const structured: ExtensionInventoryStructured = {
        extensions: all.slice(0, EXTENSION_LIMIT),
        totalCount: all.length,
        truncated,
        directoryPresent: true,
      };
      return textResult(
        truncated
          ? "Project-local Pi extensions discovered; the returned inventory is bounded."
          : "Project-local Pi extensions discovered.",
        structured,
      );
    } catch (error) {
      const structured: ExtensionInventoryStructured = {
        extensions: [],
        totalCount: 0,
        truncated: false,
        directoryPresent: true,
      };
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        structured,
      );
    }
  },
});

const resourceErrorSchema = schema.object({
  path: schema.string({ maxLength: PATH_MAX_CHARS }),
  error: schema.string({ maxLength: 4000 }),
});

const skillSchema = schema.object({
  name: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
  description: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
  path: schema.string({ maxLength: PATH_MAX_CHARS }),
  disableModelInvocation: schema.boolean(),
});

const promptSchema = schema.object({
  name: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
  description: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
  path: schema.string({ maxLength: PATH_MAX_CHARS }),
});

const contextFileSchema = schema.object({
  path: schema.string({ maxLength: PATH_MAX_CHARS }),
  chars: schema.integer(),
});

const packageSchema = schema.object({
  source: schema.string({ maxLength: PATH_MAX_CHARS }),
  scope: schema.string({ enum: ["user", "project"] as const, maxLength: 16 }),
  filtered: schema.boolean(),
  installed: schema.boolean(),
});

const resourceInventorySchema = schema.object({
  projectTrusted: schema.boolean(),
  generation: schema.integer(),
  extensions: schema.object({
    candidates: schema.integer(),
    approved: schema.integer(),
    pending: schema.integer(),
    loaded: schema.integer(),
    errors: schema.array(resourceErrorSchema, { maxItems: 128 }),
  }),
  skills: schema.array(skillSchema, { maxItems: 256 }),
  prompts: schema.array(promptSchema, { maxItems: 256 }),
  contextFiles: schema.array(contextFileSchema, { maxItems: 128 }),
  themes: schema.array(
    schema.object({ name: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }) }),
    { maxItems: 128 },
  ),
  packages: schema.array(packageSchema, { maxItems: 256 }),
});

const extensionCandidateSchema = schema.object({
  candidateId: schema.string({ maxLength: 64 }),
  displayPath: schema.string({ maxLength: PATH_MAX_CHARS }),
  scope: schema.string({ enum: ["user", "project", "temporary"] as const, maxLength: 16 }),
  sha256: schema.string({ maxLength: 64 }),
  approved: schema.boolean(),
  executable: schema.boolean(),
  nextAction: schema.string({
    enum: ["ready", "trust_then_approve", "approve_exact_fingerprint", "fix_candidate"] as const,
    maxLength: 64,
  }),
  reason: schema.optional(schema.string({ maxLength: 4000 })),
});

const piExtensionCandidateStatus = defineTool({
  name: "pi_extension_candidate_status",
  title: "Pi extension candidate status",
  description:
    "Refresh the native Pi package/extension candidate set without importing unapproved code, returning exact candidate ids and SHA-256 fingerprints for WebPi approval.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    projectTrusted: schema.boolean(),
    generation: schema.integer(),
    candidates: schema.array(extensionCandidateSchema, { maxItems: 256 }),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    try {
      const host = await runtimeHost();
      return textResult("Pi extension candidate status returned.", {
        projectTrusted: host.isProjectTrusted(),
        generation: host.generation,
        candidates: await host.extensionCandidates(),
      });
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        { projectTrusted: false, generation: 0, candidates: [] as ExtensionCandidateSummary[] },
      );
    }
  },
});

const piExtensionApprove = defineTool({
  name: "pi_extension_approve",
  title: "Approve exact Pi extension fingerprint",
  description:
    "Persist approval for one currently resolved Pi extension candidate only when candidateId and SHA-256 still match. This does not import the code; call pi_resource_reload afterwards to activate it.",
  inputSchema: schema.object({
    candidateId: schema.string({ minLength: 1, maxLength: 64 }),
    sha256: schema.string({ minLength: 64, maxLength: 64 }),
  }),
  outputSchema: schema.object({
    candidate: extensionCandidateSchema,
    reloadRequired: schema.boolean(),
  }),
  annotations: {
    readOnlyHint: false,
    destructiveHint: true,
    idempotentHint: false,
    openWorldHint: false,
  },
  async execute({ candidateId, sha256 }) {
    try {
      const candidate = await (await runtimeHost()).approveExtensionCandidate(candidateId, sha256);
      return textResult("Exact Pi extension fingerprint approved; reload is required before execution.", {
        candidate,
        reloadRequired: true,
      });
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        {
          candidate: {
            candidateId,
            displayPath: "",
            scope: "project" as const,
            sha256,
            approved: false,
            executable: false,
            nextAction: "approve_exact_fingerprint" as const,
          },
          reloadRequired: false,
        },
      );
    }
  },
});

const piExtensionRevoke = defineTool({
  name: "pi_extension_revoke",
  title: "Revoke Pi extension approval",
  description:
    "Revoke one resolved Pi extension candidate and immediately reload Pi resources so previously loaded extension code is invalidated and unloaded.",
  inputSchema: schema.object({
    candidateId: schema.string({ minLength: 1, maxLength: 64 }),
  }),
  outputSchema: resourceInventorySchema,
  annotations: {
    readOnlyHint: false,
    destructiveHint: true,
    idempotentHint: false,
    openWorldHint: false,
  },
  async execute({ candidateId }) {
    try {
      return textResult("Pi extension approval revoked and resources reloaded.", await (await runtimeHost()).revokeExtensionCandidate(candidateId));
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        emptyResourceInventory(),
      );
    }
  },
});

const extensionToolSummarySchema = schema.object({
  name: schema.string({ maxLength: TOOL_NAME_MAX_CHARS }),
  label: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
  description: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
  source: schema.string({ maxLength: PATH_MAX_CHARS }),
});

const piResourceInventory = defineTool({
  name: "pi_resource_inventory",
  title: "Pi resource inventory",
  description:
    "Load Pi resources using Pi's native ResourceLoader and project trust store, then report extensions, skills, prompts, context files, themes, and configured packages without exposing absolute host paths.",
  inputSchema: schema.object({}),
  outputSchema: resourceInventorySchema,
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    try {
      return textResult("Pi resource inventory returned.", (await runtimeHost()).inventory());
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        emptyResourceInventory(),
      );
    }
  },
});

const piExtensionToolList = defineTool({
  name: "pi_extension_tool_list",
  title: "Pi extension tool list",
  description:
    "List tools registered by the currently loaded and trusted Pi extensions. This is the native Pi extension runtime catalog, not a filesystem guess.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    generation: schema.integer(),
    tools: schema.array(extensionToolSummarySchema, { maxItems: 512 }),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    try {
      const host = await runtimeHost();
      return textResult("Pi extension tool catalog returned.", {
        generation: host.generation,
        tools: host.listExtensionTools(),
      });
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        {
          generation: 0,
          tools: [] as Array<{
            name: string;
            label: string;
            description: string;
            source: string;
          }>,
        },
      );
    }
  },
});

const piExtensionToolDescribe = defineTool({
  name: "pi_extension_tool_describe",
  title: "Describe Pi extension tool arguments",
  description: "Return one currently approved Pi extension tool's exact TypeBox/JSON Schema as parametersJson, active state, source and runtime generation. Parse parametersJson before calling the tool; do not guess its inputs. Approval is revalidated before this description and before execution.",
  inputSchema: schema.object({ tool: schema.string({ minLength: 1, maxLength: TOOL_NAME_MAX_CHARS }) }),
  outputSchema: schema.object({
    name: schema.string({ maxLength: TOOL_NAME_MAX_CHARS }),
    label: schema.string({ maxLength: 512 }),
    description: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
    source: schema.string({ maxLength: PATH_MAX_CHARS }),
    parametersJson: schema.string({ maxLength: 64 * 1024 }),
    active: schema.boolean(),
    generation: schema.integer(),
  }),
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  async execute({ tool }) {
    try { return textResult("Pi extension argument schema returned.", await (await runtimeHost()).describeExtensionTool(tool)); }
    catch (error) {
      return errorResult(error instanceof Error ? error.message.slice(0, 2000) : "Pi description failed", {
        name: tool, label: "", description: "", source: "", parametersJson: "{}", active: false, generation: 0,
      });
    }
  },
});

const piExtensionToolCall = defineTool({
  name: "pi_extension_tool_call",
  title: "Call Pi extension tool",
  description:
    "Invoke one tool registered by a trusted Pi extension through Pi's native tool_call and tool_result hook chain. Extension code runs with Pi's normal local extension authority.",
  inputSchema: schema.object({
    tool: schema.string({ minLength: 1, maxLength: TOOL_NAME_MAX_CHARS }),
    arguments: schema.object({}, { additionalProperties: true }),
  }),
  outputSchema: schema.object({
    tool: schema.string({ maxLength: TOOL_NAME_MAX_CHARS }),
    engine: schema.string({ enum: ["pi-extension"] as const, maxLength: 32 }),
    text: schema.string({ maxLength: TEXT_MAX_CHARS }),
    isError: schema.boolean(),
    contentTypes: schema.array(schema.string({ maxLength: 64 }), { maxItems: 64 }),
    detailsJson: schema.optional(schema.string({ maxLength: DETAILS_JSON_MAX_CHARS })),
    generation: schema.integer(),
  }),
  annotations: {
    readOnlyHint: false,
    destructiveHint: true,
    idempotentHint: false,
    openWorldHint: false,
  },
  async execute({ tool, arguments: args }) {
    try {
      const result = await (await runtimeHost()).callExtensionTool(tool, args);
      const { detailsJson, content, ...base } = result;
      const structured = detailsJson === null ? base : { ...base, detailsJson };
      return pluginResult(content, structured, result.isError);
    } catch (error) {
      return errorResult(error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000), {
        tool,
        engine: "pi-extension" as const,
        text: "",
        isError: true,
        contentTypes: [] as string[],
        generation: 0,
      });
    }
  },
});

const piResourceReload = defineTool({
  name: "pi_resource_reload",
  title: "Reload Pi resources",
  description:
    "Reload Pi extensions, skills, prompts, context files, themes, and package resource configuration using Pi's native ResourceLoader while preserving WebPi's project-trust boundary.",
  inputSchema: schema.object({}),
  outputSchema: resourceInventorySchema,
  annotations: {
    readOnlyHint: false,
    destructiveHint: false,
    idempotentHint: false,
    openWorldHint: false,
  },
  async execute() {
    try {
      return textResult("Pi resources reloaded.", await (await runtimeHost()).reload());
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        emptyResourceInventory(),
      );
    }
  },
});

const piPackageList = defineTool({
  name: "pi_package_list",
  title: "Pi package list",
  description:
    "List Pi packages configured for the WebPi-local Pi agent directory and current project. Native package mutation is available through the separate consequential pi_package_install/update/remove tools.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    packages: schema.array(packageSchema, { maxItems: 256 }),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    try {
      return textResult("Configured Pi packages returned.", {
        packages: (await runtimeHost()).inventory().packages,
      });
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        {
          packages: [] as Array<{
            source: string;
            scope: "user" | "project";
            filtered: boolean;
            installed: boolean;
          }>,
        },
      );
    }
  },
});

const packageScopeSchema = schema.string({ enum: ["user", "project"] as const, maxLength: 16 });

const piPackageInstall = defineTool({
  name: "pi_package_install",
  title: "Install native Pi package",
  description:
    "Install and persist a Pi package with Pi's native DefaultPackageManager, then reload resources. npm/git package installation may execute lifecycle scripts. New executable extensions remain blocked until their exact fingerprint is approved.",
  inputSchema: schema.object({
    source: schema.string({ minLength: 1, maxLength: PATH_MAX_CHARS }),
    scope: packageScopeSchema,
    confirmLifecycleScripts: schema.boolean(),
  }),
  outputSchema: resourceInventorySchema,
  annotations: {
    readOnlyHint: false,
    destructiveHint: true,
    idempotentHint: false,
    openWorldHint: true,
  },
  async execute({ source, scope, confirmLifecycleScripts }) {
    try {
      requireLifecycleConfirmation(confirmLifecycleScripts);
      return textResult(
        "Native Pi package installed and resources reloaded.",
        await (await runtimeHost()).installPackage(safePackageSource(source), scope),
      );
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        emptyResourceInventory(),
      );
    }
  },
});

const piPackageUpdate = defineTool({
  name: "pi_package_update",
  title: "Update native Pi package",
  description:
    "Update one configured Pi package, or all configured packages when source is omitted, using Pi's native DefaultPackageManager. Updates may execute lifecycle scripts; changed extension fingerprints are automatically returned to pending approval on reload.",
  inputSchema: schema.object({
    source: schema.optional(schema.string({ minLength: 1, maxLength: PATH_MAX_CHARS })),
    confirmLifecycleScripts: schema.boolean(),
  }),
  outputSchema: resourceInventorySchema,
  annotations: {
    readOnlyHint: false,
    destructiveHint: true,
    idempotentHint: false,
    openWorldHint: true,
  },
  async execute({ source, confirmLifecycleScripts }) {
    try {
      requireLifecycleConfirmation(confirmLifecycleScripts);
      return textResult(
        "Native Pi package update completed and resources reloaded.",
        await (await runtimeHost()).updatePackage(source === undefined ? undefined : safePackageSource(source)),
      );
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        emptyResourceInventory(),
      );
    }
  },
});

const piPackageRemove = defineTool({
  name: "pi_package_remove",
  title: "Remove native Pi package",
  description:
    "Remove and unconfigure a Pi package with Pi's native DefaultPackageManager, then reload resources. Package removal may execute lifecycle scripts.",
  inputSchema: schema.object({
    source: schema.string({ minLength: 1, maxLength: PATH_MAX_CHARS }),
    scope: packageScopeSchema,
    confirmLifecycleScripts: schema.boolean(),
  }),
  outputSchema: resourceInventorySchema,
  annotations: {
    readOnlyHint: false,
    destructiveHint: true,
    idempotentHint: false,
    openWorldHint: true,
  },
  async execute({ source, scope, confirmLifecycleScripts }) {
    try {
      requireLifecycleConfirmation(confirmLifecycleScripts);
      return textResult(
        "Native Pi package removed and resources reloaded.",
        await (await runtimeHost()).removePackage(safePackageSource(source), scope),
      );
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        emptyResourceInventory(),
      );
    }
  },
});

const piSkillRead = defineTool({
  name: "pi_skill_read",
  title: "Read Pi skill",
  description:
    "Read one currently loaded Pi skill by name using Pi's native resource catalog. Returns bounded SKILL.md content and project-relative metadata.",
  inputSchema: schema.object({
    name: schema.string({ minLength: 1, maxLength: EXTENSION_NAME_MAX_CHARS }),
  }),
  outputSchema: schema.object({
    name: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
    description: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
    path: schema.string({ maxLength: PATH_MAX_CHARS }),
    content: schema.string({ maxLength: 128 * 1024 }),
    disableModelInvocation: schema.boolean(),
    generation: schema.integer(),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute({ name }) {
    try {
      return textResult("Pi skill returned.", await (await runtimeHost()).readSkill(name));
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        {
          name,
          description: "",
          path: "",
          content: "",
          disableModelInvocation: false,
          generation: 0,
        },
      );
    }
  },
});

const piPromptExpand = defineTool({
  name: "pi_prompt_expand",
  title: "Expand Pi prompt",
  description:
    "Expand a currently loaded Pi prompt template with Pi's native positional/default/slice substitution semantics.",
  inputSchema: schema.object({
    name: schema.string({ minLength: 1, maxLength: EXTENSION_NAME_MAX_CHARS }),
    arguments: schema.array(schema.string({ maxLength: 8192 }), { maxItems: 64 }),
  }),
  outputSchema: schema.object({
    name: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
    description: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
    expanded: schema.string({ maxLength: 128 * 1024 }),
    generation: schema.integer(),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute({ name, arguments: args }) {
    try {
      return textResult("Pi prompt expanded.", await (await runtimeHost()).expandPrompt(name, args));
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        { name, description: "", expanded: "", generation: 0 },
      );
    }
  },
});

const piContextSnapshot = defineTool({
  name: "pi_context_snapshot",
  title: "Pi context snapshot",
  description:
    "Return the currently loaded Pi AGENTS/context files and system-prompt overlays so the web model can consume the same project guidance Pi would load.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    generation: schema.integer(),
    contextFiles: schema.array(
      schema.object({
        path: schema.string({ maxLength: PATH_MAX_CHARS }),
        content: schema.string({ maxLength: 128 * 1024 }),
      }),
      { maxItems: 128 },
    ),
    systemPrompt: schema.string({ maxLength: 128 * 1024 }),
    appendSystemPrompt: schema.array(schema.string({ maxLength: 128 * 1024 }), { maxItems: 128 }),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    try {
      return textResult("Pi context snapshot returned.", (await runtimeHost()).contextSnapshot());
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        {
          generation: 0,
          contextFiles: [] as Array<{ path: string; content: string }>,
          systemPrompt: "",
          appendSystemPrompt: [] as string[],
        },
      );
    }
  },
});

const commandSummarySchema = schema.object({
  name: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
  description: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
  source: schema.string({ maxLength: PATH_MAX_CHARS }),
});

const piExtensionCommandList = defineTool({
  name: "pi_extension_command_list",
  title: "Pi extension command list",
  description:
    "List slash commands registered by loaded trusted Pi extensions. Commands that depend on terminal UI may not be useful over WebPi.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    generation: schema.integer(),
    commands: schema.array(commandSummarySchema, { maxItems: 512 }),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    try {
      const host = await runtimeHost();
      return textResult("Pi extension commands returned.", {
        generation: host.generation,
        commands: host.listCommands(),
      });
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        {
          generation: 0,
          commands: [] as Array<{ name: string; description: string; source: string }>,
        },
      );
    }
  },
});

const piExtensionCommandCall = defineTool({
  name: "pi_extension_command_call",
  title: "Call Pi extension command",
  description:
    "Invoke a loaded trusted Pi extension slash command in WebPi's print/no-TUI command context.",
  inputSchema: schema.object({
    command: schema.string({ minLength: 1, maxLength: EXTENSION_NAME_MAX_CHARS }),
    arguments: schema.string({ maxLength: 64 * 1024 }),
  }),
  outputSchema: schema.object({
    command: schema.string({ maxLength: EXTENSION_NAME_MAX_CHARS }),
    completed: schema.boolean(),
    generation: schema.integer(),
  }),
  annotations: {
    readOnlyHint: false,
    destructiveHint: true,
    idempotentHint: false,
    openWorldHint: false,
  },
  async execute({ command, arguments: args }) {
    try {
      return textResult("Pi extension command completed.", await (await runtimeHost()).callCommand(command, args));
    } catch (error) {
      return errorResult(
        error instanceof Error ? error.message.slice(0, 2000) : String(error).slice(0, 2000),
        { command, completed: false, generation: 0 },
      );
    }
  },
});

const parityStatusSchema = schema.string({
  enum: ["native", "equivalent", "limited", "not_applicable"] as const,
  maxLength: 32,
});

const capabilityEntrySchema = schema.object({
  capability: schema.string({ maxLength: 128 }),
  status: parityStatusSchema,
  notes: schema.string({ maxLength: DESCRIPTION_MAX_CHARS }),
});

const PI_PARITY_CAPABILITIES = [
  {
    capability: "extensions",
    status: "native",
    notes:
      "Pi 0.85.x DefaultResourceLoader/ExtensionRunner load approved extensions, tools, commands, tool hooks, startup/reload/shutdown lifecycle, resources_discover, TypeBox schemas, images, and persistent extension entries.",
  },
  {
    capability: "skills_prompts_context",
    status: "native",
    notes:
      "Pi native skills, prompt-template substitution, AGENTS/context files, SYSTEM/APPEND overlays, and extension-discovered resources are loaded through Pi's own ResourceLoader.",
  },
  {
    capability: "packages",
    status: "native",
    notes:
      "WebPi delegates web-agent and local-admin install/list/update/remove plus package resource resolution to Pi's DefaultPackageManager. Web package mutation requires explicit lifecycle-script confirmation; executable extension import additionally requires exact WebPi fingerprint approval."
  },
  {
    capability: "read_images",
    status: "native",
    notes: "Native Pi read and image detection use a bounded guarded file snapshot. Project paths, links, sensitive state and image wire-size limits are enforced before results are published.",
  },
  {
    capability: "grep_find_ls",
    status: "equivalent",
    notes: "Guarded ripgrep enumeration/search and Pi find/ls formatting exclude sensitive paths, links and hardlinks. Search resolves an absolute reviewed executable, uses bounded output/deadlines and rereads matched files through file guards rather than publishing subprocess line bytes.",
  },
  {
    capability: "extension_session_state",
    status: "equivalent",
    notes:
      "Extensions receive a native Pi SessionManager. Because WebPi has no second Pi assistant loop, WebPi atomically snapshots FileEntry state under its own .webpi-state so appendEntry/session metadata survive plugin restarts.",
  },
  {
    capability: "guarded_mutation_process_git",
    status: "equivalent",
    notes:
      "WebPi deliberately keeps edits, process execution, Jobs, Git, stale-write fences, and recovery on its hardened canonical runtime instead of duplicating Pi bash/edit/write authority.",
  },
  {
    capability: "workflow_sessions",
    status: "equivalent",
    notes:
      "WebPi Workflow Sessions own the web agent's coding continuity; Pi SessionManager is retained as the extension-local state API rather than a second LLM conversation.",
  },
  {
    capability: "tool_events",
    status: "limited",
    notes:
      "Native Pi tool_call/tool_result/tool_execution lifecycle is emitted for Pi extension-tool calls. WebPi canonical edit/process/Git tools remain outside Pi's event bus by design.",
  },
  {
    capability: "session_tree_control",
    status: "limited",
    notes:
      "Extension-local state/session inspection and reload are supported. Pi new/fork/tree/switch controls cannot replace the authoritative WebPi Workflow Session and are not presented as successful WebPi session operations.",
  },
  {
    capability: "provider_model_events",
    status: "not_applicable",
    notes:
      "The primary model runs in ChatGPT web, not inside Pi ModelRuntime. Pi provider request hooks, model switching, thinking-level control, and provider-owned OAuth do not govern the web model.",
  },
  {
    capability: "agent_turn_message_events",
    status: "not_applicable",
    notes:
      "Pi agent/turn/message/input events require Pi to own the LLM loop. WebPi intentionally has no second Pi model loop; ChatGPT web remains the sole reasoning agent.",
  },
  {
    capability: "tui_ui",
    status: "not_applicable",
    notes:
      "Pi terminal renderers, widgets, keybindings, editor components, and TUI dialogs have no terminal UI surface in the web harness. Resource metadata may load, but TUI behavior is not emulated.",
  },
] as const;

const piCapabilityReport = defineTool({
  name: "pi_capability_report",
  title: "Pi parity capability report",
  description:
    "Report how WebPi maps the pinned native Pi coding-agent capability surface: native reuse, WebPi-equivalent hardened behavior, deliberately limited integration, or features not applicable to a web-GPT-primary architecture.",
  inputSchema: schema.object({}),
  outputSchema: schema.object({
    piVersion: schema.string({ maxLength: 64 }),
    architecture: schema.string({
      enum: ["web-gpt-primary-no-second-pi-model-loop"] as const,
      maxLength: 64,
    }),
    capabilities: schema.array(capabilityEntrySchema, { maxItems: 64 }),
  }),
  annotations: {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  },
  async execute() {
    return textResult("WebPi Pi capability parity report returned.", {
      piVersion: PI_VERSION,
      architecture: "web-gpt-primary-no-second-pi-model-loop" as const,
      capabilities: PI_PARITY_CAPABILITIES.map((entry) => ({ ...entry })),
    });
  },
});

runPlugin(
  definePlugin({
    tools: [
      piRead,
      piGrep,
      piFind,
      piLs,
      piExtensionInventory,
      piResourceInventory,
      piExtensionCandidateStatus,
      piExtensionApprove,
      piExtensionRevoke,
      piExtensionToolList,
      piExtensionToolDescribe,
      piExtensionToolCall,
      piResourceReload,
      piPackageList,
      piPackageInstall,
      piPackageUpdate,
      piPackageRemove,
      piSkillRead,
      piPromptExpand,
      piContextSnapshot,
      piExtensionCommandList,
      piExtensionCommandCall,
      piCapabilityReport,
    ],
  }),
  {
    async onClose() {
      if (runtimeHostPromise !== undefined) {
        await (await runtimeHostPromise).shutdown();
      }
    },
  },
);
