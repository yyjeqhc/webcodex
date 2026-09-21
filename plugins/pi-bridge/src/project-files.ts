import { spawn } from "node:child_process";
import { constants } from "node:fs";
import { access, lstat, open, opendir, realpath } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createFindTool, createLsTool, createReadTool } from "@earendil-works/pi-coding-agent";

const MAX_FILE_BYTES = 8 * 1024 * 1024;
const MAX_SEARCH_BYTES = 2 * 1024 * 1024;
const MAX_CONTEXT_BYTES = 32 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES = 20_000;

function sensitiveComponent(value: string): boolean {
  const name = value.toLowerCase();
  return /^(?:\.git|\.webpi-state|\.webpi-runtime|\.ssh|\.aws|\.azure|\.npmrc|\.pypirc|auth\.json|runner\.toml|agent\.toml|webpi\.env|webcodex\.env)$/u.test(name)
    || /^\.env(?:$|\.)/u.test(name)
    || /^(?:credentials?|secrets?|tokens?)(?:$|\.)/u.test(name)
    || /(?:\.local\.toml|\.pem|\.key|\.p12|\.pfx)$/u.test(name);
}

/** Portable project paths only. Windows aliases/ADS must not bypass the same policy. */
export function safeRelative(value: string | undefined): string {
  if (value === undefined || value === "" || value === ".") return ".";
  if (path.isAbsolute(value) || path.win32.isAbsolute(value) || /^[~\\/]/u.test(value)
      || /[:\x00-\x1f\x7f]/u.test(value)) {
    throw new Error("path must be project-relative without device names or alternate streams");
  }
  const parts = value.replaceAll("\\", "/").split("/").filter((part) => part !== "" && part !== ".");
  if (parts.includes("..")) throw new Error("path escapes the configured project root");
  if (parts.some((part) => /[. ]$/u.test(part) || /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/iu.test(part))) {
    throw new Error("ambiguous filesystem path is not exposed through pi-bridge");
  }
  if (parts.some(sensitiveComponent)) throw new Error("sensitive project path is not exposed through pi-bridge");
  return parts.length ? parts.join(path.sep) : ".";
}

function relativeInside(root: string, absolute: string): string {
  const relative = path.relative(root, absolute);
  if (path.isAbsolute(relative) || relative === ".." || relative.startsWith(".." + path.sep)) {
    throw new Error("path escapes the configured project root");
  }
  return safeRelative(relative);
}

/** Recheck lexical AND physical paths; never follow a symlink/junction under the root. */
export async function checkedProjectPath(root: string, relative: string | undefined): Promise<string> {
  const canonicalRoot = await realpath(path.resolve(root));
  const safe = safeRelative(relative);
  let current = canonicalRoot;
  if (safe !== ".") {
    for (const component of safe.split(path.sep)) {
      current = path.join(current, component);
      const info = await lstat(current);
      if (info.isSymbolicLink()) throw new Error("symlink/junction paths are not exposed through pi-bridge");
      if (!info.isDirectory() && !info.isFile()) throw new Error("special files are not exposed through pi-bridge");
    }
  }
  const physical = await realpath(current);
  relativeInside(canonicalRoot, physical);
  return physical;
}

/** Read a bounded, regular, single-link file through one handle, rejecting observed races. */
export async function readProjectFile(root: string, relative: string): Promise<Buffer> {
  const absolute = await checkedProjectPath(root, relative);
  const handle = await open(absolute, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const before = await handle.stat();
    if (!before.isFile() || before.nlink !== 1) throw new Error("only regular single-link files are exposed through pi-bridge");
    if (before.size > MAX_FILE_BYTES) throw new Error("file exceeds the bounded Pi read limit (8 MiB); use canonical paged file tools");
    const current = await lstat(await checkedProjectPath(root, relative));
    if (before.dev !== current.dev || before.ino !== current.ino) throw new Error("file changed during guarded open");
    const buffer = Buffer.alloc(before.size + 1);
    let length = 0;
    while (length < buffer.length) {
      const { bytesRead } = await handle.read(buffer, length, buffer.length - length, length);
      if (!bytesRead) break;
      length += bytesRead;
    }
    const after = await handle.stat();
    const finalPath = await lstat(await checkedProjectPath(root, relative));
    if (length !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs
        || after.ctimeMs !== before.ctimeMs || after.nlink !== 1
        || finalPath.dev !== before.dev || finalPath.ino !== before.ino) {
      throw new Error("file changed during guarded read; read again");
    }
    return buffer.subarray(0, length);
  } finally {
    await handle.close();
  }
}

const piDist = path.dirname(fileURLToPath(import.meta.resolve("@earendil-works/pi-coding-agent")));
async function piHelper(name: string): Promise<any> {
  return import(pathToFileURL(path.join(piDist, "utils", name + ".js")).href);
}

