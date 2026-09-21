import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import path from 'node:path';
import { ROOT, TESTED_AGENT_BROWSER_VERSION, type Config } from './config.js';
import { BrowserFault, UncertainAction, record } from './errors.js';
import { inheritedEnvironment, inspectNativeConfig, type NativeSelection } from './native-config.js';

export interface Backend {
  readonly mode: string;
  version(): Promise<string>;
  readonly ownership?: 'external' | 'plugin' | 'unknown';
  configuration?(): Record<string, unknown>;
  run(tokens: string[], effect?: boolean, attach?: boolean): Promise<Record<string, unknown>>;
  shutdown(): Promise<void>;
}

export function childEnvironment(input: NodeJS.ProcessEnv, native = false): NodeJS.ProcessEnv {
  return inheritedEnvironment(input, native);
}

export interface ProcessOutput { stdout: string; exitCode: number | null; }
export async function boundedProcess(executable: string, args: string[], input: string,
  env: NodeJS.ProcessEnv, timeoutMs = 14000, limit = 524288, cwd = ROOT): Promise<ProcessOutput> {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { cwd, env, shell: false, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    let size = 0, finished = false;
    const chunks: Buffer[] = [];
    const fail = (error: Error) => {
      if (finished) return;
      finished = true; clearTimeout(timer); child.kill('SIGKILL'); reject(error);
    };
    const timer = setTimeout(() => fail(new BrowserFault('backend_timeout', 'Browser command exceeded its bounded timeout.')), timeoutMs);
    child.stdout.on('data', (chunk: Buffer) => {
      size += chunk.length;
      if (size > limit) fail(new BrowserFault('backend_output_limit', 'Browser command exceeded its output limit.'));
      else chunks.push(chunk);
    });
    // Drain but never reflect backend stderr, argv echoes, credentials, or page text.
    child.stderr.on('data', () => {});
    child.stdin.on('error', () => {});
    child.on('error', () => fail(new BrowserFault('backend_unavailable', 'Could not start the configured agent-browser executable.')));
    child.on('close', exitCode => {
      if (finished) return;
      finished = true; clearTimeout(timer);
      resolve({ stdout: Buffer.concat(chunks).toString('utf8'), exitCode });
    });
    child.stdin.end(input);
  });
}

export function decodeBatch(stdout: string): Record<string, unknown> {
  let value: unknown;
  try { value = JSON.parse(stdout); } catch { throw new BrowserFault('backend_protocol', 'Malformed agent-browser response.'); }
  // Before a batch is dispatched, auto-connect may return a top-level error.
  if (record(value) && value.success === false) return value;
  if (!Array.isArray(value) || value.length !== 1 || !record(value[0]) || typeof value[0].success !== 'boolean') {
    throw new BrowserFault('backend_protocol', 'Unexpected agent-browser batch response.');
  }
  return value[0];
}

export class NativeBackend implements Backend {
  readonly mode: string;
  // Namespaces and session names both enter macOS's 103-byte UNIX socket path.
  // Keep the two opaque components short; still isolate every provider instance.
  readonly session = 'wcb-' + randomBytes(6).toString('hex');
  private readonly env: NodeJS.ProcessEnv;
  private readonly cleanupEnv: NodeJS.ProcessEnv;
  private readonly cwd: string;
  private selection: NativeSelection | undefined;
  private started = false;
  private launchHash: unknown;
  private poisoned = false;
  private touched = false;
  private checkedVersion: string | undefined;
  constructor(private readonly config: Config, private readonly execute: typeof boundedProcess = boundedProcess,
    environment: NodeJS.ProcessEnv = process.env) {
    this.mode = config.connection === 'native' ? 'native' : config.connection === 'auto' ? 'auto' : 'local-cdp';
    this.env = childEnvironment(environment, this.mode === 'native');
    this.cleanupEnv = childEnvironment(environment);
    this.cwd = config.workingDirectory ?? process.cwd();
  }

  get ownership(): 'external' | 'plugin' | 'unknown' {
    return this.mode === 'native' ? this.selection?.ownership ?? 'unknown' : 'external';
  }

  configuration(): Record<string, unknown> {
    if (this.mode !== 'native') return { configuration_mode: 'isolated_attachment', browser_ownership: 'external' };
    return this.observeConfiguration().summary;
  }

  private observeConfiguration(): NativeSelection {
    const current = inspectNativeConfig(this.config, this.env, this.cwd);
    if (this.started && this.selection?.fingerprint !== current.fingerprint) {
      throw new BrowserFault('configuration_changed', 'Native configuration changed while the browser was connected.',
        'Disconnect, then connect again. Reload the plugin for changes to the Runner-prepared shell environment.');
    }
    this.selection = current;
    return current;
  }

  private cleanupArgs(): string[] {
    return ['--config', path.join(ROOT, 'agent-browser.defaults.json'), '--session', this.session,
      '--namespace', this.session, '--json'];
  }

