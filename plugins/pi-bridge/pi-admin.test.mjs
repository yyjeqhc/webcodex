import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const PROTOCOL_VERSION = "webcodex-plugin-v1";
const here = path.dirname(fileURLToPath(import.meta.url));
const adminPath = path.join(here, "dist", "pi-admin.js");
const pluginPath = path.join(here, "dist", "plugin.js");

function tempRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "webpi-pi-admin-"));
}

function runAdmin(cwd, args, expect = 0) {
  const result = spawnSync(process.execPath, [adminPath, ...args], {
    cwd,
    env: { ...process.env },
    encoding: "utf8",
    windowsHide: true,
    shell: false,
    timeout: 30_000,
  });
  assert.equal(result.status, expect, result.stderr || result.stdout);
  return result.stdout.trim() ? JSON.parse(result.stdout) : {};
}

async function runProtocol(cwd, requests) {
  const child = spawn(process.execPath, [pluginPath], {
    cwd,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
    shell: false,
  });
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => (stdout += chunk));
  child.stderr.on("data", (chunk) => (stderr += chunk));
  for (const request of requests) child.stdin.write(JSON.stringify(request) + "\n");
  child.stdin.end();

  const exitCode = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error("pi-admin bridge protocol test timed out"));
    }, 20_000);
    child.once("exit", (code) => {
      clearTimeout(timer);
      resolve(code);
    });
  });

  assert.equal(exitCode, 0, stderr);
  assert.equal(stderr, "");
  return stdout
    .split(/\r?\n/u)
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function init(id = 1) {
  return { jsonrpc: "2.0", id, method: "initialize", params: { protocolVersion: PROTOCOL_VERSION } };
}

function call(name, args, id) {
  return { jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args } };
}

function createFixturePackage(root) {
  const pkg = path.join(root, "fixture-package");
  fs.mkdirSync(path.join(pkg, "extensions"), { recursive: true });
  fs.mkdirSync(path.join(pkg, "skills", "package-skill"), { recursive: true });
  fs.mkdirSync(path.join(pkg, "prompts"), { recursive: true });
  fs.writeFileSync(
    path.join(pkg, "package.json"),
    JSON.stringify(
      {
        name: "webpi-fixture-package",
        version: "1.0.0",
        keywords: ["pi-package"],
        pi: {
          extensions: ["./extensions"],
          skills: ["./skills"],
          prompts: ["./prompts"],
        },
      },
      null,
      2,
    ) + "\n",
  );
  fs.writeFileSync(
    path.join(pkg, "extensions", "fixture.ts"),
    [
      'import { Type } from "@earendil-works/pi-ai";',
      "export default function (pi) {",
      "  pi.registerTool({",
      '    name: "package_echo", label: "Package echo", description: "Package lifecycle fixture.",',
      "    parameters: Type.Object({ value: Type.String() }),",
      '    async execute(_id, params) { return { content: [{ type: "text", text: "pkg:" + params.value }], details: {} }; },',
      "  });",
      "}",
      "",
    ].join("\n"),
  );
  fs.writeFileSync(
    path.join(pkg, "skills", "package-skill", "SKILL.md"),
    [
      "---",
      "name: package-skill",
      "description: Package lifecycle skill.",
      "---",
      "# Package skill",
      "Loaded from a Pi package.",
      "",
    ].join("\n"),
  );
  fs.writeFileSync(
    path.join(pkg, "prompts", "package-prompt.md"),
    [
      "---",
      "description: Package lifecycle prompt.",
      "---",
      "Package prompt $1.",
      "",
    ].join("\n"),
  );
  return pkg;
}

