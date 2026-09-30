import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Rich, isolated observations for layout and wording. No native API or service
// controls run; every request remains inside the loopback fixture.
const output = path.resolve(import.meta.dirname, '../../artifacts/desktop-clarity');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const now = Date.now();
const projects = ['alpha', 'beta'].map(name => ({ id: `agent:fixture-runner:${name}`, name, path: `/fixture/${name}`, client_id: 'fixture-runner', connected: true, sessions: { active_sessions: 2, running_sessions: 1, latest_updated_at: Math.floor(now / 1000) } }));
const runners = [
  { client_id: 'fixture-runner', connected: true, status: 'online', computer_session_availability: false },
  { client_id: 'another-device', connected: true, status: 'online', computer_session_availability: null },
  { client_id: 'stale-device', connected: true, status: 'stale', computer_session_availability: true },
];
const sessions = Array.from({ length: 4 }, (_, index) => ({ session_id: `fixture-session-${index}`, project_id: projects[index % 2].id, title: ['Review the export workflow and verify the changes', 'Continue the integration tests', 'Update project instructions', 'Investigate a failed build'][index], updated_at: Math.floor(now / 1000) - index * 100, lifecycle: index === 2 ? 'closed' : 'active', running_call: index === 0, running_jobs: index === 1 ? 1 : 0, running_jobs_complete: true, overview: { attention: { open_todos: index === 0 ? 2 : 0, open_questions: index === 1 ? 1 : 0, open_risks: index === 3 ? 1 : 0 }, reported_progress: { text: ['Checking the export handler and its tests', 'Waiting for the remaining test results', 'Instructions saved', 'Build output needs review'][index], reported_at: Math.floor(now / 1000) }, validation: { state: index === 3 ? 'failed' : 'unknown', unresolved_failure_count: index === 3 ? 1 : 0 } } }));
const windows = Array.from({ length: 10 }, (_, index) => ({ client_window_key: `fixture-window-${index}`, source: 'chatgpt', last_project: projects[index % 2].id, last_seen_at_ms: now - index * 30_000, last_meaningful_activity_at_ms: now - index * 30_000, active_count: index === 0 ? 1 : 0, linked_session_count: 1 }));
const details = Object.fromEntries(windows.map((row, index) => [row.client_window_key, { ...row, linked_sessions: [{ project: row.last_project, workflow_session_id: sessions[index % 4].session_id, title: sessions[index % 4].title }], active_requests: index === 0 ? [{ server_trace_id: 'fixture-active', tool_name: 'read_files', project: row.last_project, started_at_ms: now - 2000, elapsed_ms: 2000 }] : [], activity: [{ meaningful: true, tool_name: ['read_files', 'apply_text_edits', 'run_command'][index % 3], project: row.last_project, status: index === 3 ? 'failed' : 'succeeded', started_at_ms: now - index * 30_000 - 1000, ended_at_ms: now - index * 30_000, request_observed_at_ms: now - index * 30_000 - 1000, response_handed_at_ms: now - index * 30_000, service_ms: 1000 }], sessions_truncated: false, activity_truncated: false }]));
const report = { fixture: true, nativeBackend: false, checks: [], screenshots: [], errors: [] };
let browser;
async function bounded(page, label) {
  const bounds = await page.evaluate(() => { const main = document.querySelector('.main-content'); return { width: innerWidth, document: document.documentElement.scrollWidth, main: main.clientWidth, content: main.scrollWidth }; });
  assert(bounds.document <= bounds.width + 1 && bounds.content <= bounds.main + 1, `${label}: ${JSON.stringify(bounds)}`);
  report.checks.push(label);
}
try {
  browser = await chromium.launch({ headless: true });
  for (const width of [1440, 1280, 1024, 768, 600, 390]) for (const theme of ['light', 'dark']) {
    const context = await browser.newContext({ viewport: { width, height: 960 }, reducedMotion: 'reduce' });
    await context.addInitScript(theme => { localStorage.setItem('webcodex.desktop.locale', 'en-US'); localStorage.setItem('webcodex.desktop.appearance', theme); }, theme);
    const page = await context.newPage();
    page.on('pageerror', error => report.errors.push(error.message));
    await page.route('**/__fixture/desktop-state*', async route => {
      const response = await route.fetch(); const state = await response.json();
      state.workspace_runner = { client_id: 'fixture-runner', config_path: '/fixture/runner.toml', server_url: 'http://127.0.0.1:1' };
      await route.fulfill({ response, json: state });
    });
    await page.route('**/desktop-shim.js', async route => {
      const response = await route.fetch(); let source = await response.text();
      const marker = "if(request.kind==='overview')return";
      assert(source.includes(marker));
      const override = `if(request.kind==='overview')return ${JSON.stringify({ runners, projects_available: true, visible_projects: projects.length, projects, projects_truncated: false, recent_sessions: { sessions, truncated: false, scan_truncated: false } })};if(request.kind==='projects')return ${JSON.stringify({ projects, total: projects.length, truncated: false })};if(request.kind==='windows')return ${JSON.stringify({ windows })};if(request.kind==='window')return ${JSON.stringify(details)}[request.client_window_key];if(request.kind==='session')return ${JSON.stringify(sessions)}.find(row=>row.session_id===request.session_id);`;
      source = source.replace(marker, override + marker);
      await route.fulfill({ response, body: source });
    });
    await page.goto(fixture.url + '/desktop/');
    for (const destination of ['home', 'projects', 'activity', 'connection', 'extensions', 'settings']) {
      await page.locator(`[data-webcodex-action="navigate-${destination}"]`).click();
      await page.locator(`[data-webcodex-page="${destination}"]`).waitFor();
      if (destination === 'projects') {
        const devices = page.getByRole('region', { name: 'Task execution devices' });
        await devices.waitFor();
        assert((await devices.innerText()).includes('Desktop session unavailable'));
        assert((await devices.innerText()).includes('Desktop session status not reported'));
        assert((await devices.innerText()).includes('Desktop session status needs a fresh check'));
        assert.equal(await devices.locator('[data-runner-id="fixture-runner"] details').getAttribute('open'), null);
      }
      if (destination === 'activity') {
        await page.locator('.activity-work-row').first().waitFor();
        for (const tab of ['windows', 'sessions', 'system']) {
          await page.locator(`#activity-tab-${tab}`).click();
          await bounded(page, `Activity ${tab} ${width} ${theme}`);
          if (width === 1280 || width === 390) {
            const filename = `activity-${tab}-${width}-${theme}.png`;
            await page.screenshot({ path: path.join(output, filename), animations: 'disabled' }); report.screenshots.push(filename);
          }
        }
      }
      await bounded(page, `${destination} ${width} ${theme}`);
      if ((width === 1280 || width === 390) && destination === 'projects') {
        const filename = `projects-${width}-${theme}.png`;
        await page.screenshot({ path: path.join(output, filename), animations: 'disabled' }); report.screenshots.push(filename);
      }
    }
    const mutations = await page.evaluate(() => window.__fixtureCalls.filter(call => /^(save_|update_|start_|stop_|restart_|remove_|register_|authorize_|environment_service_action)/.test(call.cmd)));
    assert.deepEqual(mutations, [], 'browsing pages must not mutate configuration, authority, or services');
    await context.close();
  }
  assert.deepEqual(report.errors, []);
  console.log(JSON.stringify({ fixture: true, nativeBackend: false, checks: report.checks.length, screenshots: report.screenshots.length }));
} finally {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  if (browser) await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
