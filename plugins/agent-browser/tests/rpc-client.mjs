import { spawn } from 'node:child_process';
import readline from 'node:readline';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { sleep } from './chrome-fixture.mjs';
const root = fileURLToPath(new URL('../', import.meta.url));

export class PluginClient {
  nextId = 0;
  pending = new Map();
  stderr = '';
  constructor(configPath, { env = process.env, cwd = root } = {}) {
    this.child = spawn(process.execPath, [path.join(root, 'dist/plugin.js'), ...(configPath ? ['--config', configPath] : [])],
      { cwd, env, stdio: ['pipe', 'pipe', 'pipe'] });
    const lines = readline.createInterface({ input: this.child.stdout });
    lines.on('line', line => {
      try {
        const value = JSON.parse(line), entry = this.pending.get(value.id);
        if (!entry) throw new Error('Unexpected JSON-RPC response id');
        this.pending.delete(value.id); clearTimeout(entry.timer);
        if (value.error) entry.reject(new Error('JSON-RPC error: ' + value.error.code));
        else entry.resolve(value.result);
      } catch (error) { for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(error); } this.pending.clear(); }
    });
    this.child.stderr.on('data', bytes => { this.stderr = (this.stderr + bytes).slice(-2048); });
    this.child.stdin.on('error', () => {});
    this.child.on('error', error => this.rejectAll(error));
    this.child.on('exit', code => this.rejectAll(new Error('Plugin exited before reply (code ' + code + ').')));
  }
  rejectAll(error) { for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(error); } this.pending.clear(); }
  request(method, params) {
    if (this.child.exitCode !== null || this.child.signalCode !== null || this.child.stdin.destroyed) {
      return Promise.reject(new Error('Plugin is no longer running.'));
    }
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error('Plugin RPC timeout for ' + method)); }, 110000);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    });
  }
  async init() { return this.request('initialize', { protocolVersion: 'webcodex-plugin-v1' }); }
  async raw(name, args = {}) { return this.request('tools/call', { name, arguments: args }); }
  async call(name, args = {}) {
    const r = await this.raw(name, args);
    if (r.isError) throw new Error(name + ': ' + r.structuredContent?.code + ' — ' + r.structuredContent?.message);
    return r.structuredContent;
  }
  async stop() {
    this.child.stdin.end();
    for (let i = 0; i < 50; i++) { if (this.child.exitCode !== null || this.child.signalCode !== null) return; await sleep(100); }
    this.child.kill('SIGTERM');
    for (let i = 0; i < 40; i++) { if (this.child.exitCode !== null || this.child.signalCode !== null) return; await sleep(100); }
    this.child.kill('SIGKILL');
  }
}

export async function cdpRequest(fixture, pageUrl, method, params = {}) {
  const targets = await (await fetch(`http://127.0.0.1:${fixture.port}/json/list`)).json();
  const target = targets.find(t => t.type === 'page' && t.url === pageUrl);
  if (!target) throw new Error('Expected isolated fixture tab is missing.');
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    const timer = setTimeout(() => { socket.close(); reject(new Error('Test CDP timeout')); }, 10000);
    socket.onopen = () => socket.send(JSON.stringify({ id: 1, method, params }));
    socket.onerror = () => { clearTimeout(timer); reject(new Error('Test CDP connection error')); };
    socket.onmessage = event => {
      const r = JSON.parse(String(event.data));
      if (r.id !== 1) return;
      clearTimeout(timer); socket.close();
      if (r.error) reject(new Error('Test CDP protocol error')); else resolve(r.result);
    };
  });
}
export async function evaluate(fixture, pageUrl, expression) {
  const result = await cdpRequest(fixture, pageUrl, 'Runtime.evaluate', { expression, returnByValue: true });
  if (result.exceptionDetails) throw new Error('Isolated fixture evaluation failed');
  return result.result.value;
}