test("WebPi local Pi admin reuses native Pi trust, package lifecycle, and extension approval", async () => {
  const root = tempRoot();
  try {
    const pkg = createFixturePackage(root);

    const initialTrust = runAdmin(root, ["trust-status"]);
    assert.equal(initialTrust.trusted, false);

    const trusted = runAdmin(root, ["trust-set", "true"]);
    assert.equal(trusted.trusted, true);

    const installed = runAdmin(root, ["package-install", pkg, "--scope", "project"]);
    assert.equal(installed.installed, true);
    assert.equal(installed.scope, "project");

    const packages = runAdmin(root, ["package-list"]);
    assert.equal(packages.packages.length, 1);
    assert.equal(packages.packages[0].scope, "project");

    const candidates = runAdmin(root, ["extension-candidates"]);
    assert.equal(candidates.candidates.length, 1);
    assert.equal(candidates.candidates[0].approved, false);
    assert.equal(candidates.candidates[0].scope, "project");
    assert.match(candidates.candidates[0].candidateId, /^wpx_[a-f0-9]{24}$/u);

    const approval = runAdmin(root, [
      "extension-approve",
      candidates.candidates[0].candidateId,
    ]);
    assert.equal(approval.approved, true);
    assert.equal(approval.candidateId, candidates.candidates[0].candidateId);

    const bridge = await runProtocol(root, [
      init(1),
      call("pi_resource_inventory", {}, 2),
      call("pi_extension_tool_call", { tool: "package_echo", arguments: { value: "ok" } }, 3),
      call("pi_skill_read", { name: "package-skill" }, 4),
      call("pi_prompt_expand", { name: "package-prompt", arguments: ["alpha"] }, 5),
    ]);
    const inventory = bridge[1].result.structuredContent;
    assert.equal(inventory.extensions.loaded, 1);
    assert.equal(inventory.packages.length, 1);
    assert.ok(inventory.skills.some((skill) => skill.name === "package-skill"));
    assert.ok(inventory.prompts.some((prompt) => prompt.name === "package-prompt"));
    assert.equal(bridge[2].result.structuredContent.text, "pkg:ok");
    assert.match(bridge[3].result.structuredContent.content, /Loaded from a Pi package/u);
    assert.equal(bridge[4].result.structuredContent.expanded.trim(), "Package prompt alpha.");

    const updated = runAdmin(root, ["package-update"]);
    assert.equal(updated.updated, true);

    const removed = runAdmin(root, ["package-remove", pkg, "--scope", "project"]);
    assert.equal(removed.removed, true);
    assert.equal(runAdmin(root, ["package-list"]).packages.length, 0);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("WebPi web bridge exposes native Pi package self-extension with exact extension approval", async () => {
  const root = tempRoot();
  try {
    createFixturePackage(root);

    const untrusted = await runProtocol(root, [
      init(1),
      call(
        "pi_package_install",
        { source: "./fixture-package", scope: "project", confirmLifecycleScripts: true },
        2,
      ),
    ]);
    assert.equal(untrusted[1].result.isError, true);
    assert.match(untrusted[1].result.content[0].text, /explicit.*trust|trust/i);
    assert.equal(runAdmin(root, ["package-list"]).packages.length, 0);

    runAdmin(root, ["trust-set", "true"]);

    const refused = await runProtocol(root, [
      init(1),
      call(
        "pi_package_install",
        { source: "./fixture-package", scope: "project", confirmLifecycleScripts: false },
        2,
      ),
    ]);
    assert.equal(refused[1].result.isError, true);
    assert.match(refused[1].result.content[0].text, /lifecycle|confirm/i);
    assert.equal(runAdmin(root, ["package-list"]).packages.length, 0);

    const installed = await runProtocol(root, [
      init(1),
      call(
        "pi_package_install",
        { source: "./fixture-package", scope: "project", confirmLifecycleScripts: true },
        2,
      ),
      call("pi_extension_candidate_status", {}, 3),
    ]);
    const installInventory = installed[1].result.structuredContent;
    assert.equal(installed[1].result.isError, false);
    assert.equal(installInventory.packages.length, 1);
    assert.equal(installInventory.extensions.loaded, 0);
    assert.ok(installInventory.skills.some((skill) => skill.name === "package-skill"));
    assert.ok(installInventory.prompts.some((prompt) => prompt.name === "package-prompt"));

    const candidates = installed[2].result.structuredContent.candidates;
    assert.equal(candidates.length, 1);
    const candidate = candidates[0];
    assert.match(candidate.candidateId, /^wpx_[a-f0-9]{24}$/u);
    assert.match(candidate.sha256, /^[a-f0-9]{64}$/u);
    assert.equal(candidate.approved, false);
    assert.equal(candidate.executable, false);
    assert.equal(candidate.nextAction, "approve_exact_fingerprint");
    assert.equal(candidate.displayPath.includes(root), false);

    const staleApproval = await runProtocol(root, [
      init(1),
      call(
        "pi_extension_approve",
        { candidateId: candidate.candidateId, sha256: "0".repeat(64) },
        2,
      ),
    ]);
    assert.equal(staleApproval[1].result.isError, true);
    assert.match(staleApproval[1].result.content[0].text, /fingerprint|changed/i);

    const activated = await runProtocol(root, [
      init(1),
      call(
        "pi_extension_approve",
        { candidateId: candidate.candidateId, sha256: candidate.sha256 },
        2,
      ),
      call("pi_resource_reload", {}, 3),
      call("pi_extension_tool_call", { tool: "package_echo", arguments: { value: "web" } }, 4),
    ]);
    assert.equal(activated[1].result.isError, false);
    assert.equal(activated[1].result.structuredContent.reloadRequired, true);
    assert.equal(activated[1].result.structuredContent.candidate.approved, true);
    assert.equal(activated[2].result.structuredContent.extensions.loaded, 1);
    assert.equal(activated[3].result.structuredContent.text, "pkg:web");

    const revoked = await runProtocol(root, [
      init(1),
      call("pi_extension_revoke", { candidateId: candidate.candidateId }, 2),
      call("pi_extension_tool_list", {}, 3),
      call(
        "pi_package_remove",
        { source: "./fixture-package", scope: "project", confirmLifecycleScripts: true },
        4,
      ),
    ]);
    assert.equal(revoked[1].result.structuredContent.extensions.loaded, 0);
    assert.equal(
      revoked[2].result.structuredContent.tools.some((tool) => tool.name === "package_echo"),
      false,
    );
    assert.equal(revoked[3].result.structuredContent.packages.length, 0);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("WebPi Pi admin refuses extension approval outside the native resolved candidate set", () => {
  const root = tempRoot();
  try {
    runAdmin(root, ["trust-set", "true"]);
    const arbitrary = path.join(root, "arbitrary.ts");
    fs.writeFileSync(arbitrary, "export default () => {};\n");
    const result = spawnSync(
      process.execPath,
      [adminPath, "extension-approve", "wpx_deadbeefdeadbeefdeadbeef"],
      {
        cwd: root,
        encoding: "utf8",
        windowsHide: true,
        shell: false,
        timeout: 30_000,
      },
    );
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /candidate|unknown|resolved/i);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
