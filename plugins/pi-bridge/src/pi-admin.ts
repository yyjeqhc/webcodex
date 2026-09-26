import { createHash } from "node:crypto";
import path from "node:path";

import {
  DefaultPackageManager,
  ProjectTrustStore,
  SettingsManager,
} from "@earendil-works/pi-coding-agent";
import { ExtensionApprovalStore } from "./approval-store.js";
import { inspectNpmPackage } from "./package-inspect.js";

type SourceScope = "user" | "project" | "temporary";

interface Candidate {
  candidateId: string;
  path: string;
  displayPath: string;
  scope: SourceScope;
  approved: boolean;
  sha256: string | null;
  reason?: string;
}

function writeJson(value: unknown): void {
  process.stdout.write(JSON.stringify(value) + "\n");
}

function fail(message: string): never {
  process.stderr.write(message + "\n");
  process.exitCode = 1;
  throw new Error(message);
}

function agentDirFor(cwd: string): string {
  return path.resolve(
    process.env.WEBPI_PI_AGENT_DIR ?? path.join(cwd, ".webpi-state", "pi-agent"),
  );
}

function displayPath(cwd: string, agentDir: string, value: string): string {
  const resolved = path.resolve(value);
  const cwdPrefix = cwd.endsWith(path.sep) ? cwd : cwd + path.sep;
  const agentPrefix = agentDir.endsWith(path.sep) ? agentDir : agentDir + path.sep;
  if (resolved === cwd) return ".";
  if (resolved.startsWith(cwdPrefix)) {
    return path.relative(cwd, resolved).split(path.sep).join("/");
  }
  if (resolved === agentDir) return "<webpi-agent>";
  if (resolved.startsWith(agentPrefix)) {
    return "<webpi-agent>/" + path.relative(agentDir, resolved).split(path.sep).join("/");
  }
  return "<external>/" + path.basename(resolved);
}

function candidateIdFor(extensionPath: string): string {
  return (
    "wpx_" +
    createHash("sha256").update("webpi-extension-candidate\0").update(path.resolve(extensionPath)).digest("hex").slice(0, 24)
  );
}

function parseScope(args: string[]): { args: string[]; scope: "user" | "project" } {
  const result = [...args];
  const at = result.indexOf("--scope");
  if (at < 0) return { args: result, scope: "project" };
  const value = result[at + 1];
  if (value !== "user" && value !== "project") {
    fail("--scope must be user or project");
  }
  result.splice(at, 2);
  return { args: result, scope: value };
}

async function runtime(cwd: string) {
  const agentDir = agentDirFor(cwd);
  const trustStore = new ProjectTrustStore(agentDir);
  const trusted = trustStore.get(cwd) === true;
  const settingsManager = SettingsManager.create(cwd, agentDir, { projectTrusted: trusted });
  const packageManager = new DefaultPackageManager({ cwd, agentDir, settingsManager });
  const approvalStore = new ExtensionApprovalStore(cwd, agentDir);
  return { cwd, agentDir, trustStore, trusted, settingsManager, packageManager, approvalStore };
}

