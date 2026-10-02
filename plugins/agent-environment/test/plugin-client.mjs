import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

/** Test-only JSON-RPC client. Production transport remains the Plugin SDK/Runner. */
export class PluginClient {
  constructor({ cwd, agentDir, stateDir }) {
    this.nextId = 0;
    this.pending = new Map();
    this.stderr = "";
    this.child = spawn(process.execPath, [fileURLToPath(new URL("../src/plugin.mjs", import.meta.url)), "--agent-dir", agentDir, "--state-dir", stateDir], {
      cwd, stdio: ["pipe", "pipe", "pipe"],
      env: { PATH: process.env.PATH, HOME: stateDir, LANG: "C.UTF-8", PI_OFFLINE: "1", PI_SKIP_VERSION_CHECK: "1" },
    });
    this.exited = once(this.child, "exit");
    this.child.stderr.on("data", (chunk) => { this.stderr = (this.stderr + chunk.toString()).slice(-16000); });
    this.lines = createInterface({ input: this.child.stdout, terminal: false });
    this.lines.on("line", (line) => {
      let response;
      try { response = JSON.parse(line); } catch { this.fail(new Error("Non-JSON Plugin stdout")); return; }
      const pending = this.pending.get(response.id);
      if (!pending) { this.fail(new Error("Uncorrelated Plugin response")); return; }
      this.pending.delete(response.id);
      clearTimeout(pending.timer);
      response.error ? pending.reject(new Error(JSON.stringify(response.error))) : pending.resolve(response.result);
    });
    this.child.on("error", (error) => this.fail(error));
    this.child.on("exit", (code) => this.fail(new Error(`Plugin exited ${code}: ${this.stderr}`)));
  }

  fail(error) { for (const item of this.pending.values()) { clearTimeout(item.timer); item.reject(error); } this.pending.clear(); }
  rpc(method, params = {}, timeout = 30000) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`Plugin request timed out: ${method}; ${this.stderr}`)); }, timeout);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }
  async initialize() {
    await this.rpc("initialize", { protocolVersion: "webcodex-plugin-v1" });
    return this.rpc("tools/list");
  }
  async action(input) {
    const result = await this.rpc("tools/call", { name: "agent_environment", arguments: input });
    return result.structuredContent;
  }
  async close() {
    this.child.stdin.end();
    const timer = setTimeout(() => this.child.kill("SIGKILL"), 3000);
    try { await this.exited; } finally { clearTimeout(timer); this.lines.close(); }
  }
}
