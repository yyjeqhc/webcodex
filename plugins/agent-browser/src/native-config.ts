import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { type Config, localConnection } from './config.js';
import { BrowserFault, record } from './errors.js';

// These are protocol/session ownership controls, not user browser preferences.
// Everything else AGENT_BROWSER_* is passed through to the installed CLI.
export const RESERVED_ENV = new Set([
  'AGENT_BROWSER_SESSION', 'AGENT_BROWSER_NAMESPACE', 'AGENT_BROWSER_SOCKET_DIR',
  'AGENT_BROWSER_PIN_TAB', 'AGENT_BROWSER_NO_AUTO_DIALOG', 'AGENT_BROWSER_RESTORE_SAVE',
  'AGENT_BROWSER_DEFAULT_TIMEOUT', 'AGENT_BROWSER_IDLE_TIMEOUT_MS', 'AGENT_BROWSER_JSON',
  'AGENT_BROWSER_SCREENSHOT_FORMAT', 'AGENT_BROWSER_ANNOTATE', 'AGENT_BROWSER_DEBUG',
]);
const BASE_ENV = new Set(['HOME', 'PATH', 'USER', 'TMPDIR', 'TMP', 'TEMP', 'SystemRoot', 'SYSTEMROOT',
  'WINDIR', 'USERPROFILE', 'LOCALAPPDATA', 'APPDATA', 'DISPLAY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR']);
const PROXY_ENV = new Set(['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY', 'http_proxy', 'https_proxy', 'all_proxy', 'no_proxy']);

export function inheritedEnvironment(input: NodeJS.ProcessEnv, native: boolean): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {};
  for (const [key, value] of Object.entries(input)) {
    if (value !== undefined && (BASE_ENV.has(key) || (native && (PROXY_ENV.has(key) ||
      (key.startsWith('AGENT_BROWSER_') && !RESERVED_ENV.has(key)))))) env[key] = value;
  }
  // Stable protocol bounds and a short idle lease; never forward WebCodex or
  // unrelated account credentials. Browser proxy credentials remain local.
  env.AGENT_BROWSER_DEFAULT_TIMEOUT = '8000';
  if (!native) env.NO_PROXY = 'localhost,127.0.0.1,::1';
  return env;
}

export interface NativeSelection {
  fingerprint: string;
  ownership: 'external' | 'plugin';
  summary: { [key: string]: unknown };
}

