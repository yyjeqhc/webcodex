import { createHash, randomUUID } from "node:crypto";
import { constants } from "node:fs";
import { lstat, mkdir, open, opendir, realpath, rename, rm } from "node:fs/promises";
import path from "node:path";

const APPROVAL_FILE_VERSION = 1;
const MAX_DIRECTORY_FILES = 5000;
const MAX_TOTAL_BYTES = 64 * 1024 * 1024;
const MAX_APPROVAL_STORE_BYTES = 1024 * 1024;
const MAX_DIRECTORY_DEPTH = 64;

interface ApprovalDocument {
  version: 1;
  approvals: Record<string, string>;
}

export interface ApprovalStatus {
  path: string;
  sha256: string | null;
  approved: boolean;
  reason?: string;
}

function emptyDocument(): ApprovalDocument {
  return { version: APPROVAL_FILE_VERSION, approvals: {} };
}

async function withApprovalLock<T>(file: string, operation: () => Promise<T>): Promise<T> {
  await mkdir(path.dirname(file), { recursive: true });
  const lockPath = file + ".lock";
  let lock;
  try { lock = await open(lockPath, "wx", 0o600); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === "EEXIST") throw new Error("approval store is busy; finish the other administration operation or inspect its stale lock locally");
    throw error;
  }
  try {
    await lock.writeFile(JSON.stringify({ pid: process.pid, createdAt: new Date().toISOString() }));
    return await operation();
  } finally {
    await lock.close();
    await rm(lockPath, { force: true });
  }
}

async function atomicWriteJson(file: string, value: ApprovalDocument): Promise<void> {
  await mkdir(path.dirname(file), { recursive: true });
  const temp = file + ".tmp-" + randomUUID();
  try {
    const output = await open(temp, "wx", 0o600);
    try {
      await output.writeFile(JSON.stringify(value, null, 2) + "\n", "utf8");
      await output.sync();
    } finally { await output.close(); }
    await rename(temp, file);
  } finally {
    await rm(temp, { force: true }).catch(() => undefined);
  }
}

async function readBoundedRegularFile(file: string, maxBytes: number): Promise<Buffer> {
  const observed = await lstat(file);
  if (!observed.isFile() || observed.isSymbolicLink() || observed.nlink !== 1) {
    throw new Error("approval input must be a regular single-link file, not a symlink or special file");
  }
  if (observed.size > maxBytes) throw new Error("approval input exceeds size limit");
  const handle = await open(file, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const before = await handle.stat();
    if (before.dev !== observed.dev || before.ino !== observed.ino || before.size !== observed.size) {
      throw new Error("approval input changed during open");
    }
    const bytes = Buffer.alloc(before.size + 1);
    let count = 0;
    while (count < bytes.length) {
      const part = await handle.read(bytes, count, bytes.length - count, count);
      if (!part.bytesRead) break;
      count += part.bytesRead;
    }
    const after = await handle.stat();
    const current = await lstat(file);
    if (count !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs
        || after.ctimeMs !== before.ctimeMs || after.nlink !== 1
        || current.isSymbolicLink() || current.dev !== before.dev || current.ino !== before.ino) {
      throw new Error("approval input changed while hashing; review it again");
    }
    return bytes.subarray(0, count);
  } finally { await handle.close(); }
}

async function fingerprintFile(file: string): Promise<{ digest: string; bytes: number }> {
  const content = await readBoundedRegularFile(file, MAX_TOTAL_BYTES);
  return {
    digest: createHash("sha256").update("file\0").update(content).digest("hex"),
    bytes: content.length,
  };
}

async function fingerprintDirectory(directory: string): Promise<string> {
  const root = await realpath(directory);
  const hash = createHash("sha256");
  let files = 0;
  let totalBytes = 0;
  let visited = 0;

  async function walk(current: string, relative: string, depth = 0): Promise<void> {
    if (depth > MAX_DIRECTORY_DEPTH) throw new Error("extension directory exceeds approval depth limit");
    const currentInfo = await lstat(current);
    if (currentInfo.isSymbolicLink() || !currentInfo.isDirectory()) throw new Error("extension directories must not be symlinks/junctions");
    const physical = await realpath(current);
    const within = path.relative(root, physical);
    if (within === ".." || within.startsWith(".." + path.sep) || path.isAbsolute(within)) throw new Error("extension directory escapes its approval root");
    const entries = [];
    for await (const entry of await opendir(current)) {
      if (++visited > MAX_DIRECTORY_FILES) throw new Error("extension directory exceeds approval entry-count limit");
      entries.push(entry);
    }
    entries.sort((a, b) => a.name.localeCompare(b.name));
    for (const entry of entries) {
      const absolute = path.join(current, entry.name);
      const rel = relative ? relative + "/" + entry.name : entry.name;
      if (entry.isSymbolicLink()) {
        throw new Error("extension approval rejects symlinks/junctions; use a reviewed regular-file copy");
      }
      if (entry.isDirectory()) {
        hash.update("dir\0").update(rel).update("\0");
        await walk(absolute, rel, depth + 1);
        continue;
      }
      if (!entry.isFile()) throw new Error("extension approval rejects special files");
      files += 1;
      if (files > MAX_DIRECTORY_FILES) throw new Error("extension directory exceeds approval file-count limit");
      const content = await readBoundedRegularFile(absolute, MAX_TOTAL_BYTES - totalBytes);
      totalBytes += content.length;
      hash.update("file\0").update(rel).update("\0").update(content).update("\0");
    }
  }

  await walk(root, "");
  return hash.digest("hex");
}