export function createGuardedReadTool(root: string) {
  let snapshot: Promise<Buffer> | undefined;
  let boundPath: string | undefined;
  const read = (absolute: string) => {
    const relative = relativeInside(path.resolve(root), path.resolve(absolute));
    if (boundPath !== undefined && relative !== boundPath) throw new Error("read snapshot path changed");
    boundPath = relative;
    return snapshot ??= readProjectFile(root, relative);
  };
  return createReadTool(root, {
    operations: {
      access: async (absolute) => { await read(absolute); },
      readFile: read,
      detectImageMimeType: async (absolute) => {
        const bytes = await read(absolute);
        return (await piHelper("mime")).detectSupportedImageMimeType(bytes) as string | null;
      },
    },
  });
}

// Positive user globs are followed by non-overridable sensitive-path exclusions.
// Returned files are checked again, so casing, link aliases and changing paths fail closed.
const EXCLUDED = [".git", ".webpi-state", ".webpi-runtime", ".ssh", ".aws", ".azure", "node_modules", "target",
  ".env", ".env.*", ".npmrc", ".pypirc", "auth.json", "runner.toml", "agent.toml", "webpi.env", "webcodex.env",
  "credential", "credentials", "credential.*", "credentials.*", "secret", "secrets", "secret.*", "secrets.*",
  "token", "tokens", "token.*", "tokens.*", "*.local.toml", "*.pem", "*.key", "*.p12", "*.pfx"];

export async function resolveSearchBinary(root: string): Promise<string> {
  const configured = process.env.WEBPI_RG_BIN;
  const candidates: string[] = [];
  if (configured !== undefined) {
    if (!path.isAbsolute(configured)) throw new Error("WEBPI_RG_BIN must be an absolute deployment-owned executable path");
    candidates.push(configured);
  } else {
    // Do not call Pi's getToolPath(): its command probe can search an untrusted
    // project cwd on Windows. Resolve an absolute executable without executing it.
    const config = await import(pathToFileURL(path.join(piDist, "config.js")).href);
    candidates.push(path.join(config.getBinDir(), process.platform === "win32" ? "rg.exe" : "rg"));
    for (const directory of (process.env.PATH ?? "").split(path.delimiter)) {
      if (!path.isAbsolute(directory)) continue;
      candidates.push(path.join(directory, process.platform === "win32" ? "rg.exe" : "rg"));
    }
  }
  const canonicalRoot = await realpath(root);
  for (const candidate of candidates) {
    try {
      const resolved = await realpath(candidate);
      const relative = path.relative(canonicalRoot, resolved);
      const inProject = !path.isAbsolute(relative) && relative !== ".." && !relative.startsWith(".." + path.sep);
      if (inProject && configured === undefined) continue;
      if (!(await lstat(resolved)).isFile()) continue;
      await access(resolved, constants.X_OK);
      return resolved;
    } catch { /* Missing PATH entries are not executable candidates. */ }
  }
  throw new Error("reviewed ripgrep executable is unavailable; configure an absolute WEBPI_RG_BIN or install it outside the project cwd");
}

class SearchBudgetError extends Error {}

async function runRg(root: string, args: string[]): Promise<string> {
  const executable = await resolveSearchBinary(root);
  return new Promise<string>((resolve, reject) => {
    const child = spawn(executable, args, { cwd: root, stdio: ["ignore", "pipe", "pipe"], windowsHide: true, shell: false });
    const chunks: Buffer[] = [];
    let size = 0;
    let stderrSize = 0;
    let failure: Error | undefined;
    const stop = (message: string) => { failure ??= new Error(message); child.kill(); };
    const timer = setTimeout(() => stop("Pi search exceeded its 15-second deadline; narrow the search"), 15_000);
    child.stdout.on("data", (chunk: Buffer) => {
      size += chunk.length;
      if (size > MAX_SEARCH_BYTES) stop("Pi search exceeded its output bound; narrow the search");
      else chunks.push(chunk);
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderrSize += chunk.length;
      if (stderrSize > 64 * 1024) stop("Pi search exceeded its diagnostic bound");
    });
    child.once("error", (error) => { failure ??= error; });
    child.once("close", (code) => {
      clearTimeout(timer);
      if (failure) reject(failure);
      else if (code !== 0 && code !== 1) reject(new Error("Pi search failed; check the pattern and local search runtime"));
      else resolve(Buffer.concat(chunks).toString("utf8"));
    });
  });
}

function searchArgs(glob?: string): string[] {
  const args = ["--no-config", "--no-follow", "--hidden"];
  if (glob !== undefined) {
    if (!glob || glob.startsWith("!") || /[\x00-\x1f\x7f]/u.test(glob)) throw new Error("search glob must be a positive bounded pattern");
    args.push("--glob", glob);
  }
  for (const name of EXCLUDED) args.push("--iglob", "!**/" + name, "--iglob", "!**/" + name + "/**");
  return args;
}