  private async assertExistingSession(): Promise<void> {
    if (!this.started) return;
    const p = await this.execute(this.config.executable, [...this.cleanupArgs(), 'session', 'info'], '',
      this.cleanupEnv, 5000, 32768, this.cwd);
    let value: unknown;
    try { value = JSON.parse(p.stdout); } catch { throw new BrowserFault('browser_session_lost', 'Could not verify the existing browser session.'); }
    if (p.exitCode !== 0 || !record(value) || value.success !== true || !record(value.data) ||
      value.data.active !== true || !record(value.data.runtime) ||
      (this.launchHash !== undefined && value.data.runtime.launchHash !== this.launchHash)) {
      throw new BrowserFault('browser_session_lost', 'The previous browser session ended or was replaced. No new browser was launched.',
        'Disconnect and explicitly reconnect; old page and snapshot IDs must not be reused.');
    }
  }

  async version(): Promise<string> {
    if (this.checkedVersion) return this.checkedVersion;
    const p = await this.execute(this.config.executable, ['--version'], '', this.cleanupEnv, 5000, 4096, this.cwd);
    const match = /^agent-browser\s+(\d+\.\d+\.\d+)\s*$/.exec(p.stdout.trim());
    if (p.exitCode !== 0 || !match || match[1] !== TESTED_AGENT_BROWSER_VERSION) {
      throw new BrowserFault('unsupported_backend_version', `This release is tested with agent-browser ${TESTED_AGENT_BROWSER_VERSION}; select that version before connecting.`);
    }
    return this.checkedVersion = match[1];
  }

  async run(tokens: string[], effect = false, attach = true): Promise<Record<string, unknown>> {
    if (this.poisoned) throw new UncertainAction();
    if (attach && this.mode === 'native') this.observeConfiguration();
    if (attach) await this.assertExistingSession();
    const native = attach && this.mode === 'native';
    const args = native ? ['--session', this.session, '--namespace', this.session, '--json',
      '--pin-tab', '--no-auto-dialog', '--restore-save', 'never', '--idle-timeout', '10m',
      '--screenshot-format', 'png', '--annotate', 'false', '--debug', 'false'] : this.cleanupArgs();
    if (native && this.config.agentBrowserConfig) args.push('--config', this.config.agentBrowserConfig);
    if (attach && !native) args.push(...(this.config.connection === 'auto' ? ['--auto-connect'] : ['--cdp', this.config.connection]));
    args.push('batch', '--bail');
    try {
      // Only command arrays authored inside this plugin are accepted. Values travel
      // over JSON stdin, never as shell text or globally parsed CLI flag arguments.
      this.touched = true;
      const p = await this.execute(this.config.executable, args, JSON.stringify([tokens]), native ? this.env : this.cleanupEnv,
        attach && !this.started ? 45000 : 14000, 524288, this.cwd);
      const r = decodeBatch(p.stdout);
      if (r.success !== true) {
        const message = typeof r.error === 'string' ? r.error : '';
        if (message.includes('No running Chrome instance found')) {
          throw new BrowserFault('chrome_not_authorized', 'No authorized local Chrome debugging connection is available.',
            'In your normal Chrome open chrome://inspect/#remote-debugging, enable remote debugging, then allow the Chrome connection prompt and call browser_connect again. The plugin never enables it for you.');
        }
        const pageGone = /tab_gone|tab not found|unknown tab|no tab found|target.*not found/i.test(message) || r.code === 'tab_gone';
        // Once an effect was dispatched, even a structured "tab gone" response
        // cannot prove the action did not happen immediately before the target
        // disappeared. Preserve OutcomeUnknown rather than presenting a retry-safe
        // application error. Observations may still report page_gone directly.
        if (effect) { this.poisoned = true; throw new UncertainAction(); }
        if (pageGone) {
          throw new BrowserFault('page_gone', 'The selected tab no longer exists.', 'List tabs and explicitly select the intended page; do not reuse its old snapshot.');
        }
        throw new BrowserFault('browser_observation_failed', 'The browser observation could not be completed.', 'Check Chrome connection/permission/dialog state, then obtain a fresh observation.');
      }
      if (p.exitCode !== 0 || !record(r.result)) throw new BrowserFault('backend_protocol', 'Unexpected browser command result.');
      if (attach) {
        this.started = true;
        if (record(r.result.lifecycle) && record(r.result.lifecycle.effectiveLaunch)) {
          const nextHash = r.result.lifecycle.effectiveLaunch.launchHash;
          if (this.launchHash !== undefined && this.launchHash !== nextHash) {
            this.poisoned = true; throw new UncertainAction();
          }
          this.launchHash = nextHash;
        }
      } else { this.started = false; this.launchHash = undefined; this.selection = undefined; this.touched = false; }
      return r.result;
    } catch (error) {
      if (error instanceof BrowserFault && ['chrome_not_authorized', 'backend_unavailable', 'page_gone'].includes(error.code)) throw error;
      if (effect) { this.poisoned = true; throw new UncertainAction(); }
      throw error;
    }
  }

  async shutdown(): Promise<void> {
    if (!this.touched || this.poisoned) return;
    this.touched = false;
    const args = [...this.cleanupArgs(), 'batch', '--bail'];
    // No --cdp/--auto-connect here: cleanup must not attach or create a new tab.
    // The installed CLI owns lifecycle: close detaches CDP, but closes its own
    // launched browser and cleans its named-profile copy. Never use close --all.
    await this.execute(this.config.executable, args, JSON.stringify([['close']]), this.cleanupEnv, 5000, 16384, this.cwd).catch(() => {});
  }
}
