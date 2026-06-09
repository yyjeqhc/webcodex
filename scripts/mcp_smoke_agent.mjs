#!/usr/bin/env node
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import readline from "node:readline";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const timeoutMs = Number(process.env.PRIVATE_DROP_MCP_SMOKE_TIMEOUT_MS || "120000");

function commandSpec() {
  if (process.env.PRIVATE_DROP_MCP_COMMAND) {
    const [command, ...args] = process.env.PRIVATE_DROP_MCP_COMMAND.split(" ");
    return { command, args };
  }
  const debugBin = resolve(repoRoot, "target/debug/private-drop-mcp");
  if (existsSync(debugBin)) {
    return { command: debugBin, args: [] };
  }
  return { command: "cargo", args: ["run", "--quiet", "--bin", "private-drop-mcp"] };
}

const { command, args } = commandSpec();
const child = spawn(command, args, {
  cwd: repoRoot,
  env: process.env,
  stdio: ["pipe", "pipe", "pipe"],
});

let stderr = "";
child.stderr.on("data", (chunk) => {
  stderr += chunk.toString();
});

const pending = new Map();
const stdout = readline.createInterface({ input: child.stdout });
stdout.on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch (error) {
    fail(`Invalid JSON-RPC output: ${error.message}\nLine: ${line}`);
  }
  for (const item of Array.isArray(message) ? message : [message]) {
    const entry = pending.get(item.id);
    if (!entry) continue;
    pending.delete(item.id);
    clearTimeout(entry.timer);
    if (item.error) {
      entry.reject(new Error(JSON.stringify(item.error)));
    } else {
      entry.resolve(item.result);
    }
  }
});

child.on("exit", (code, signal) => {
  for (const [id, entry] of pending) {
    clearTimeout(entry.timer);
    entry.reject(new Error(`MCP process exited before response ${id}: code=${code} signal=${signal}`));
  }
  pending.clear();
});

let nextId = 1;

function request(method, params = {}) {
  const id = nextId++;
  const message = { jsonrpc: "2.0", id, method, params };
  child.stdin.write(`${JSON.stringify(message)}\n`);
  return new Promise((resolvePromise, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Timed out waiting for ${method}`));
    }, timeoutMs);
    pending.set(id, { resolve: resolvePromise, reject, timer });
  });
}

function notification(method, params = {}) {
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method, params })}\n`);
}

function assert(condition, message) {
  if (!condition) {
    fail(message);
  }
}

function fail(message) {
  child.kill("SIGTERM");
  const suffix = stderr.trim() ? `\n\nstderr:\n${stderr.trim()}` : "";
  console.error(`${message}${suffix}`);
  process.exit(1);
}

function names(list) {
  return list.map((item) => item.name).sort();
}

try {
  const init = await request("initialize", {
    protocolVersion: "2025-06-18",
    clientInfo: { name: "private-drop-mcp-smoke-agent", version: "0.1.0" },
    capabilities: {},
  });
  assert(init.protocolVersion, "initialize did not return protocolVersion");
  assert(init.capabilities?.tools, "initialize did not advertise tools");
  assert(init.capabilities?.resources, "initialize did not advertise resources");
  assert(init.capabilities?.prompts, "initialize did not advertise prompts");
  notification("notifications/initialized");

  const toolList = await request("tools/list");
  const toolNames = names(toolList.tools || []);
  for (const name of [
    "list_projects",
    "get_project_context_batch",
    "apply_project_edit",
    "run_job_op",
    "action_session_op",
  ]) {
    assert(toolNames.includes(name), `tools/list missing ${name}`);
  }

  const resourceList = await request("resources/list");
  const resourceUris = (resourceList.resources || []).map((item) => item.uri).sort();
  assert(resourceUris.includes("private-drop://workflow"), "resources/list missing workflow resource");
  assert(resourceUris.includes("private-drop://schema/gpt"), "resources/list missing GPT schema resource");

  const workflow = await request("resources/read", { uri: "private-drop://workflow" });
  assert(workflow.contents?.[0]?.text?.includes("get_project_context_batch"), "workflow resource text is incomplete");

  const promptList = await request("prompts/list");
  const promptNames = names(promptList.prompts || []);
  assert(promptNames.includes("project_startup"), "prompts/list missing project_startup");
  assert(promptNames.includes("long_job_workflow"), "prompts/list missing long_job_workflow");

  const prompt = await request("prompts/get", {
    name: "safe_edit_workflow",
    arguments: { project: "private-drop", paths: "README.md, src/main.rs" },
  });
  assert(prompt.messages?.[0]?.content?.text?.includes("expected_fingerprints"), "safe_edit_workflow prompt is incomplete");

  if (process.env.PRIVATE_DROP_MCP_SMOKE_CALL_HTTP === "1") {
    const result = await request("tools/call", {
      name: "list_projects",
      arguments: { action_session_id: "mcp-smoke-agent" },
    });
    assert(result.content?.[0]?.type === "text", "list_projects did not return text content");
  }

  child.stdin.end();
  child.kill("SIGTERM");
  console.log(`MCP smoke agent passed (${toolNames.length} tools, ${resourceUris.length} resources, ${promptNames.length} prompts).`);
} catch (error) {
  fail(error.stack || error.message);
}
