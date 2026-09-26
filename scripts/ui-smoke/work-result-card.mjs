import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { baseState } from '../../src/mcp_tests/work_result_app_fixture.mjs';

// Real browser, shipped card HTML, isolated MCP Host protocol fixture.
const html = fs.readFileSync(new URL('../../src/mcp_work_result_app.html', import.meta.url), 'utf8');
const output = new URL('../../artifacts/projects-runtime/card-review/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const browser = await chromium.launch({ headless: true, ...(fs.existsSync(chrome) ? { executablePath: chrome } : {}) });
try {
  for (const width of [800, 390]) {
    const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
    page.setDefaultTimeout(10_000);
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.setContent('<style>body{margin:0}iframe{width:100%;height:980px;border:0}</style>');
    const state = structuredClone(baseState);
    state.window_activity.active = true;
    state.window_activity.active_requests = ["run_shell", "read_files"].map((tool, index) => ({
      label: "Active call", tool_name: tool, server_trace_id: "active-" + index, started_at_ms: Date.now(),
    }));
    state.activity.active = true;
    state.activity.current = { label: "Running tools", kind: "run", started_at_ms: Date.now() };
    state.collaboration.messages = [{ message_id: 'wc_msg_reading', created_at_ms: Date.now(), message: 'Please review the collaboration workflow before release.', source: 'operator', direction: 'inbound', requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null }];
    await page.evaluate(({ html, state }) => {
      window.fixtureState = state;
      window.writes = [];
      const frame = document.createElement('iframe');
      addEventListener('message', event => {
        if (event.source !== frame.contentWindow) return;
        const request = event.data;
        const reply = result => frame.contentWindow.postMessage({ jsonrpc: '2.0', id: request.id, result }, '*');
        if (request.method === 'ui/initialize') {
          reply({ protocolVersion: '2026-01-26' });
          frame.contentWindow.postMessage({ jsonrpc: '2.0', method: 'ui/notifications/tool-result', params: { structuredContent: { success: true, output: { work_result: window.fixtureState } } } }, '*');
        } else if (request.method === 'tools/call') {
          if (request.params.name === 'work_result_state') reply({ structuredContent: { success: true, output: { work_result: window.fixtureState } } });
          if (request.params.name === 'work_result_send_message') {
            window.writes.push(request.params.arguments);
            reply(window.writes.length === 1 ? {} : { structuredContent: { success: true, output: { message_id: 'wc_msg_receipt' } } });
          }
        }
      });
      frame.srcdoc = html;
      document.body.append(frame);
    }, { html, state });
    const card = page.frameLocator('iframe');
    await card.getByText('run_shell', { exact: true }).waitFor();
    assert.equal(await card.getByText('Running', { exact: true }).count(), 2);
    assert(await card.locator('body').evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await page.screenshot({ path: new URL(`work-result-activity-${width}.png`, output).pathname, fullPage: true });
    await card.getByRole('tab', { name: 'Results', exact: true }).click();
    await card.getByText('src/a.rs', { exact: true }).waitFor();
    await card.getByText('Modified · Unstaged', { exact: true }).waitFor();
    await card.getByText('Checks passed', { exact: true }).waitFor();
    assert.equal(await card.locator('#finalChanges').isVisible(), false);
    assert(await card.locator('body').evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await card.locator('#workspaceFiles code').evaluate(node => {
      window.originalFile = node;
      const range = document.createRange(); range.selectNodeContents(node);
      getSelection().removeAllRanges(); getSelection().addRange(range);
    });
    await page.evaluate(() => { window.fixtureState.state_version = 'wr2_' + 'c'.repeat(64); });
    await card.locator('#refresh').evaluate(node => node.click());
    await card.getByRole('button', { name: 'Refresh', exact: true }).waitFor();
    assert(await card.locator('#workspaceFiles code').evaluate(node => node === window.originalFile && getSelection().toString() === node.textContent));
    await page.screenshot({ path: new URL(`work-result-files-${width}.png`, output).pathname, fullPage: true });
    await card.getByRole('button', { name: 'Discuss these changes', exact: true }).click();
    await card.getByText('Saved', { exact: true }).waitFor();
    await card.locator('#messages .message-copy').evaluate(node => {
      window.originalMessage = node;
      const range = document.createRange(); range.selectNodeContents(node);
      getSelection().removeAllRanges(); getSelection().addRange(range);
    });
    await page.evaluate(() => { window.fixtureState.state_version = 'wr2_' + 'b'.repeat(64); });
    // Programmatic click preserves the browser selection, just like automatic polling.
    await card.locator('#refresh').evaluate(node => node.click());
    await card.getByRole('button', { name: 'Refresh', exact: true }).waitFor();
    assert(await card.locator('#messages .message-copy').evaluate(node => node === window.originalMessage && getSelection().toString() === node.textContent));
    await card.getByRole('textbox', { name: 'Message this Window' }).fill('Ready for review');
    await card.getByRole('button', { name: 'Send', exact: true }).click();
    await card.getByRole('button', { name: 'Retry', exact: true }).click();
    await card.locator('#composerState').filter({ hasText: 'Saved' }).waitFor();
    const writes = await page.evaluate(() => window.writes);
    assert.equal(writes.length, 2);
    assert.deepEqual(writes[0], writes[1]);
    assert(await card.locator('body').evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    assert.equal(await card.locator('#refresh').evaluate(node => getComputedStyle(node).transitionDuration), '0s');
    await page.screenshot({ path: new URL(`work-result-${width}.png`, output).pathname, fullPage: true });
    assert.deepEqual(errors, []);
    await page.close();
  }
  console.log('Work Result card: live files, Results navigation, selection preservation, exact retry, reduced motion and narrow layout passed at 800 / 390 px.');
} finally { await browser.close(); }
