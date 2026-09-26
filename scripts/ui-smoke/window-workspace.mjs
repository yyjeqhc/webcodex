import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Render the production bundle against loopback fixtures; no real messages are sent.
const output = new URL('../../artifacts/runtime-call-stream/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const errors = [];
const checks = [];
try {
  for (const width of [1440, 1024, 390]) {
    for (const theme of ['light', 'dark']) {
      for (const language of ['en', 'zh-CN']) {
        const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
        page.on('pageerror', error => errors.push(error.message));
        await page.addInitScript(theme => localStorage.setItem('webcodex.runtime.appearance.v1', theme), theme);
        // The fixture normally initializes English; replace that one preference for Chinese checks.
        await page.route('**/runtime/', async route => {
          const response = await route.fetch();
          await route.fulfill({ response, body: (await response.text()).replace('language.v1","en"', `language.v1","${language}"`) });
        });
        await page.route('**/api/runtime-console/window', async route => {
          const response = await route.fetch();
          const data = await response.json();
          const base = data.activity[0];
          const firstSession = data.linked_sessions[0];
          data.linked_sessions = [
            firstSession,
            ...Array.from({ length: 11 }, (_, index) => ({
              ...firstSession,
              workflow_session_id: `wc_sess_fixture_${String(index + 2).padStart(2, '0')}`,
              first_linked_at_ms: firstSession.first_linked_at_ms + (index + 1) * 1000,
              last_linked_at_ms: firstSession.last_linked_at_ms + (index + 1) * 1000,
              title: `Fixture work Session ${index + 2}`,
            })),
          ];
          data.sessions_returned = data.linked_sessions.length;
          const taggedCalls = data.linked_sessions.map((session, index) => ({
            ...base,
            server_trace_id: `session-call-${index}`,
            tool_name: index % 3 === 0 ? 'read_files' : index % 3 === 1 ? 'run_shell' : 'apply_text_edits',
            status: index === 2 ? 'error' : 'success',
            started_at_ms: base.started_at_ms + index * 50,
            ended_at_ms: base.ended_at_ms + index * 50,
            workflow_sessions: [{ workflow_session_id: session.workflow_session_id, project: base.project, relation: 'recording' }],
          }));
          const fixtureJobId = 'wc_job_fixture_background_123';
          taggedCalls[0] = { ...taggedCalls[0], tool_name: 'cargo_test', async_job_id: fixtureJobId };
          taggedCalls[1] = { ...taggedCalls[1], tool_name: 'observe_jobs', observed_job_ids: [fixtureJobId] };
          data.jobs = [{
            job_id: fixtureJobId,
            status: 'running',
            active: true,
            terminal: false,
            elapsed_secs: 95,
          }];
          data.jobs_truncated = false;
          data.activity = [
            { ...base, server_trace_id: 'observe-1', tool_name: 'runtime_status', meaningful: false, project: undefined, workflow_sessions: [] },
            ...taggedCalls,
          ];
          data.activity_returned = data.activity.length;
          await route.fulfill({ response, json: data });
        });
        const messages = [
          { message_id: 'wc_msg_operator', source: 'operator', direction: 'inbound', message: 'Check the failing tests first.', created_at_ms: Date.now(), requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null },
          { message_id: 'wc_msg_peer', source: 'peer', direction: 'inbound', peer_id: 'wc_peer_' + 'b'.repeat(32), message: 'Parser review complete.', created_at_ms: Date.now(), requires_ack: false, first_projected_at_ms: Date.now(), first_ack_observed_at_ms: null },
        ];
        const sent = [];
        await page.route('**/api/runtime-console/window-collaboration', route => route.fulfill({ json: { available: true, can_send: true, messages, truncated: false } }));
        await page.route('**/api/runtime-console/window-collaboration-post', async route => {
          const payload = route.request().postDataJSON(); sent.push(payload);
          const message_id = 'wc_msg_send' + sent.length;
          messages.push({ ...messages[0], message_id, message: payload.message, context_session_id: payload.context_session_id || undefined });
          await route.fulfill({ json: { message_id, replayed: false, state_changed: true } });
        });
        await page.goto(fixture.url + '/runtime/');
        const zh = language === 'zh-CN';
        await page.locator('.window-call-card.running').waitFor();
        const calls = page.getByTestId('window-workflow-step');
        assert.equal(await calls.count(), 14);
        assert.equal((await calls.locator('header strong').allTextContents())[0], 'runtime_status');
        assert.equal((await calls.locator('header strong').allTextContents()).at(-1), 'run_process');
        assert.equal(await calls.nth(0).getByTestId('window-project-tag').count(), 0);
        assert(await page.getByText(zh ? '失败' : 'Failed', { exact: true }).first().isVisible());
        const centerTabs = page.locator('.window-center-tabs [role="tab"]');
        assert.equal(await centerTabs.count(), 2);
        const tabLabels = (await centerTabs.allTextContents()).map(value => value.replace(/\s+/g, ' ').trim());
        assert(tabLabels[0].includes(zh ? '窗口' : 'Window'), JSON.stringify(tabLabels));
        assert(tabLabels[1].includes(zh ? '协作' : 'Collaboration'), JSON.stringify(tabLabels));
        assert.equal(await page.getByRole('tab', { name: /Work Sessions|工作会话/ }).count(), 0);

        const sessionFilter = page.getByRole('combobox', { name: zh ? '会话筛选' : 'Session filter' });
        await sessionFilter.waitFor();
        assert.equal(await sessionFilter.locator('option').count(), 13);
        assert.equal(await sessionFilter.inputValue(), '');
        const jobTags = page.locator('.window-job-tag');
        assert.equal(await jobTags.count(), 2);
        assert((await jobTags.nth(0).textContent()).includes(zh ? '后台运行' : 'Background running'));
        assert((await jobTags.nth(0).textContent()).includes('1m 35s'));
        assert((await jobTags.nth(1).textContent()).includes(zh ? '观察' : 'Observing'));
        await sessionFilter.selectOption('wc_sess_fixture_02');
        assert.equal(await calls.count(), 1);
        await sessionFilter.selectOption('');
        assert.equal(await calls.count(), 14);

        const bounds = await page.evaluate(() => {
          const filter = document.querySelector('.window-session-focus')?.getBoundingClientRect();
          return { width: innerWidth, body: document.body.scrollWidth, root: document.documentElement.scrollWidth, filterRight: filter?.right ?? null };
        });
        assert(bounds.body <= width + 1 && bounds.root <= width + 1, JSON.stringify(bounds));
        assert(bounds.filterRight !== null && bounds.filterRight <= width + 1, JSON.stringify(bounds));
        await page.screenshot({ path: new URL(`calls-${width}-${theme}-${language}.png`, output).pathname, fullPage: true });
        await centerTabs.nth(1).click();
        await page.getByText('Parser review complete.', { exact: true }).waitFor();
        const composer = page.getByRole('textbox', { name: zh ? '给这个窗口发消息' : 'Message this Window' });
        await composer.fill('Window message without context');
        await page.getByRole('button', { name: zh ? '发送' : 'Send', exact: true }).click();
        await page.getByText('Window message without context', { exact: true }).waitFor();
        assert.equal(sent[0].context_session_id, null);
        await centerTabs.nth(0).click();
        await sessionFilter.selectOption('wc_sess_fixture_02');
        await centerTabs.nth(1).click();
        await composer.fill('Window message with context');
        await page.getByRole('button', { name: zh ? '发送' : 'Send', exact: true }).click();
        await page.getByText('Window message with context', { exact: true }).waitFor();
        assert.equal(sent[1].client_window_key, sent[0].client_window_key);
        assert.equal(sent[1].context_session_id, 'wc_sess_fixture_02');
        const collaborationBounds = await page.evaluate(() => ({ body: document.body.scrollWidth, root: document.documentElement.scrollWidth }));
        assert(collaborationBounds.body <= width + 1 && collaborationBounds.root <= width + 1, JSON.stringify(collaborationBounds));
        await page.screenshot({ path: new URL(`collaboration-${width}-${theme}-${language}.png`, output).pathname, fullPage: true });
        checks.push({ width, theme, language, overflow: false, individualCalls: 14, centerTabs: 2, sessionFilter: true, sessionOptions: 13, jobLinks: 2, collaboration: true });
        await page.close();
      }
    }
  }
  assert.deepEqual(errors, []);
  fs.writeFileSync(new URL('report.json', output), JSON.stringify({ fixture: true, nativeBackend: false, checks, errors }, null, 2));
  console.log(JSON.stringify({ checks: checks.length, errors }));
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