// Validation/diagnostics only: NEVER serialize a merged replacement config.
// The installed agent-browser still reads the original files and applies its
// native precedence, array merging, environment, and relative-path semantics.
export function inspectNativeConfig(config: Config, env: NodeJS.ProcessEnv, cwd: string): NativeSelection {
  try { if (!fs.statSync(cwd).isDirectory()) throw new Error(); }
  catch { throw new BrowserFault('invalid_working_directory', 'The operator-selected browser working directory is unavailable.'); }
  const home = env.HOME || env.USERPROFILE;
  if (!home) throw new BrowserFault('native_config_unavailable', 'The Runner must supply HOME or USERPROFILE for native configuration.');
  const explicit = config.agentBrowserConfig || env.AGENT_BROWSER_CONFIG;
  const files = explicit ? [path.resolve(cwd, explicit)] : [path.join(home, '.agent-browser/config.json'), path.join(cwd, 'agent-browser.json')];
  const digest = createHash('sha256').update(cwd);
  const objects: Record<string, unknown>[] = [];
  const present: boolean[] = [];
  for (const filename of files) {
    try {
      const stat = fs.statSync(filename);
      if (!stat.isFile() || stat.size > 262144) throw new Error();
      const text = fs.readFileSync(filename, 'utf8');
      const value: unknown = JSON.parse(text);
      if (!record(value)) throw new Error();
      objects.push(value); present.push(true); digest.update(filename).update(text);
    } catch (error) {
      if (!explicit && (error as NodeJS.ErrnoException).code === 'ENOENT') {
        objects.push({}); present.push(false); digest.update(filename).update('absent');
      } else throw new BrowserFault('invalid_native_config', 'A native Agent Browser config is missing, malformed, unreadable, or oversized.',
        'Repair the operator-selected config; the plugin does not silently fall back to an empty browser.');
    }
  }
  const effective: Record<string, unknown> = Object.assign({}, ...objects);
  // Only inspect the small set that determines local/external ownership. CLI is
  // authoritative for all other settings. Do not log values or profile paths.
  for (const [variable, key] of Object.entries({ AGENT_BROWSER_PROFILE: 'profile', AGENT_BROWSER_CDP: 'cdp',
    AGENT_BROWSER_PROVIDER: 'provider', AGENT_BROWSER_ENGINE: 'engine',
    AGENT_BROWSER_RESTORE: 'restore', AGENT_BROWSER_SESSION_NAME: 'sessionName' })) {
    if (env[variable] !== undefined) effective[key] = env[variable];
  }
  if (env.AGENT_BROWSER_AUTO_CONNECT !== undefined) {
    if (!['1', 'true', '0', 'false'].includes(env.AGENT_BROWSER_AUTO_CONNECT)) {
      throw new BrowserFault('invalid_native_config', 'AGENT_BROWSER_AUTO_CONNECT must be 1/true or 0/false.');
    }
    effective.autoConnect = ['1', 'true'].includes(env.AGENT_BROWSER_AUTO_CONNECT);
  }
  if (effective.autoConnect !== undefined && typeof effective.autoConnect !== 'boolean') {
    throw new BrowserFault('invalid_native_config', 'autoConnect must be a boolean.');
  }
  for (const key of ['profile', 'cdp', 'provider', 'engine']) {
    if (effective[key] !== undefined && typeof effective[key] !== 'string') {
      throw new BrowserFault('invalid_native_config', `Native ${key} must be a string.`);
    }
  }
  if (effective.restore || effective.sessionName) {
    throw new BrowserFault('unsupported_native_restore', 'Shared restore/session-name keys are incompatible with this plugin’s isolated namespace.',
      'Use a native profile or an explicit state file. Shared restore keys are not silently redirected to an empty namespace.');
  }
  if (effective.provider || (effective.engine && effective.engine !== 'chrome')) {
    throw new BrowserFault('unsupported_native_backend', 'This plugin supports native local Chrome and local CDP, not cloud providers or other engines.',
      'Choose compatible native configuration explicitly; no provider setting is silently discarded.');
  }
  const pluginEntries = env.AGENT_BROWSER_PLUGINS !== undefined ? (() => {
    try { return JSON.parse(env.AGENT_BROWSER_PLUGINS!); } catch { throw new BrowserFault('invalid_native_config', 'Invalid native plugin registry.'); }
  })() : objects.flatMap(o => Array.isArray(o.plugins) ? o.plugins : []);
  if (!Array.isArray(pluginEntries)) throw new BrowserFault('invalid_native_config', 'Native plugin registry must be an array.');
  if (pluginEntries.some(p => record(p) && Array.isArray(p.capabilities) &&
    p.capabilities.some(c => c === 'browser.provider' || c === 'launch.mutate'))) {
    throw new BrowserFault('unsupported_native_backend', 'Native plugins that replace or mutate browser launch need separate compatibility validation.');
  }
  if (effective.cdp) localConnection(effective.cdp);
  if (effective.cdp === 'auto' || effective.cdp === 'native') throw new BrowserFault('invalid_native_config', 'cdp must identify a local debugging endpoint.');
  if ([!!effective.profile, !!effective.cdp, effective.autoConnect === true].filter(Boolean).length > 1) {
    throw new BrowserFault('conflicting_native_config', 'Profile, CDP, and auto-connect are mutually exclusive in this bridge.',
      'Choose one connection mode in native config instead of relying on silent precedence between browser modes.');
  }
  const profile = typeof effective.profile === 'string' ? effective.profile : '';
  const named = !!profile && !/[\\/]/.test(profile) && !profile.startsWith('.') && !profile.startsWith('~');
  const ownership = effective.cdp || effective.autoConnect === true ? 'external' : 'plugin';
  return { fingerprint: digest.digest('hex'), ownership, summary: {
    configuration_mode: 'native', configuration_source: explicit ? 'explicit_native_config' : 'native_user_and_project_discovery',
    user_config_present: explicit ? null : present[0], project_config_present: explicit ? null : present[1],
    explicit_config_present: explicit ? present[0] : false,
    profile_configured: !!profile, profile_kind: !profile ? 'none' : named ? 'named_chrome_profile' : 'persistent_directory',
    profile_name: named && profile.length <= 64 ? profile.replace(/[\u0000-\u001f\u007f]/g, '') : null,
    inherited_environment_keys: Object.keys(env).filter(k => k.startsWith('AGENT_BROWSER_') && !RESERVED_ENV.has(k)).sort(),
    browser_ownership: ownership, working_directory_source: config.workingDirectory ? 'operator_config' : 'runner_provider_cwd',
    protocol_overrides: ['session', 'namespace', 'pinTab', 'json', 'noAutoDialog', 'restoreSave=never',
      'idleTimeout=10m', 'defaultTimeout=8000', 'screenshotFormat=png', 'annotate=false', 'debug=false'],
    note: 'Native config is loaded by agent-browser itself. Shell variables come from the Runner prepared environment, not repeated shell execution.'
  } };
}
