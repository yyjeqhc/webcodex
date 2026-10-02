import { createHash } from "node:crypto";

export class BridgeError extends Error {
  constructor(code, message = code) { super(message); this.code = code; }
}

export function requireCondition(condition, code) {
  if (!condition) throw new BridgeError(code);
}

export const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
export const isId = (value) => typeof value === "string" && /^[A-Za-z0-9_.:-]{1,128}$/.test(value);
// Native names are not WebCodex global tool identifiers. Preserve their spelling.
export const isNativeName = (value) => typeof value === "string" && value.trim().length > 0 && value.length <= 256 && Buffer.byteLength(value) <= 512 && !/[\u0000-\u001f\u007f]/.test(value);

// Hash only data, never executable tool definitions or source/environment objects.
export function canonical(value, depth = 0) {
  requireCondition(depth < 48, "native_catalog_invalid");
  if (Array.isArray(value)) return value.map((item) => canonical(item, depth + 1));
  if (isObject(value)) return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key], depth + 1)]));
  requireCondition(value === null || ["string", "boolean", "number"].includes(typeof value), "native_catalog_invalid");
  return value;
}

export function digest(value) {
  return createHash("sha256").update(JSON.stringify(canonical(value))).digest("base64url");
}

export function fits(value, bytes, nodes = 2800, depth = 10) {
  try {
    let remaining = nodes;
    const walk = (item, level) => {
      if (--remaining < 0 || level > depth) throw new Error("bounds");
      if (Array.isArray(item)) item.forEach((child) => walk(child, level + 1));
      else if (isObject(item)) Object.values(item).forEach((child) => walk(child, level + 1));
    };
    walk(value, 0);
    return Buffer.byteLength(JSON.stringify(value)) <= bytes;
  } catch { return false; }
}

export function nativeResult(result) {
  // A completed native result stays completed even when rich output is too large.
  const value = { content: result.content ?? [], ...(result.details !== undefined ? { details: result.details } : {}), ...(result.structuredContent !== undefined ? { structuredContent: result.structuredContent } : {}) };
  if (fits(value, 12 * 1024, 128, 6)) return JSON.parse(JSON.stringify(value));
  const firstText = Array.isArray(result.content) ? result.content.find((part) => part.type === "text" && typeof part.text === "string")?.text : undefined;
  return { content: firstText === undefined ? [] : [{ type: "text", text: firstText.slice(0, 2048) }], omitted: true, reason: "native_result_bounds_exceeded" };
}

export class Catalog {
  constructor(tools, salt) {
    requireCondition(Array.isArray(tools), "native_catalog_invalid");
    this.salt = salt;
    const names = new Set();
    this.entries = tools.map((tool) => {
      requireCondition(isNativeName(tool.name) && !names.has(tool.name), "native_catalog_duplicate_or_invalid_name");
      names.add(tool.name);
      requireCondition(["direct", "model-only", "codemode", "deferred", "hidden"].includes(tool.exposure), "native_exposure_invalid");
      requireCondition(isObject(tool.parameters), "native_catalog_invalid");
      return {
        name: tool.name,
        description: String(tool.description ?? ""),
        parameters: canonical(tool.parameters),
        exposure: tool.exposure,
        declared: tool.declared === true && tool.exposure !== "hidden",
        callable: tool.callable === true && tool.exposure !== "hidden" && tool.exposure !== "model-only",
        ...(tool.namespace ? { namespace: String(tool.namespace.name) } : {}),
        ...(tool.annotations ? { annotations: canonical(tool.annotations) } : {}),
      };
    }).sort((a, b) => a.name.localeCompare(b.name, "en"));
    const hash = createHash("sha256").update(salt);
    for (const entry of this.entries) hash.update(JSON.stringify(canonical(entry))).update("\0");
    this.revision = `cat_${hash.digest("base64url")}`;
    this.byName = new Map(this.entries.map((tool) => [tool.name, tool]));
  }

  visible(tool, searching = false) {
    if (tool.exposure === "hidden") return false;
    if (tool.declared) return true;
    if (!tool.callable) return false;
    return tool.exposure !== "deferred" || searching;
  }

  binding(tool) { return `schema_${digest([this.salt, tool.name, tool.parameters, tool.exposure, tool.declared, tool.callable, tool.annotations ?? null])}`; }

  page({ query = "", cursor, limit = 20 }) {
    requireCondition(typeof query === "string" && query.length <= 128, "invalid_query");
    requireCondition(Number.isInteger(limit), "invalid_limit");
    limit = Math.max(1, Math.min(32, limit));
    const search = query.trim().toLowerCase();
    const pageIdentity = digest([this.revision, search]);
    let offset = 0;
    if (cursor !== undefined) {
      requireCondition(typeof cursor === "string" && cursor.length <= 128, "invalid_cursor");
      const [identity, position, extra] = cursor.split(".");
      requireCondition(identity === pageIdentity && /^\d{1,10}$/.test(position ?? "") && extra === undefined, "stale_cursor");
      offset = Number(position);
    }
    const visible = this.entries.filter((tool) => this.visible(tool, Boolean(search)) && (!search || `${tool.name} ${tool.description}`.toLowerCase().includes(search)));
    requireCondition(offset <= visible.length, "invalid_cursor");
    const tools = visible.slice(offset, offset + limit).map((tool) => ({
      name: tool.name, description: tool.description.slice(0, 512), exposure: tool.exposure,
      access: tool.declared ? "model_call" : "native_orchestrator_only",
      ...(tool.namespace ? { namespace: tool.namespace.slice(0, 128) } : {}),
    }));
    return { catalog: this.revision, tools, total: visible.length, ...(offset + tools.length < visible.length ? { next_cursor: `${pageIdentity}.${offset + tools.length}` } : {}), discovery: "Deferred tools appear on search. Native-orchestrator-only tools must be reached through this environment's native codemode/tool-search tools, not direct bridge calls." };
  }

  describe(name) {
    const tool = this.byName.get(name);
    requireCondition(tool && this.visible(tool, true), "native_tool_unavailable");
    const value = { name: tool.name, description: tool.description.slice(0, 4096), ...(tool.description.length > 4096 ? { description_truncated: true } : {}), inputSchema: tool.parameters, exposure: tool.exposure, access: tool.declared ? "model_call" : "native_orchestrator_only", binding: this.binding(tool) };
    requireCondition(fits(value, 48 * 1024), "native_schema_bounds_exceeded");
    return value;
  }

  admit(calls) {
    requireCondition(Array.isArray(calls) && calls.length >= 1 && calls.length <= 16, "invalid_batch");
    const ids = new Set();
    return calls.map((call) => {
      requireCondition(isObject(call) && Object.keys(call).every((key) => ["id", "tool", "binding", "arguments"].includes(key)), "invalid_call");
      requireCondition(isId(call.id) && !ids.has(call.id), "duplicate_or_invalid_call_id");
      ids.add(call.id);
      const tool = this.byName.get(call.tool);
      requireCondition(tool && tool.declared && tool.exposure !== "hidden", "native_tool_not_declared");
      requireCondition(call.binding === this.binding(tool), "native_schema_changed");
      requireCondition(isObject(call.arguments) && fits(call.arguments, 48 * 1024, 2800, 10), "invalid_native_arguments");
      return { id: call.id, name: tool.name, arguments: call.arguments, parameters: tool.parameters };
    });
  }
}
