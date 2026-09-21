import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { ProjectTrustStore } from "@earendil-works/pi-coding-agent";
import { ExtensionApprovalStore } from "./dist/approval-store.js";
import { PiRuntimeHost } from "./dist/pi-runtime-host.js";

async function fixture(run) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "webpi-runtime-security-"));
  const agent = path.join(root, ".webpi-state", "pi-agent");
  const extension = path.join(root, ".pi", "extensions", "security-fixture.ts");
  let host;
  try {
    new ProjectTrustStore(agent).set(root, true);
    await fs.mkdir(path.dirname(extension), { recursive: true });
    await fs.writeFile(extension, `import { Type } from "typebox";
export default function (pi) {
  let ends = 0;
  pi.on("tool_execution_end", () => { ends++; });
  pi.on("tool_call", (event) => event.input?.block ? { block: true, reason: "synthetic block" } : undefined);
  pi.registerTool({ name: "secure_echo", label: "Echo", description: "Echo a required value",
    parameters: Type.Object({ value: Type.String(), block: Type.Optional(Type.Boolean()) }),
    async execute(_id, args) { return { content: [{ type: "text", text: args.value }], details: { ends } }; }
  });
  pi.registerCommand("fixture-command", { description: "Fixture command", async handler() {} });
}
`);
    const approvals = new ExtensionApprovalStore(root, agent);
    await approvals.approve(extension);
    host = await PiRuntimeHost.create(root);
    await run({ root, agent, extension, host, approvals });
  } finally {
    if (host) await host.shutdown();
    await fs.rm(root, { recursive: true, force: true });
  }
}

test("extension description exposes the exact native argument schema and active generation", async () => fixture(async ({ host }) => {
  const description = await host.describeExtensionTool("secure_echo");
  const schema = JSON.parse(description.parametersJson);
  assert.equal(schema.type, "object");
  assert.equal(schema.properties.value.type, "string");
  assert.ok(schema.required.includes("value"));
  assert.equal(description.active, true);
  assert.equal(description.generation, host.generation);
  await assert.rejects(host.describeExtensionTool("missing"), /unknown/u);
}));

test("valid long extension text is not silently truncated to 4000 characters", async () => fixture(async ({ host }) => {
  const value = "a".repeat(6000);
  const result = await host.callExtensionTool("secure_echo", { value });
  assert.equal(result.isError, false);
  assert.equal(result.text, value);
}));

test("a blocked native tool emits a balanced execution-end event", async () => fixture(async ({ host }) => {
  assert.equal((await host.callExtensionTool("secure_echo", { value: "blocked", block: true })).isError, true);
  const result = await host.callExtensionTool("secure_echo", { value: "allowed" });
  assert.equal(JSON.parse(result.detailsJson).ends, 1);
}));

test("approval revocation invalidates already loaded extension tools before further execution", async () => fixture(async ({ host, approvals, extension }) => {
  await approvals.revoke(extension);
  await assert.rejects(host.callExtensionTool("secure_echo", { value: "must not run" }), /approval|trust changed/u);
  await assert.rejects(host.callCommand("fixture-command", ""), /closed|not initialized/u);
}));

test("changed code cannot execute under the old loaded approval", async () => fixture(async ({ host, extension }) => {
  await fs.appendFile(extension, "\n// edited after approval\n");
  await assert.rejects(host.callExtensionTool("secure_echo", { value: "must not run" }), /approval|content/u);
}));

test("shutdown prevents further commands, descriptions and reload", async () => fixture(async ({ host }) => {
  await host.shutdown();
  await assert.rejects(host.callCommand("fixture-command", ""), /closed/u);
  await assert.rejects(host.describeExtensionTool("secure_echo"), /closed/u);
  await assert.rejects(host.reload(), /closed/u);
}));
