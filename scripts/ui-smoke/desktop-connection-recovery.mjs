import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Renderer-only evidence. The loopback fixture supplies all observations and
// captures all commands; no native service or network route is changed.
const output = path.resolve(import.meta.dirname, '../../artifacts/desktop-workflow');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const cases = [
  { name: 'openai-direct', error: 'tunnel_unavailable', reason: 'tunnel_control_plane_unreachable', title: 'Cannot reach the OpenAI tunnel service', section: 'network', auto: false },
  { name: 'openai-proxy', error: 'tunnel_unavailable', reason: 'tunnel_control_plane_probe_failed', title: 'Cannot reach the OpenAI tunnel service', section: 'network', auto: true },
  { name: 'local-mcp', error: 'local_mcp_unavailable', reason: 'local_mcp_unavailable', title: 'Local MCP service is unreachable', section: 'runtime' },
  { name: 'download', error: 'tunnel_unavailable', reason: 'tunnel_client_download_failed', title: 'Tunnel client download failed', section: 'network' },
  { name: 'install', error: 'tunnel_unavailable', reason: 'tunnel_client_install_failed', title: 'Tunnel client installation failed', section: 'diagnostics' },
  { name: 'health-stale', error: 'health_stale', reason: 'tunnel_health_stale', title: 'Tunnel health reports stopped arriving', section: 'diagnostics' },
  { name: 'no-error-code', error: null, reason: null, title: 'Connection unavailable', section: 'diagnostics' },
];
let browser;
const errors = [], results = [];
try {
  browser = await chromium.launch({ headless: true });
  for (const width of [1440, 1280, 1024, 768, 390]) {
    for (const theme of ['light', 'dark']) {
      const context = await browser.newContext({ viewport: { width, height: 1000 }, colorScheme: theme, reducedMotion: 'reduce' });
      const page = await context.newPage();
      page.on('pageerror', error => errors.push(error.message));
      let current;
      await page.route('**/__fixture/desktop-state*', async route => {
        const response = await route.fetch(), state = await response.json();
        const profile = { id: 'recovery-fixture', name: 'ChatGPT Work', tunnel_id: 'tunnel_fixture', credential_present: true,
          enabled: true, autostart: false, revision: 1, source: 'file', lifecycle: 'error', pid: null, health: 'degraded',
          last_error: current.error, ready: false, process_started: true, process_ready: true,
          tunnel_ready: current.name === 'local-mcp', local_mcp_ready: current.name !== 'local-mcp',
          reason_code: current.reason, failure_stage: null, auto_proxy_used: current.auto ?? null,
          runtime_directory: null, health_url: null, log_file: null, local_mcp_url: null, tunnel_client_pid: null, logs: [] };
        state.connections = { profiles: [profile], running: 0, needs_attention: 1, config_error: false };
        state.tunnel_proxy = { mode: current.auto ? 'auto' : 'direct', custom_url: null, effective_source: current.auto ? 'system' : 'direct', effective_proxy_present: Boolean(current.auto), system_proxy_detected: Boolean(current.auto) };
        await route.fulfill({ response, json: state });
      });
      for (const test of cases) {
        current = test;
        await page.goto(fixture.url + '/desktop/');
        await page.locator('[data-webcodex-action="navigate-connection"]').click();
        const card = page.locator('[data-tunnel-profile-id="recovery-fixture"]');
        await card.waitFor();
        assert((await card.innerText()).includes(test.title));
        assert.equal(await card.locator('.connection-recovery').isVisible(), true);
        assert.equal(await card.locator('.connection-details').getAttribute('open'), null);
        const dimensions = await page.evaluate(() => { const el = document.querySelector('.main-content'); return { viewport: innerWidth, document: document.documentElement.scrollWidth, client: el.clientWidth, scroll: el.scrollWidth }; });
        assert(dimensions.document <= width + 1 && dimensions.scroll <= dimensions.client + 1, JSON.stringify(dimensions));
        assert.equal((await card.innerText()).includes('automatically detected proxy'), Boolean(test.auto));
        await card.locator('.connection-recovery button').click();
        assert.equal(await page.locator(`#settings-tab-${test.section}`).getAttribute('aria-selected'), 'true');
        const effects = await page.evaluate(() => window.__fixtureCalls.filter(call => ['tunnel_profile_action', 'restart_owned_runner', 'update_tunnel_proxy'].includes(call.cmd)));
        assert.deepEqual(effects, [], 'opening recovery must never change processes or network routes');
        results.push({ width, theme, scenario: test.name });
        if (test.name === 'openai-direct') {
          // Capture the actual Chinese UI using the real locale control.
          await page.locator('#settings-tab-general').click();
          await page.locator('#desktop-settings-locale').selectOption('zh-CN');
          await page.locator('[data-webcodex-action="navigate-connection"]').click();
          await page.screenshot({ path: path.join(output, `connection-recovery-${width}-${theme}.png`), animations: 'disabled' });
        }
      }
      await context.close();
    }
  }
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ fixture: true, nativeBackend: false, checks: results.length }));
} finally {
  fs.writeFileSync(path.join(output, 'connection-recovery-report.json'), JSON.stringify({ fixture: true, nativeBackend: false, results, errors }, null, 2));
  if (browser) await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
