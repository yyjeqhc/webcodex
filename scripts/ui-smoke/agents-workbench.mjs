import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

const output = new URL('../../artifacts/projects-runtime/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const errors = [];
try {
  for (const width of [1440, 390]) {
    const page = await browser.newPage({ viewport: { width, height: 1000 } });
    page.setDefaultTimeout(10_000);
    page.on('pageerror', error => errors.push(error.message));
    const writes = [];
    let acknowledged = false;
    const agent = { agent_id: 'reviewer', handle: 'reviewer', display_name: 'Release reviewer', description: 'Review changes and coordinate release checks.', specialty_labels: ['Code review', 'Release'], profile_revision: 2, active_endpoint_count: 1, queued_delivery_count: 1 };
    await page.route('**/api/runtime-console/communication/**', async route => {
      const name = route.request().url().split('/communication/')[1];
      const payload = route.request().postDataJSON();
      let data;
      if (name === 'agents') data = { agents: [agent] };
      else if (name === 'conversations') data = { conversations: [{ conversation_id: 'review', title: 'Release readiness', message_count: 1, last_seq: 1 }] };
      else if (name === 'conversation') data = { messages: [{ message_id: 'm1', seq: 1, body: 'Please review the navigation changes before release.', author: { participant_kind: 'human' } }] };
      else if (name === 'endpoint/attach') { writes.push({ name, payload }); data = { endpoint: { agent_id: 'reviewer', endpoint_id: 'browser-endpoint', controller_generation: 7, lifecycle: 'attached', wake_capable: false } }; }
      else if (name === 'inbox') data = { deliveries: acknowledged ? [] : [{ delivery_id: 'd1', conversation_id: 'review', conversation_title: 'Release readiness', message: { body: 'Please review the navigation changes before release.' } }] };
      else if (name === 'inbox/consume') { writes.push({ name, payload }); acknowledged = true; data = {}; }
      else if (name === 'message/post') { writes.push({ name, payload }); data = { message: {} }; }
      else data = {};
      await route.fulfill({ json: data });
    });
    await page.addInitScript(() => { localStorage.setItem('webcodex.runtime.v2.view.v1', 'runtime'); localStorage.setItem('webcodex.runtime.language.v1', 'en'); });
    await page.goto(fixture.url + '/runtime/');
    await page.getByRole('tab', { name: 'Agents', exact: true }).click();
    await page.getByRole('heading', { name: 'Release reviewer' }).waitFor();
    assert.equal(writes.length, 0);
    const noOverflow = async () => assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    await noOverflow();
    await page.getByRole('button', { name: 'Connect as this Agent', exact: true }).click();
    await page.getByRole('button', { name: 'Acknowledge', exact: true }).waitFor();
    await page.screenshot({ path: new URL(`agents-inbox-${width}.png`, output).pathname, fullPage: true });
    await page.getByRole('button', { name: 'Acknowledge', exact: true }).click();
    await page.getByText('No queued Inbox deliveries.', { exact: true }).waitFor();
    assert.deepEqual(writes.find(write => write.name === 'inbox/consume').payload, { agent_id: 'reviewer', endpoint_id: 'browser-endpoint', expected_controller_generation: 7, delivery_ids: ['d1'] });
    await page.getByRole('tab', { name: 'All conversations', exact: true }).click();
    await page.getByRole('combobox', { name: 'Recipients (optional)' }).fill('Release');
    await page.getByRole('option', { name: 'Release reviewer (@reviewer)' }).click();
    await page.getByRole('textbox', { name: 'Message', exact: true }).fill('Navigation review is ready.');
    await Promise.all([page.waitForResponse(response => response.url().endsWith('/communication/message/post')), page.getByRole('button', { name: 'Send message', exact: true }).click()]);
    const sent = writes.find(write => write.name === 'message/post').payload;
    assert.equal(sent.conversation_id, 'review');
    assert.equal(sent.author_agent_id, null);
    assert.deepEqual(sent.recipient_agent_ids, ['reviewer']);
    await noOverflow();
    await page.screenshot({ path: new URL(`agents-conversations-${width}.png`, output).pathname, fullPage: true });
    await page.getByRole('tab', { name: 'Profile', exact: true }).click();
    await page.getByText('Technical details', { exact: true }).click();
    await noOverflow();
    await page.unrouteAll({ behavior: 'wait' });
    await page.close();
  }
  assert.deepEqual(errors, []);
  console.log('Agent inbox, exact endpoint acknowledgement, named recipients, human send, profile and overflow: passed at 1440 / 390 px.');
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
