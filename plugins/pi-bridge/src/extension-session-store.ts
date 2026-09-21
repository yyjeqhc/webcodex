import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";

import type { FileEntry, SessionManager } from "@earendil-works/pi-coding-agent";

const DOCUMENT_VERSION = 1;
const MAX_STATE_BYTES = 16 * 1024 * 1024;

interface ExtensionSessionDocument {
  version: 1;
  cwd: string;
  entries: FileEntry[];
}

function isFileEntry(value: unknown): value is FileEntry {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    typeof (value as { type?: unknown }).type === "string"
  );
}

export class ExtensionSessionStore {
  readonly file: string;
  private readonly cwd: string;

  constructor(cwd: string, agentDir: string) {
    this.cwd = path.resolve(cwd);
    this.file = path.join(agentDir, "extension-state", "session.json");
  }

  load(): FileEntry[] {
    if (!existsSync(this.file)) return [];
    const size = statSync(this.file).size;
    if (size > MAX_STATE_BYTES) {
      throw new Error("WebPi Pi extension session state exceeds the bounded state budget");
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(readFileSync(this.file, "utf8"));
    } catch (error) {
      throw new Error(
        "WebPi Pi extension session state is malformed: " +
          (error instanceof Error ? error.message : String(error)),
      );
    }

    if (
      typeof parsed !== "object" ||
      parsed === null ||
      Array.isArray(parsed) ||
      (parsed as { version?: unknown }).version !== DOCUMENT_VERSION ||
      typeof (parsed as { cwd?: unknown }).cwd !== "string" ||
      path.resolve((parsed as { cwd: string }).cwd) !== this.cwd ||
      !Array.isArray((parsed as { entries?: unknown }).entries) ||
      !(parsed as { entries: unknown[] }).entries.every(isFileEntry)
    ) {
      throw new Error("WebPi Pi extension session state has an invalid structure");
    }

    return structuredClone((parsed as ExtensionSessionDocument).entries);
  }

  save(sessionManager: SessionManager): void {
    const header = sessionManager.getHeader();
    if (header === null) {
      throw new Error("Pi extension session has no session header");
    }

    const document: ExtensionSessionDocument = {
      version: DOCUMENT_VERSION,
      cwd: this.cwd,
      entries: [header, ...sessionManager.getEntries()],
    };
    const serialized = JSON.stringify(document) + "\n";
    if (Buffer.byteLength(serialized, "utf8") > MAX_STATE_BYTES) {
      throw new Error("WebPi Pi extension session state exceeds the bounded state budget");
    }

    mkdirSync(path.dirname(this.file), { recursive: true });
    const temp = this.file + ".tmp-" + process.pid + "-" + Date.now();
    try {
      writeFileSync(temp, serialized, { encoding: "utf8", flag: "wx" });
      renameSync(temp, this.file);
    } finally {
      rmSync(temp, { force: true });
    }
  }
}