export function createGuardedGrepTool(root: string) {
  return {
    async execute(_id: string, input: { pattern: string; path?: string; glob?: string; literal?: boolean; ignoreCase?: boolean; context?: number; limit?: number }, _signal?: unknown, _update?: unknown) {
      const relative = safeRelative(input.path);
      const absolute = await checkedProjectPath(root, relative);
      const limit = Math.max(1, Math.min(input.limit ?? 100, 1000));
      const context = Math.max(0, Math.min(input.context ?? 0, 20));
      const args = [...searchArgs(input.glob), "--json", "--line-number", "--color=never", "--max-filesize", "8M", "--max-count", String(limit)];
      if (input.literal) args.push("--fixed-strings");
      if (input.ignoreCase) args.push("--ignore-case");
      args.push("--", input.pattern, absolute);
      const output = await runRg(root, args);
      const cache = new Map<string, string[]>();
      const lines: string[] = [];
      let matches = 0;
      let totalBytes = 0;
      for (const record of output.split("\n")) {
        if (!record || matches >= limit) continue;
        const event = JSON.parse(record);
        if (event.type !== "match" || typeof event.data?.path?.text !== "string" || !Number.isInteger(event.data.line_number)) continue;
        // Never forward rg's raw line bytes: a link swap or an excluded-path alias
        // must not turn the search subprocess into a credential disclosure path.
        let file: string;
        let content: string[];
        try {
          file = relativeInside(path.resolve(root), path.resolve(root, event.data.path.text));
          content = cache.get(file) ?? [];
          if (!cache.has(file)) {
            const bytes = await readProjectFile(root, file);
            totalBytes += bytes.length;
            if (totalBytes > MAX_CONTEXT_BYTES) throw new SearchBudgetError("search context read budget exceeded; narrow the search");
            content = bytes.toString("utf8").replaceAll("\r\n", "\n").split("\n");
            cache.set(file, content);
          }
        } catch (error) {
          if (error instanceof SearchBudgetError) throw error;
          continue;
        }
        matches += 1;
        const number = event.data.line_number as number;
        for (let line = Math.max(1, number - context); line <= Math.min(content.length, number + context); line += 1) {
          const separator = line === number ? ":" : "-";
          lines.push(`${file.split(path.sep).join("/")}${separator}${line}${separator} ${(content[line - 1] ?? "").slice(0, 2000)}`);
        }
      }
      if (matches >= limit) lines.push(`[${limit} matches limit reached; narrow the search]`);
      return { content: [{ type: "text" as const, text: lines.join("\n").slice(0, 64 * 1024) || "No matches found" }] };
    },
  };
}

export function createGuardedFindTool(root: string) {
  return createFindTool(root, { operations: {
    exists: async (absolute) => { await checkedProjectPath(root, relativeInside(path.resolve(root), absolute)); return true; },
    glob: async (pattern, absolute, options) => {
      await checkedProjectPath(root, relativeInside(path.resolve(root), absolute));
      const result = await runRg(root, [...searchArgs(pattern), "--files", "--null", "--", absolute]);
      const files: string[] = [];
      for (const candidate of result.split("\0")) {
        if (!candidate || files.length >= Math.max(1, Math.min(options.limit, 1000))) continue;
        try {
          const relative = relativeInside(path.resolve(root), path.resolve(root, candidate));
          const checked = await checkedProjectPath(root, relative);
          const info = await lstat(checked);
          if (info.isFile() && info.nlink === 1) files.push(checked);
        } catch { /* Sensitive, linked, or concurrently removed entries are not published. */ }
      }
      return files;
    },
  } });
}

export function createGuardedLsTool(root: string) {
  const check = (absolute: string) => checkedProjectPath(root, relativeInside(path.resolve(root), absolute));
  return createLsTool(root, { operations: {
    exists: async (absolute) => { await check(absolute); return true; },
    stat: async (absolute) => lstat(await check(absolute)),
    readdir: async (absolute) => {
      const directory = await opendir(await check(absolute));
      const entries: string[] = [];
      let visited = 0;
      for await (const entry of directory) {
        if (++visited > MAX_DIRECTORY_ENTRIES) throw new Error("directory exceeds Pi listing scan bound; use a narrower canonical file query");
        try {
          const checked = await check(path.join(absolute, entry.name));
          const info = await lstat(checked);
          if (info.isDirectory() || (info.isFile() && info.nlink === 1)) entries.push(entry.name);
        } catch { /* Do not list credentials, links, or special files. */ }
      }
      return entries;
    },
  } });
}
