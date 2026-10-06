import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { baseState } from '../../src/mcp_tests/work_result_app_fixture.mjs';
const { chromium } = createRequire(import.meta.url)('playwright');
const html = fs.readFileSync(new URL('../../src/mcp_work_result_app.html', import.meta.url), 'utf8');
const output = new URL('../../artifacts/projects-runtime/collaboration-history/', import.meta.url);
fs.mkdirSync(output, {recursive: true});
const browser = await chromium.launch({headless: true});
try {
  for (const width of [800, 390]) {
    const page = await browser.newPage({viewport: {width, height: 1000}, reducedMotion: 'reduce'});
    page.setDefaultTimeout(10000);
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.setContent('<style>body{margin:0}iframe{width:100%;height:980px;border:0}</style>');
    const now = Date.now();
    const message = (id, text, delta) => ({message_id: `wc_msg_${id}`, created_at_ms: now + delta, message: text,
      source: 'operator', direction: 'inbound', kind: 'guidance', priority: 'normal', requires_ack: true,
      first_projected_at_ms: null, first_ack_observed_at_ms: null});
    const state = structuredClone(baseState);
    state.collaboration = {available: true, can_send: true, history_scope: 'a'.repeat(64), truncated: true,
      next_before: 'wc_msg_question', messages: [
        {...message('question', 'Please keep the earlier discussion visible after refresh. Can you confirm the persistence boundary?', -60000), kind: 'question', first_projected_at_ms: now - 50000, first_ack_observed_at_ms: now - 40000},
        {...message('answer', 'The history is now separate from the delivery queue. I will verify restart and pagination before submitting the PR.', -30000), source: 'window', direction: 'outbound', kind: 'answer', requires_ack: false, reply_to_message_id: 'wc_msg_question'},
        {...message('peer', 'Review note: an acknowledgement is not task completion. Keep the reply relationship explicit.', -10000), source: 'peer', direction: 'inbound', kind: 'risk', priority: 'high', requires_ack: false, peer_id: 'wc_peer_' + 'b'.repeat(32)},
      ]};
    const earlier = {...message('earlier', 'Earlier decision: preserve history, but keep model delivery bounded.', -120000), kind: 'decision'};
    await page.evaluate(({html, state, earlier}) => {
      window.fixtureState = state; window.reads = []; window.writes = [];
      const frame = document.createElement('iframe');
      addEventListener('message', event => {
        if (event.source !== frame.contentWindow) return;
        const request = event.data;
        const reply = result => frame.contentWindow.postMessage({jsonrpc: '2.0', id: request.id, result}, '*');
        if (request.method === 'ui/initialize') {
          reply({protocolVersion: '2026-01-26'});
          frame.contentWindow.postMessage({jsonrpc: '2.0', method: 'ui/notifications/tool-result', params: {structuredContent: {success: true, output: {work_result: window.fixtureState}}}}, '*');
        } else if (request.method === 'tools/call') {
          const args = request.params.arguments;
          if (request.params.name === 'get_work_result_state') {
            window.reads.push(args);
            const history = args.collaboration?.before_message_id
              ? {...state.collaboration, messages: [earlier], truncated: false, next_before: null}
              : state.collaboration;
            reply({structuredContent: {success: true, output: args.collaboration ? {work_result_collaboration: history} : {work_result: window.fixtureState}}});
          } else if (request.params.name === 'send_work_result_message') {
            window.writes.push(args);
            reply({structuredContent: {success: true, output: {message_id: 'wc_msg_sent'}}});
          }
        }
      });
      frame.srcdoc = html; document.body.append(frame);
    }, {html, state, earlier});
    const card = page.frameLocator('iframe');
    await card.getByRole('tab', {name: 'Collaboration', exact: true}).click();
    await card.getByText('Reply received', {exact: true}).waitFor();
    await card.locator('.message-reply').filter({hasText: 'Please keep the earlier discussion'}).waitFor();
    assert.equal(await card.locator('.message').count(), 3);
    await card.getByRole('button', {name: 'Load earlier messages'}).click();
    await card.getByText(earlier.message, {exact: true}).waitFor();
    await page.evaluate(() => { window.fixtureState.state_version = 'wr2_' + 'd'.repeat(64); });
    await card.getByRole('button', {name: 'Refresh', exact: true}).click();
    await card.getByRole('button', {name: 'Refresh', exact: true}).waitFor();
    assert.equal(await card.getByText(earlier.message, {exact: true}).count(), 1);
    assert(await card.locator('body').evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await page.screenshot({path: fileURLToPath(new URL(`history-${width}.png`, output)), fullPage: true});
    await card.getByRole('button', {name: 'Return to latest messages'}).click();
    await card.getByRole('combobox', {name: 'Message type'}).selectOption('question');
    await card.getByRole('textbox', {name: 'Message this Window'}).fill('Please verify these changes.');
    await card.getByRole('button', {name: 'Send', exact: true}).click();
    await card.locator('#composerState').filter({hasText: 'Saved'}).waitFor();
    assert.deepEqual(await page.evaluate(() => window.writes.map(write => write.kind)), ['question']);
    assert.deepEqual(errors, []);
    await page.close();
  }
  console.log('Collaboration history: real Chromium 800/390px, exact reply context, intent, earlier-page persistence across refresh, return to latest and bounded layout passed. Isolated MCP Host fixture, not a production session.');
} finally { await browser.close(); }
