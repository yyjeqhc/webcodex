import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { BrowserFault, record } from './errors.js';

export const ROOT = fileURLToPath(new URL('../', import.meta.url));
export const TESTED_AGENT_BROWSER_VERSION = '0.38.1';
export interface Config { executable: string; connection: string; workingDirectory?: string; agentBrowserConfig?: string; }

export function localConnection(value: unknown): string {
  if (value === 'auto' || value === 'native') return value;
  if (typeof value !== 'string') throw new BrowserFault('invalid_config', 'Connection must be auto or a local debugging port.');
  if (/^[1-9]\d{0,4}$/.test(value) && Number(value) <= 65535) return value;
  // Operator-only configuration, never tool input. Restrict to literal loopback;
  // do not let hostnames/DNS, credentials, or remote endpoints enter this bridge.
  try {
    const u = new URL(value);
    if (u.protocol === 'ws:' && ['127.0.0.1', '[::1]'].includes(u.hostname) && u.port &&
        !u.username && !u.password && !u.search && !u.hash && /^\/devtools\/browser\/[A-Za-z0-9-]+$/.test(u.pathname)) return u.href;
  } catch {}
  throw new BrowserFault('invalid_config', 'Only auto, a loopback port, or a literal-loopback browser WebSocket is supported.');
}

export function loadConfig(argv = process.argv.slice(2)): Config {
  if (argv.length !== 0 && (argv.length !== 2 || argv[0] !== '--config')) {
    throw new BrowserFault('invalid_config', 'Usage: node dist/plugin.js [--config /absolute/config.local.json]');
  }
  let input: unknown = {};
  if (argv.length === 2) {
    const filename = argv[1]!;
    if (!path.isAbsolute(filename)) throw new BrowserFault('invalid_config', 'Plugin config path must be absolute.');
    const stat = fs.statSync(filename);
    if (!stat.isFile() || stat.size > 16384) throw new BrowserFault('invalid_config', 'Invalid plugin config file.');
    input = JSON.parse(fs.readFileSync(filename, 'utf8'));
  }
  if (!record(input) || Object.keys(input).some(k => !['executable', 'connection', 'workingDirectory', 'agentBrowserConfig'].includes(k))) {
    throw new BrowserFault('invalid_config', 'Unknown plugin configuration property.');
  }
  const executable = input.executable ?? 'agent-browser';
  if (typeof executable !== 'string' || executable.length > 2048 || /[\0\r\n]/.test(executable) ||
      (executable !== 'agent-browser' && !path.isAbsolute(executable))) {
    throw new BrowserFault('invalid_config', 'Executable must be agent-browser or an absolute native executable path.');
  }
  const config: Config = { executable, connection: localConnection(input.connection ?? 'native') };
  for (const key of ['workingDirectory', 'agentBrowserConfig'] as const) {
    const value = input[key];
    if (value !== undefined) {
      if (typeof value !== 'string' || !path.isAbsolute(value) || value.length > 4096 || /[\0\r\n]/.test(value)) {
        throw new BrowserFault('invalid_config', `${key} must be an absolute operator-configured path.`);
      }
      config[key] = value;
    }
  }
  if (config.connection !== 'native' && config.agentBrowserConfig) {
    throw new BrowserFault('invalid_config', 'agentBrowserConfig is only used in native mode; explicit attachment uses isolated launch configuration.');
  }
  return config;
}

export function navigationUrl(value: unknown): string {
  if (typeof value !== 'string' || Buffer.byteLength(value) > 8192 || /[\u0000-\u0020\u007f]/.test(value)) {
    throw new BrowserFault('invalid_url', 'Provide an absolute HTTP(S) URL without control characters or spaces.');
  }
  let u: URL;
  try { u = new URL(value); } catch { throw new BrowserFault('invalid_url', 'Provide an absolute HTTP(S) URL.'); }
  if (!['http:', 'https:'].includes(u.protocol) || !u.hostname || u.username || u.password) {
    throw new BrowserFault('invalid_url', 'Only HTTP(S) URLs without embedded credentials are allowed.');
  }
  return u.href;
}

export function supportedPage(url: string): boolean {
  if (url === 'about:blank') return true;
  try { navigationUrl(url); return true; } catch { return false; }
}

export function displayUrl(value: string): string {
  if (value === 'about:blank') return value;
  try {
    const u = new URL(value);
    if (!['http:', 'https:'].includes(u.protocol)) return '(unsupported browser page)';
    u.username = ''; u.password = ''; u.search = ''; u.hash = '';
    return u.href.slice(0, 512);
  } catch { return '(unavailable)'; }
}