async function extensionCandidates(cwd: string): Promise<Candidate[]> {
  const rt = await runtime(cwd);
  await rt.settingsManager.reload();
  const resolved = await rt.packageManager.resolve(async () => "skip");
  const byPath = new Map<string, SourceScope>();
  for (const entry of resolved.extensions) {
    if (!entry.enabled) continue;
    const resolvedPath = path.resolve(entry.path);
    const existing = byPath.get(resolvedPath);
    if (existing === undefined || (existing === "user" && entry.metadata.scope === "project")) {
      byPath.set(resolvedPath, entry.metadata.scope);
    }
  }

  const candidates: Candidate[] = [];
  for (const [extensionPath, scope] of [...byPath.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    const status = await rt.approvalStore.status(extensionPath);
    candidates.push({
      candidateId: candidateIdFor(extensionPath),
      path: extensionPath,
      displayPath: displayPath(cwd, rt.agentDir, extensionPath),
      scope,
      approved: status.approved,
      sha256: status.sha256,
      ...(status.reason === undefined ? {} : { reason: status.reason }),
    });
  }
  return candidates;
}

async function main(): Promise<void> {
  const cwd = path.resolve(process.cwd());
  const [command, ...rawArgs] = process.argv.slice(2);
  if (!command) fail("usage: pi-admin <command> [arguments]");

  if (command === "trust-status") {
    const rt = await runtime(cwd);
    writeJson({ trusted: rt.trustStore.get(cwd) === true });
    return;
  }

  if (command === "trust-set") {
    const value = rawArgs[0];
    if (value !== "true" && value !== "false") fail("trust-set requires true or false");
    const rt = await runtime(cwd);
    rt.trustStore.set(cwd, value === "true");
    writeJson({ trusted: value === "true" });
    return;
  }

  if (command === "package-list") {
    const rt = await runtime(cwd);
    writeJson({
      packages: rt.packageManager.listConfiguredPackages().map((entry) => ({
        source: entry.source,
        scope: entry.scope,
        filtered: entry.filtered,
        installed: entry.installedPath !== undefined,
        ...(entry.installedPath === undefined
          ? {}
          : { installedPath: displayPath(cwd, rt.agentDir, entry.installedPath) }),
      })),
    });
    return;
  }

  if (command === "package-inspect") {
    const source = rawArgs[0];
    if (!source) fail("package-inspect requires an explicit npm: package source");
    writeJson(await inspectNpmPackage(source));
    return;
  }

  if (command === "package-install") {
    const parsed = parseScope(rawArgs);
    const source = parsed.args[0];
    if (!source) fail("package-install requires a source");
    const rt = await runtime(cwd);
    await rt.packageManager.installAndPersist(source, { local: parsed.scope === "project" });
    writeJson({ installed: true, source, scope: parsed.scope });
    return;
  }

  if (command === "package-remove") {
    const parsed = parseScope(rawArgs);
    const source = parsed.args[0];
    if (!source) fail("package-remove requires a source");
    const rt = await runtime(cwd);
    const removed = await rt.packageManager.removeAndPersist(source, {
      local: parsed.scope === "project",
    });
    writeJson({ removed, source, scope: parsed.scope });
    return;
  }

  if (command === "package-update") {
    const source = rawArgs[0];
    const rt = await runtime(cwd);
    await rt.packageManager.update(source);
    writeJson({ updated: true, ...(source === undefined ? {} : { source }) });
    return;
  }

  if (command === "extension-candidates") {
    const candidates = await extensionCandidates(cwd);
    writeJson({
      candidates: candidates.map(({ path: _path, ...candidate }) => candidate),
    });
    return;
  }

  if (command === "extension-approvals") {
    const rt = await runtime(cwd);
    const approved = await rt.approvalStore.list();
    writeJson({
      approvals: approved.map((entry) => ({
        displayPath: displayPath(cwd, rt.agentDir, entry.path),
        candidateId: candidateIdFor(entry.path),
        sha256: entry.sha256,
      })),
    });
    return;
  }

  if (command === "extension-approve" || command === "extension-revoke") {
    const candidateId = rawArgs[0];
    if (!candidateId) fail(command + " requires a candidate id from extension-candidates");
    const rt = await runtime(cwd);
    const candidates = await extensionCandidates(cwd);
    const candidate = candidates.find((entry) => entry.candidateId === candidateId);
    if (candidate === undefined) fail("unknown extension candidate id; refresh extension-candidates");
    if (candidate.scope === "project" && rt.trustStore.get(cwd) !== true) {
      fail("project extension approval requires Pi project trust first");
    }

    if (command === "extension-approve") {
      const status = await rt.approvalStore.approve(candidate.path);
      writeJson({
        candidateId,
        displayPath: candidate.displayPath,
        scope: candidate.scope,
        approved: status.approved,
        sha256: status.sha256,
      });
      return;
    }

    const revoked = await rt.approvalStore.revoke(candidate.path);
    writeJson({
      candidateId,
      displayPath: candidate.displayPath,
      scope: candidate.scope,
      revoked,
    });
    return;
  }

  fail(
    "unknown pi-admin command; expected trust-status, trust-set, package-list, package-inspect, package-install, package-remove, package-update, extension-candidates, extension-approvals, extension-approve, or extension-revoke",
  );
}

main().catch((error) => {
  if (process.exitCode !== 1) {
    process.exitCode = 1;
    process.stderr.write((error instanceof Error ? error.message : String(error)) + "\n");
  }
});
