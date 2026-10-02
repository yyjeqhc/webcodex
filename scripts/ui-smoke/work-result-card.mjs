import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { baseState } from '../../src/mcp_tests/work_result_app_fixture.mjs';
const { chromium } = createRequire(import.meta.url)('playwright');

// Real browser, shipped card HTML, isolated MCP Host protocol fixture.
const html = fs.readFileSync(new URL('../../src/mcp_work_result_app.html', import.meta.url), 'utf8');
const output = new URL('../../artifacts/projects-runtime/card-review/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const chrome = ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "C:/Program Files/Google/Chrome/Application/chrome.exe"].find(path => fs.existsSync(path));
const browser = await chromium.launch({ headless: true, ...(chrome ? { executablePath: chrome } : {}) });
try {
  for (const [thread, width] of [[false, 800], [false, 390], [true, 800], [true, 390]]) {
    const surface = thread ? 'thread' : 'card';
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
    state.jobs = { available: true, active: true, truncated: false, items: [
      { job_id: 'smoke-running', tool: 'cargo_test', status: 'running', state: 'active' },
      { job_id: 'smoke-failed', tool: 'project_build', status: 'failed', state: 'terminal', outcome: 'failed' },
      { job_id: 'smoke-passed', tool: 'project_validate', status: 'completed', state: 'terminal', outcome: 'passed' },
    ] };
    state.collaboration.messages = [{ message_id: 'wc_msg_reading', created_at_ms: Date.now(), message: 'Please review the collaboration workflow before release.', source: 'operator', direction: 'inbound', requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null }];
    await page.evaluate(({ html, state, thread }) => {
      window.fixtureState = state;
      window.writes = [];
      window.reads = [];
      window.frozenReads = [];
      const frame = document.createElement('iframe');
      addEventListener('message', event => {
        if (event.source !== frame.contentWindow) return;
        const request = event.data;
        const reply = result => frame.contentWindow.postMessage({ jsonrpc: '2.0', id: request.id, result }, '*');
        if (request.method === 'ui/initialize') {
          reply({ protocolVersion: '2026-01-26' });
          if (thread) frame.contentWindow.postMessage({ jsonrpc: '2.0', method: 'ui/notifications/tool-input', params: { arguments: {} } }, '*');
          frame.contentWindow.postMessage({ jsonrpc: '2.0', method: 'ui/notifications/tool-result', params: {
            structuredContent: { success: true, output: { work_result: window.fixtureState } },
            ...(thread ? { _meta: { 'webcodex/workResultThread': { session_id: state.session_id } } } : {}),
          } }, '*');
        } else if (request.method === 'tools/call') {
          if (request.params.name === 'get_work_result_state') {
            const args = request.params.arguments;
            window.reads.push(args);
            const snapshot_id = 'wc_changes_snapshot_' + '3'.repeat(32);
            const files = args.files?.path ? {
              project: state.project, snapshot_id, path: args.files.path,
              diff: '@@ -1 +1 @@\n-old\n+reviewed', truncated: false,
            } : {
              project: state.project, snapshot_id, offset: 0, files_total: 1, source_truncated: false, next_offset: null,
              files: [{ path: 'src/a.rs', kind: 'modified', additions: 4, deletions: 1 }],
            };
            reply({ structuredContent: { success: true, output: args.files ? { work_result_files: files } : { work_result: window.fixtureState } } });
          }
          if (request.params.name === 'read_changed_file_diff') {
            window.frozenReads.push(request.params.arguments);
            const diff = '@@ -1 +1 @@\n-old\n+final';
            const bytes = new TextEncoder().encode(diff).length;
            reply({ structuredContent: { success: true, output: { changes_file_diff: {
              version: 1, ...request.params.arguments, kind: 'modified', binary: false,
              diff, truncated: false, bytes_total: bytes, bytes_returned: bytes, lines_total: 3, lines_returned: 3,
            } } } });
          }
          if (request.params.name === 'send_work_result_message') {
            window.writes.push(request.params.arguments);
            reply(window.writes.length === 1 ? {} : { structuredContent: { success: true, output: { message_id: 'wc_msg_receipt' } } });
          }
        }
      });
      frame.srcdoc = html;
      document.body.append(frame);
    }, { html, state, thread });
    const card = page.frameLocator('iframe');
    if (thread) {
      await card.getByRole('heading', { name: 'Changed files', exact: true }).waitFor();
      assert.equal(await card.getByRole('tab', { name: 'Review', exact: true }).getAttribute('aria-selected'), 'true');
      assert.equal(await card.locator('#diagnostics').evaluate(node => node.open), false);
      assert.equal(await card.locator('#projectIdentity').isVisible(), false);
      assert.equal(await page.evaluate(() => window.reads.some(args => args.files)), false);
      await page.screenshot({ path: fileURLToPath(new URL(`work-result-${surface}-review-${width}.png`, output)), fullPage: true });
      await card.getByRole('tab', { name: 'Activity', exact: true }).click();
    } else {
      await card.locator('#projectIdentity').waitFor();
      assert.equal(await card.locator('#diagnostics').isVisible(), false);
    }
    await card.getByText('run_shell', { exact: true }).waitFor();
    assert.equal(await card.getByText('Running', { exact: true }).count(), 3);
    assert.equal(await card.locator('#jobsSection').evaluate(node => node.closest('.progress-section') !== null), true);
    assert.equal(await card.locator('#jobResults').evaluate(node => node.open), false);
    await card.getByText('Recent background results · 1 failed · 1 passed', { exact: true }).waitFor();
    assert.equal(await card.getByText('project build', { exact: true }).isVisible(), false);
    await card.locator('#jobResultsSummary').focus();
    await page.keyboard.press('Enter');
    await card.getByText('project build', { exact: true }).waitFor();
    await card.locator('#jobResultsSummary').click();
    assert(await card.locator('body').evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await page.screenshot({ path: fileURLToPath(new URL(`work-result-${surface}-activity-${width}.png`, output)), fullPage: true });
    await card.getByRole('tab', { name: thread ? 'Review' : 'Results', exact: true }).click();
    await card.getByRole('button', { name: 'src/a.rs · +4 −1', exact: true }).waitFor();
    await card.getByText('Checks passed', { exact: true }).waitFor();
    assert.equal(await card.locator('#finalChanges').isVisible(), false);
    assert(await card.locator('body').evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await card.locator('#workspaceFiles .file-toggle').evaluate(node => {
      window.originalFile = node;
      const range = document.createRange(); range.selectNodeContents(node);
      getSelection().removeAllRanges(); getSelection().addRange(range);
    });
    await page.evaluate(() => { window.fixtureState.state_version = 'wr2_' + 'c'.repeat(64); });
    await card.locator('#refresh').evaluate(node => node.click());
    await card.getByRole('button', { name: 'Refresh', exact: true }).waitFor();
    assert(await card.locator('#workspaceFiles .file-toggle').evaluate(node => node === window.originalFile && getSelection().toString() === node.textContent));
    await card.locator('#workspaceFiles .file-toggle').click();
    await card.locator('#workspaceFiles .diff').filter({ hasText: '+reviewed' }).waitFor();
    await card.locator('#refresh').click();
    await card.getByRole('button', { name: 'Refresh', exact: true }).waitFor();
    assert.equal(await card.locator('#workspaceFiles .file-toggle').getAttribute('aria-expanded'), 'true');
    if (thread) assert(await page.evaluate(() => window.reads.filter(args => !args.files).every(args => args.session_id === window.fixtureState.session_id)));
    if (thread) {
      await page.evaluate(() => {
        window.fixtureState.state_version = 'wr2_' + 'd'.repeat(64);
        window.fixtureState.final_changes = {
          snapshot_id: 'wc_changes_snapshot_' + 'f'.repeat(32), files_changed: 1, files_total: 1, files_returned: 1,
          files_truncated: false, additions: 2, deletions: 1,
          files: [{ path: 'src/final.rs', kind: 'modified', binary: false, additions: 2, deletions: 1 }],
        };
      });
      await card.locator('#refresh').click();
      await card.locator('#finalChanges').waitFor();
      assert.equal(await page.evaluate(() => window.frozenReads.length), 0);
      assert(await card.locator('#panelResults').evaluate(node => {
        const top = id => node.querySelector('#' + id).getBoundingClientRect().top;
        return top('workspaceChangesSection') < top('resultChecks') && top('resultChecks') < top('finalChanges');
      }));
      await card.locator('#frozenFiles .file-toggle').click();
      await card.locator('#frozenFiles .diff').filter({ hasText: '+final' }).waitFor();
      const frozen = await card.locator('#frozenFiles .diff').innerText();
      await page.evaluate(() => {
        window.fixtureState.state_version = 'wr2_' + 'e'.repeat(64);
        window.fixtureState.workspace = { ...window.fixtureState.workspace, additions: 6,
          files: [{ ...window.fixtureState.workspace.files[0], additions: 6 }] };
      });
      await card.locator('#refresh').click();
      await card.getByRole('button', { name: 'Refresh', exact: true }).waitFor();
      assert.equal(await card.locator('#frozenFiles .diff').innerText(), frozen);
      assert.equal(await page.evaluate(() => window.frozenReads.length), 1);
    }
    await page.screenshot({ path: fileURLToPath(new URL(`work-result-${surface}-files-${width}.png`, output)), fullPage: true });
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
    await page.screenshot({ path: fileURLToPath(new URL(`work-result-${surface}-${width}.png`, output)), fullPage: true });
    assert.deepEqual(errors, []);
    await page.close();
  }
  console.log('Work Result inline card and thread review: lazy diff, Session refresh, folded diagnostics, keyboard disclosure, selection preservation, exact retry, reduced motion and narrow layout passed at 800 / 390 px.');
} finally { await browser.close(); }
