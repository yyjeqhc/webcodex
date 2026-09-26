import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const PROTOCOL_VERSION = "webcodex-plugin-v1";
const here = path.dirname(fileURLToPath(import.meta.url));
const pluginPath = path.join(here, "dist", "plugin.js");

function tempRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "webpi-pi-bridge-"));
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
  for (const request of requests) child.stdin.write(`${JSON.stringify(request)}\n`);
  child.stdin.end();

  const exitCode = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error("pi-bridge protocol test timed out"));
    }, 15_000);
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

async function approveExtension(root, extensionPath) {
  const { ExtensionApprovalStore } = await import("./dist/approval-store.js");
  const agentDir = path.join(root, ".webpi-state", "pi-agent");
  return new ExtensionApprovalStore(root, agentDir).approve(extensionPath);
}

test("Pi bridge lists a frozen read-only catalog and executes Pi read/search tools", async () => {
  const root = tempRoot();
  try {
    fs.mkdirSync(path.join(root, "src"), { recursive: true });
    fs.writeFileSync(path.join(root, "README.md"), "# WebPi fixture\nneedle-value\n");
    fs.writeFileSync(path.join(root, "src", "main.ts"), "export const needle = 42;\n");
    fs.mkdirSync(path.join(root, ".pi", "extensions", "sample"), { recursive: true });
    fs.writeFileSync(path.join(root, ".pi", "extensions", "sample", "index.ts"), "export default () => {};\n");

    const responses = await runProtocol(root, [
      init(1),
      { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
      call("pi_read", { path: "README.md", limit: 20 }, 3),
      call("pi_grep", { pattern: "needle", path: "src", literal: true }, 4),
      call("pi_extension_inventory", {}, 5),
    ]);

    assert.equal(responses[0].result.protocolVersion, PROTOCOL_VERSION);
    const names = responses[1].result.tools.map((tool) => tool.name);
    for (const expected of ["pi_read", "pi_grep", "pi_find", "pi_ls", "pi_extension_inventory", "pi_extension_tool_describe"]) {
      assert.ok(names.includes(expected), "missing bridge tool: " + expected);
    }
    assert.equal(responses[2].result.isError, false);
    assert.match(responses[2].result.structuredContent.text, /WebPi fixture/u);
    assert.equal(responses[2].result.structuredContent.engine, "pi");
    assert.equal(responses[3].result.isError, false);
    assert.match(responses[3].result.structuredContent.text, /main\.ts/u);
    assert.equal(responses[4].result.isError, false);
    assert.equal(responses[4].result.structuredContent.totalCount, 1);
    assert.equal(responses[4].result.structuredContent.extensions[0].name, "sample");
    assert.equal(JSON.stringify(responses).includes(root), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi bridge rejects traversal and sensitive paths before calling Pi", async () => {
  const root = tempRoot();
  try {
    fs.writeFileSync(path.join(root, ".env"), "SECRET=nope\n");
    const responses = await runProtocol(root, [
      init(1),
      call("pi_read", { path: "../outside.txt" }, 2),
      call("pi_read", { path: ".env" }, 3),
    ]);
    assert.equal(responses[1].result.isError, true);
    assert.match(responses[1].result.content[0].text, /escapes/u);
    assert.equal(responses[2].result.isError, true);
    assert.match(responses[2].result.content[0].text, /sensitive/u);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("filesystem extension inventory rejects an aliased root without leaking its entries", async () => {
  const root = tempRoot();
  try {
    const target = path.join(root, ".webpi-state", "private-extensions");
    fs.mkdirSync(target, { recursive: true });
    fs.mkdirSync(path.join(root, ".pi"));
    fs.writeFileSync(path.join(target, "private-entry.ts"), "export default () => {};\n");
    fs.symlinkSync(target, path.join(root, ".pi", "extensions"), process.platform === "win32" ? "junction" : "dir");
    const responses = await runProtocol(root, [init(1), call("pi_extension_inventory", {}, 2)]);
    assert.equal(responses[1].result.isError, true);
    assert.equal(responses[1].result.structuredContent.extensions.length, 0);
    assert.equal(JSON.stringify(responses).includes("private-entry.ts"), false);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test("Pi bridge loads native Pi extensions, hooks, skills, prompts, and context without a second agent loop", async () => {
  const root = tempRoot();
  try {
    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    const agentDir = path.join(root, ".webpi-state", "pi-agent");
    new ProjectTrustStore(agentDir).set(root, true);
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    fs.mkdirSync(path.join(root, ".pi", "skills", "fixture-skill"), { recursive: true });
    fs.mkdirSync(path.join(root, ".pi", "prompts"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pi", "extensions", "fixture.ts"),
      [
        'import { Type } from "typebox";',
        "export default function (pi) {",
        '  let commandValue = "unset";',
        '  pi.on("tool_call", (event) => {',
        '    if (event.toolName === "fixture_echo" && typeof event.input?.value === "string") {',
        '      event.input.value = "hooked:" + event.input.value;',
        "    }",
        "  });",
        "  pi.registerTool({",
        '    name: "fixture_echo",',
        '    label: "Fixture echo",',
        '    description: "Echo a value after a Pi tool_call hook mutates it.",',
        "    parameters: Type.Object({ value: Type.String() }),",
        "    async execute(_id, params, _signal, _update, ctx) {",
        "      return {",
        '        content: [{ type: "text", text: params.value + "|mode=" + ctx.mode + "|trusted=" + ctx.isProjectTrusted() + "|command=" + commandValue }],',
        "        details: { value: params.value },",
        "      };",
        "    },",
        "  });",
        '  pi.registerCommand("fixture-command", {',
        '    description: "Record command arguments for the fixture.",',
        "    async handler(args) { commandValue = args; },",
        "  });",
        "}",
        "",
      ].join("\n"),
    );
    fs.writeFileSync(
      path.join(root, ".pi", "skills", "fixture-skill", "SKILL.md"),
      [
        "---",
        "name: fixture-skill",
        "description: Fixture skill for WebPi parity tests.",
        "---",
        "",
        "# Fixture skill",
        "",
        "Use fixture semantics.",
        "",
      ].join("\n"),
    );
    fs.writeFileSync(
      path.join(root, ".pi", "prompts", "fixture.md"),
      [
        "---",
        "description: Fixture prompt.",
        "---",
        "Review $1 with $@ and fixture rules.",
        "",
      ].join("\n"),
    );
    fs.writeFileSync(path.join(root, "AGENTS.md"), "# Fixture agents\n- preserve fixture behavior\n");
    await approveExtension(root, path.join(root, ".pi", "extensions", "fixture.ts"));

    const responses = await runProtocol(root, [
      init(1),
      { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
      call("pi_resource_inventory", {}, 3),
      call("pi_extension_tool_list", {}, 4),
      call("pi_extension_tool_call", { tool: "fixture_echo", arguments: { value: "hello" } }, 5),
    ]);

    const names = responses[1].result.tools.map((tool) => tool.name);
    for (const expected of [
      "pi_resource_inventory",
      "pi_extension_candidate_status",
      "pi_extension_approve",
      "pi_extension_revoke",
      "pi_extension_tool_list",
      "pi_extension_tool_call",
      "pi_resource_reload",
      "pi_package_inspect",
      "pi_package_list",
      "pi_package_install",
      "pi_package_update",
      "pi_package_remove",
      "pi_skill_read",
      "pi_prompt_expand",
      "pi_context_snapshot",
      "pi_extension_command_list",
      "pi_extension_command_call",
    ]) {
      assert.ok(names.includes(expected), "missing bridge tool: " + expected);
    }

    const inventory = responses[2].result.structuredContent;
    assert.equal(inventory.extensions.loaded, 1);
    assert.equal(inventory.extensions.errors.length, 0);
    assert.ok(inventory.skills.some((skill) => skill.name === "fixture-skill"));
    assert.ok(inventory.prompts.some((prompt) => prompt.name === "fixture"));
    assert.ok(inventory.contextFiles.some((entry) => entry.path === "AGENTS.md"));

    const extensionTools = responses[3].result.structuredContent.tools;
    const fixtureTool = extensionTools.find((tool) => tool.name === "fixture_echo");
    assert.ok(fixtureTool);
    assert.match(fixtureTool.description, /tool_call hook/u);
    assert.equal(fixtureTool.source.endsWith(".pi/extensions/fixture.ts"), true);

    assert.equal(responses[4].result.isError, false);
    assert.match(responses[4].result.structuredContent.text, /hooked:hello\|mode=print\|trusted=true\|command=unset/u);
    assert.equal(responses[4].result.structuredContent.tool, "fixture_echo");
    assert.equal(responses[4].result.structuredContent.engine, "pi-extension");
    assert.equal(JSON.stringify(responses).includes(root), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi bridge exposes native skill, prompt, context, command, and TypeBox tool semantics", async () => {
  const root = tempRoot();
  try {
    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    const agentDir = path.join(root, ".webpi-state", "pi-agent");
    new ProjectTrustStore(agentDir).set(root, true);
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    fs.mkdirSync(path.join(root, ".pi", "skills", "fixture-skill"), { recursive: true });
    fs.mkdirSync(path.join(root, ".pi", "prompts"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pi", "extensions", "fixture.ts"),
      [
        'import { Type } from "@earendil-works/pi-ai";',
        "export default function (pi) {",
        '  let commandValue = "unset";',
        "  pi.registerTool({",
        '    name: "fixture_echo", label: "Fixture echo", description: "Fixture typed echo.",',
        "    parameters: Type.Object({ value: Type.String() }),",
        "    async execute(_id, params) {",
        '      return { content: [{ type: "text", text: params.value + "|command=" + commandValue }], details: {} };',
        "    },",
        "  });",
        '  pi.registerCommand("fixture-command", {',
        '    description: "Record command arguments.",',
        "    async handler(args) { commandValue = args; },",
        "  });",
        "}",
        "",
      ].join("\n"),
    );
    fs.writeFileSync(
      path.join(root, ".pi", "skills", "fixture-skill", "SKILL.md"),
      [
        "---",
        "name: fixture-skill",
        "description: Fixture skill for native Pi resource parity.",
        "---",
        "# Fixture skill",
        "Native skill body.",
        "",
      ].join("\n"),
    );
    fs.writeFileSync(
      path.join(root, ".pi", "prompts", "fixture.md"),
      [
        "---",
        "description: Fixture prompt expansion.",
        "---",
        "Review $1 with $@.",
        "",
      ].join("\n"),
    );
    fs.writeFileSync(path.join(root, "AGENTS.md"), "# Fixture context\nContext body.\n");
    await approveExtension(root, path.join(root, ".pi", "extensions", "fixture.ts"));

    const responses = await runProtocol(root, [
      init(1),
      call("pi_skill_read", { name: "fixture-skill" }, 2),
      call("pi_prompt_expand", { name: "fixture", arguments: ["alpha", "beta"] }, 3),
      call("pi_context_snapshot", {}, 4),
      call("pi_extension_command_list", {}, 5),
      call("pi_extension_command_call", { command: "fixture-command", arguments: "cmd-value" }, 6),
      call("pi_extension_tool_call", { tool: "fixture_echo", arguments: { value: "ok" } }, 7),
      call("pi_extension_tool_call", { tool: "fixture_echo", arguments: { value: 123 } }, 8),
    ]);

    assert.match(responses[1].result.structuredContent.content, /# Fixture skill\nNative skill body\./u);
    assert.equal(responses[1].result.structuredContent.name, "fixture-skill");

    assert.equal(responses[2].result.structuredContent.expanded.trim(), "Review alpha with alpha beta.");

    assert.ok(
      responses[3].result.structuredContent.contextFiles.some(
        (entry) => entry.path === "AGENTS.md" && /Context body/u.test(entry.content),
      ),
    );

    const commands = responses[4].result.structuredContent.commands;
    assert.ok(commands.some((entry) => entry.name === "fixture-command"));
    assert.equal(responses[5].result.isError, false);
    assert.match(responses[6].result.structuredContent.text, /ok\|command=cmd-value/u);

    assert.equal(responses[7].result.isError, true);
    assert.match(responses[7].result.content[0].text, /invalid|schema|value|string/i);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi package inspect is exposed as a read-only preflight and rejects non-npm sources without mutation", async () => {
  const root = tempRoot();
  try {
    const responses = await runProtocol(root, [
      init(1),
      { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
      call("pi_package_inspect", { source: "git:github.com/example/repo" }, 3),
    ]);
    const tool = responses[1].result.tools.find((entry) => entry.name === "pi_package_inspect");
    assert.ok(tool);
    assert.equal(tool.annotations.readOnlyHint, true);
    assert.equal(tool.annotations.destructiveHint, false);
    assert.equal(tool.annotations.openWorldHint, true);
    assert.equal(responses[2].result.isError, true);
    assert.match(responses[2].result.content[0].text, /only explicit npm:/u);
    assert.equal(responses[2].result.structuredContent.source, "git:github.com/example/repo");
    assert.equal(responses[2].result.structuredContent.sourceReviewComplete, false);
    assert.equal(fs.existsSync(path.join(root, ".webpi-state", "pi-agent", "settings.json")), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi bridge enforces native Pi project trust before executing project extensions", async () => {
  const root = tempRoot();
  try {
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pi", "extensions", "untrusted.ts"),
      [
        'import { Type } from "@earendil-works/pi-ai";',
        "export default function (pi) {",
        "  pi.registerTool({",
        '    name: "untrusted_tool", label: "Untrusted", description: "Must not load before trust.",',
        "    parameters: Type.Object({}),",
        '    async execute() { return { content: [{ type: "text", text: "executed" }], details: {} }; },',
        "  });",
        "}",
        "",
      ].join("\n"),
    );

    const untrusted = await runProtocol(root, [
      init(1),
      call("pi_resource_inventory", {}, 2),
      call("pi_extension_tool_list", {}, 3),
    ]);
    assert.equal(untrusted[1].result.structuredContent.projectTrusted, false);
    assert.equal(untrusted[1].result.structuredContent.extensions.loaded, 0);
    assert.equal(
      untrusted[2].result.structuredContent.tools.some((tool) => tool.name === "untrusted_tool"),
      false,
    );

    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    new ProjectTrustStore(path.join(root, ".webpi-state", "pi-agent")).set(root, true);
    await approveExtension(root, path.join(root, ".pi", "extensions", "untrusted.ts"));
    const trusted = await runProtocol(root, [
      init(1),
      call("pi_resource_inventory", {}, 2),
      call("pi_extension_tool_list", {}, 3),
    ]);
    assert.equal(trusted[1].result.structuredContent.projectTrusted, true);
    assert.equal(trusted[1].result.structuredContent.extensions.loaded, 1);
    assert.equal(
      trusted[2].result.structuredContent.tools.some((tool) => tool.name === "untrusted_tool"),
      true,
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi resource reload replaces the native extension runtime and increments generation", async () => {
  const root = tempRoot();
  try {
    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    new ProjectTrustStore(path.join(root, ".webpi-state", "pi-agent")).set(root, true);
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    fs.writeFileSync(path.join(root, "fixture-version.txt"), "v1");
    fs.writeFileSync(
      path.join(root, ".pi", "extensions", "reload.ts"),
      [
        'import { Type } from "@earendil-works/pi-ai";',
        'import { readFileSync, writeFileSync } from "node:fs";',
        'import path from "node:path";',
        'const version = readFileSync(path.join(process.cwd(), "fixture-version.txt"), "utf8").trim();',
        "export default function (pi) {",
        "  pi.registerTool({",
        '    name: "fixture_version", label: "Fixture version", description: "Return load-time version.",',
        "    parameters: Type.Object({}),",
        '    async execute() { return { content: [{ type: "text", text: version }], details: {} }; },',
        "  });",
        '  pi.registerCommand("write-v2", {',
        '    async handler() { writeFileSync(path.join(process.cwd(), "fixture-version.txt"), "v2"); },',
        "  });",
        "}",
        "",
      ].join("\n"),
    );

    await approveExtension(root, path.join(root, ".pi", "extensions", "reload.ts"));

    const responses = await runProtocol(root, [
      init(1),
      call("pi_extension_tool_call", { tool: "fixture_version", arguments: {} }, 2),
      call("pi_extension_command_call", { command: "write-v2", arguments: "" }, 3),
      call("pi_extension_tool_call", { tool: "fixture_version", arguments: {} }, 4),
      call("pi_resource_reload", {}, 5),
      call("pi_extension_tool_call", { tool: "fixture_version", arguments: {} }, 6),
    ]);
    assert.equal(responses[1].result.structuredContent.text, "v1");
    assert.equal(responses[3].result.structuredContent.text, "v1");
    const generation = responses[4].result.structuredContent.generation;
    assert.ok(generation >= 2);
    assert.equal(responses[5].result.structuredContent.generation, generation);
    assert.equal(responses[5].result.structuredContent.text, "v2");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi extension image content survives the WebPi Native Plugin result boundary", async () => {
  const root = tempRoot();
  try {
    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    const agentDir = path.join(root, ".webpi-state", "pi-agent");
    new ProjectTrustStore(agentDir).set(root, true);
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    const extensionPath = path.join(root, ".pi", "extensions", "image.ts");
    fs.writeFileSync(
      extensionPath,
      [
        'import { Type } from "@earendil-works/pi-ai";',
        "export default function (pi) {",
        "  pi.registerTool({",
        '    name: "fixture_image", label: "Fixture image", description: "Return mixed text and PNG content.",',
        "    parameters: Type.Object({}),",
        "    async execute() {",
        "      return {",
        "        content: [",
        '          { type: "text", text: "caption" },',
        '          { type: "image", data: "iVBORw0KGgo=", mimeType: "image/png" },',
        "        ],",
        "        details: { ok: true },",
        "      };",
        "    },",
        "  });",
        "}",
        "",
      ].join("\n"),
    );
    await approveExtension(root, extensionPath);

    const responses = await runProtocol(root, [
      init(1),
      call("pi_extension_tool_call", { tool: "fixture_image", arguments: {} }, 2),
    ]);
    const result = responses[1].result;
    assert.equal(result.isError, false);
    assert.deepEqual(result.content, [
      { type: "text", text: "caption" },
      { type: "image", data: "iVBORw0KGgo=", mimeType: "image/png" },
    ]);
    assert.deepEqual(result.structuredContent.contentTypes, ["text", "image"]);
    assert.equal(result.structuredContent.engine, "pi-extension");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("WebPi extension approval gates native Pi import before extension code executes", async () => {
  const root = tempRoot();
  try {
    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    const agentDir = path.join(root, ".webpi-state", "pi-agent");
    new ProjectTrustStore(agentDir).set(root, true);
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    const extensionPath = path.join(root, ".pi", "extensions", "approved.ts");
    const marker = path.join(root, "extension-executed.txt");
    const source = [
      'import { Type } from "@earendil-works/pi-ai";',
      'import { writeFileSync } from "node:fs";',
      'import path from "node:path";',
      'writeFileSync(path.join(process.cwd(), "extension-executed.txt"), "executed");',
      "export default function (pi) {",
      "  pi.registerTool({",
      '    name: "approved_tool", label: "Approved", description: "Approval fixture.",',
      "    parameters: Type.Object({}),",
      '    async execute() { return { content: [{ type: "text", text: "approved" }], details: {} }; },',
      "  });",
      "}",
      "",
    ].join("\n");
    fs.writeFileSync(extensionPath, source);

    const pending = await runProtocol(root, [init(1), call("pi_resource_inventory", {}, 2)]);
    assert.equal(pending[1].result.structuredContent.projectTrusted, true);
    assert.equal(pending[1].result.structuredContent.extensions.candidates, 1);
    assert.equal(pending[1].result.structuredContent.extensions.approved, 0);
    assert.equal(pending[1].result.structuredContent.extensions.pending, 1);
    assert.equal(pending[1].result.structuredContent.extensions.loaded, 0);
    assert.equal(fs.existsSync(marker), false, "unapproved extension code executed during discovery");

    const { ExtensionApprovalStore } = await import("./dist/approval-store.js");
    const approvalStore = new ExtensionApprovalStore(root, agentDir);
    await approvalStore.approve(extensionPath);

    const approved = await runProtocol(root, [init(1), call("pi_resource_inventory", {}, 2)]);
    assert.equal(approved[1].result.structuredContent.extensions.approved, 1);
    assert.equal(approved[1].result.structuredContent.extensions.pending, 0);
    assert.equal(approved[1].result.structuredContent.extensions.loaded, 1);
    assert.equal(fs.readFileSync(marker, "utf8"), "executed");

    fs.rmSync(marker, { force: true });
    fs.appendFileSync(extensionPath, "// changed\n");
    const changed = await runProtocol(root, [init(1), call("pi_resource_inventory", {}, 2)]);
    assert.equal(changed[1].result.structuredContent.extensions.approved, 0);
    assert.equal(changed[1].result.structuredContent.extensions.pending, 1);
    assert.equal(changed[1].result.structuredContent.extensions.loaded, 0);
    assert.equal(fs.existsSync(marker), false, "modified extension bypassed hash approval");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi bridge dispatches native session, dynamic-resource, and tool-execution lifecycle events", async () => {
  const root = tempRoot();
  try {
    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    const agentDir = path.join(root, ".webpi-state", "pi-agent");
    new ProjectTrustStore(agentDir).set(root, true);
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    fs.mkdirSync(path.join(root, "dynamic-skills", "dynamic-skill"), { recursive: true });
    fs.mkdirSync(path.join(root, "dynamic-prompts"), { recursive: true });
    fs.writeFileSync(
      path.join(root, "dynamic-skills", "dynamic-skill", "SKILL.md"),
      [
        "---",
        "name: dynamic-skill",
        "description: Discovered from a Pi extension lifecycle hook.",
        "---",
        "# Dynamic skill",
        "Dynamic resource body.",
        "",
      ].join("\n"),
    );
    fs.writeFileSync(
      path.join(root, "dynamic-prompts", "dynamic-prompt.md"),
      [
        "---",
        "description: Dynamic prompt.",
        "---",
        "Dynamic $1.",
        "",
      ].join("\n"),
    );

    const extensionPath = path.join(root, ".pi", "extensions", "lifecycle.ts");
    fs.writeFileSync(
      extensionPath,
      [
        'import { Type } from "@earendil-works/pi-ai";',
        'import { appendFileSync } from "node:fs";',
        'import path from "node:path";',
        'const log = (line) => appendFileSync(path.join(process.cwd(), "lifecycle.log"), line + "\\n");',
        "export default function (pi) {",
        '  pi.on("session_start", (event) => log("session_start:" + event.reason));',
        '  pi.on("session_shutdown", (event) => log("session_shutdown:" + event.reason));',
        '  pi.on("resources_discover", (event) => {',
        '    log("resources_discover:" + event.reason);',
        "    return {",
        '      skillPaths: [path.join(process.cwd(), "dynamic-skills")],',
        '      promptPaths: [path.join(process.cwd(), "dynamic-prompts")],',
        "    };",
        "  });",
        '  pi.on("tool_execution_start", (event) => log("tool_execution_start:" + event.toolName));',
        '  pi.on("tool_execution_end", (event) => log("tool_execution_end:" + event.toolName + ":" + event.isError));',
        "  pi.registerTool({",
        '    name: "lifecycle_echo", label: "Lifecycle echo", description: "Lifecycle fixture.",',
        "    parameters: Type.Object({ value: Type.String() }),",
        '    async execute(_id, params) { return { content: [{ type: "text", text: params.value }], details: {} }; },',
        "  });",
        "}",
        "",
      ].join("\n"),
    );
    await approveExtension(root, extensionPath);

    const responses = await runProtocol(root, [
      init(1),
      call("pi_resource_inventory", {}, 2),
      call("pi_skill_read", { name: "dynamic-skill" }, 3),
      call("pi_prompt_expand", { name: "dynamic-prompt", arguments: ["alpha"] }, 4),
      call("pi_extension_tool_call", { tool: "lifecycle_echo", arguments: { value: "ok" } }, 5),
      call("pi_resource_reload", {}, 6),
      call("pi_extension_tool_call", { tool: "lifecycle_echo", arguments: { value: "after" } }, 7),
    ]);

    assert.ok(responses[1].result.structuredContent.skills.some((entry) => entry.name === "dynamic-skill"));
    assert.match(responses[2].result.structuredContent.content, /Dynamic resource body/u);
    assert.equal(responses[3].result.structuredContent.expanded.trim(), "Dynamic alpha.");
    assert.equal(responses[4].result.structuredContent.text, "ok");
    assert.equal(responses[6].result.structuredContent.text, "after");

    const log = fs.readFileSync(path.join(root, "lifecycle.log"), "utf8").trim().split(/\r?\n/u);
    assert.deepEqual(log, [
      "session_start:startup",
      "resources_discover:startup",
      "tool_execution_start:lifecycle_echo",
      "tool_execution_end:lifecycle_echo:false",
      "session_shutdown:reload",
      "session_start:reload",
      "resources_discover:reload",
      "tool_execution_start:lifecycle_echo",
      "tool_execution_end:lifecycle_echo:false",
      "session_shutdown:quit",
    ]);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi extension appendEntry state survives WebPi plugin process restart", async () => {
  const root = tempRoot();
  try {
    const { ProjectTrustStore } = await import("@earendil-works/pi-coding-agent");
    const agentDir = path.join(root, ".webpi-state", "pi-agent");
    new ProjectTrustStore(agentDir).set(root, true);
    fs.mkdirSync(path.join(root, ".pi", "extensions"), { recursive: true });
    const extensionPath = path.join(root, ".pi", "extensions", "persistent-state.ts");
    fs.writeFileSync(
      extensionPath,
      [
        'import { Type } from "@earendil-works/pi-ai";',
        "export default function (pi) {",
        "  pi.registerTool({",
        '    name: "state_counter", label: "State counter", description: "Persist extension state via Pi session entries.",',
        "    parameters: Type.Object({ action: Type.String() }),",
        "    async execute(_id, params, _signal, _update, ctx) {",
        '      const entries = ctx.sessionManager.getEntries().filter((entry) => entry.type === "custom" && entry.customType === "fixture-state");',
        "      const last = entries.length === 0 ? 0 : Number(entries.at(-1)?.data?.value ?? 0);",
        '      if (params.action === "increment") {',
        "        const next = last + 1;",
        '        pi.appendEntry("fixture-state", { value: next });',
        '        return { content: [{ type: "text", text: String(next) }], details: { value: next } };',
        "      }",
        '      return { content: [{ type: "text", text: String(last) }], details: { value: last } };',
        "    },",
        "  });",
        "}",
        "",
      ].join("\n"),
    );
    await approveExtension(root, extensionPath);

    const first = await runProtocol(root, [
      init(1),
      call("pi_extension_tool_call", { tool: "state_counter", arguments: { action: "increment" } }, 2),
    ]);
    assert.equal(first[1].result.structuredContent.text, "1");

    const second = await runProtocol(root, [
      init(1),
      call("pi_extension_tool_call", { tool: "state_counter", arguments: { action: "get" } }, 2),
    ]);
    assert.equal(second[1].result.structuredContent.text, "1");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("Pi bridge reports the native/equivalent/not-applicable parity contract", async () => {
  const root = tempRoot();
  try {
    const responses = await runProtocol(root, [
      init(1),
      { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
      call("pi_capability_report", {}, 3),
    ]);
    const names = responses[1].result.tools.map((tool) => tool.name);
    assert.ok(names.includes("pi_capability_report"));

    const report = responses[2].result.structuredContent;
    assert.match(report.piVersion, /^0\.85\./u);
    assert.equal(report.architecture, "web-gpt-primary-no-second-pi-model-loop");
    const byName = new Map(report.capabilities.map((entry) => [entry.capability, entry]));
    assert.equal(byName.get("extensions")?.status, "native");
    assert.equal(byName.get("skills_prompts_context")?.status, "native");
    assert.equal(byName.get("packages")?.status, "native");
    assert.equal(byName.get("guarded_mutation_process_git")?.status, "equivalent");
    assert.equal(byName.get("provider_model_events")?.status, "not_applicable");
    assert.equal(byName.get("tui_ui")?.status, "not_applicable");
    assert.equal(byName.get("session_tree_control")?.status, "limited");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