async function rejectLinkedAncestors(target: string): Promise<void> {
  for (let current = path.resolve(target); ; current = path.dirname(current)) {
    if ((await lstat(current)).isSymbolicLink()) throw new Error("extension approval rejects symlink/junction ancestors");
    if (path.dirname(current) === current) return;
  }
}

export async function fingerprintExtension(target: string): Promise<string> {
  const resolved = path.resolve(target);
  await rejectLinkedAncestors(resolved);
  const info = await lstat(resolved);
  if (info.isSymbolicLink()) {
    throw new Error("extension approval rejects symlink/junction targets");
  }
  if (info.isFile()) return (await fingerprintFile(resolved)).digest;
  if (info.isDirectory()) return fingerprintDirectory(resolved);
  throw new Error("extension approval target must be a file or directory");
}

export class ExtensionApprovalStore {
  readonly cwd: string;
  readonly agentDir: string;
  readonly file: string;

  constructor(cwd: string, agentDir: string) {
    this.cwd = path.resolve(cwd);
    this.agentDir = path.resolve(agentDir);
    this.file = path.join(this.agentDir, "webpi-extension-approvals.json");
  }

  private async load(): Promise<ApprovalDocument> {
    try {
      const value = JSON.parse((await readBoundedRegularFile(this.file, MAX_APPROVAL_STORE_BYTES)).toString("utf8")) as unknown;
      if (
        typeof value !== "object" ||
        value === null ||
        (value as { version?: unknown }).version !== APPROVAL_FILE_VERSION ||
        typeof (value as { approvals?: unknown }).approvals !== "object" ||
        (value as { approvals?: unknown }).approvals === null ||
        Array.isArray((value as { approvals?: unknown }).approvals)
      ) {
        throw new Error("invalid WebPi extension approval store");
      }
      const document = value as ApprovalDocument;
      const entries = Object.entries(document.approvals);
      if (entries.length > MAX_DIRECTORY_FILES || entries.some(([key, digest]) =>
          !path.isAbsolute(key) || typeof digest !== "string" || !/^[a-f0-9]{64}$/u.test(digest))) {
        throw new Error("invalid WebPi extension approval entries");
      }
      return document;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return emptyDocument();
      throw error;
    }
  }

  async status(target: string): Promise<ApprovalStatus> {
    const resolved = path.resolve(target);
    let sha256: string;
    try {
      sha256 = await fingerprintExtension(resolved);
    } catch (error) {
      return {
        path: resolved,
        sha256: null,
        approved: false,
        reason: error instanceof Error ? error.message : String(error),
      };
    }
    const document = await this.load();
    return {
      path: resolved,
      sha256,
      approved: document.approvals[resolved] === sha256,
    };
  }

  async approve(target: string): Promise<ApprovalStatus> {
    return withApprovalLock(this.file, async () => {
      const resolved = path.resolve(target);
      const sha256 = await fingerprintExtension(resolved);
      const document = await this.load();
      document.approvals[resolved] = sha256;
      await atomicWriteJson(this.file, document);
      return { path: resolved, sha256, approved: true };
    });
  }

  async approveExpected(target: string, expectedSha256: string): Promise<ApprovalStatus> {
    return withApprovalLock(this.file, async () => {
      const resolved = path.resolve(target);
      const sha256 = await fingerprintExtension(resolved);
      if (sha256 !== expectedSha256) {
        throw new Error("extension fingerprint changed; refresh candidate status before approval");
      }
      const document = await this.load();
      document.approvals[resolved] = sha256;
      await atomicWriteJson(this.file, document);
      return { path: resolved, sha256, approved: true };
    });
  }

  async revoke(target: string): Promise<boolean> {
    return withApprovalLock(this.file, async () => {
      const resolved = path.resolve(target);
      const document = await this.load();
      if (!(resolved in document.approvals)) return false;
      delete document.approvals[resolved];
      await atomicWriteJson(this.file, document);
      return true;
    });
  }

  async list(): Promise<Array<{ path: string; sha256: string }>> {
    const document = await this.load();
    return Object.entries(document.approvals)
      .map(([entryPath, sha256]) => ({ path: entryPath, sha256 }))
      .sort((a, b) => a.path.localeCompare(b.path));
  }
}
