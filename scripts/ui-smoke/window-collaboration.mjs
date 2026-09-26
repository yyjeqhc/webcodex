import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { startFixtureServer } from './server.mjs';

// Production WebUI with isolated message fixtures; no real messages are sent.
const output = new URL('../../artifacts/projects-runtime/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(10_000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  let rows = Array.from({ length: 30 }, (_, i) => ({
    message_id: 'wc_msg_' + i, source: 'window', direction: 'outbound',
    message: 'Reply ' + i + ' — ' + 'Review this change carefully. '.repeat(8),
    created_at_ms: Date.now() + i, kind: 'answer', priority: 'normal', requires_ack: false,
    first_projected_at_ms: null, first_ack_observed_at_ms: null,
  }));
  const writes = [];
  await page.route('**/api/runtime-console/window-collaboration', route => route.fulfill({ json: { available: true, can_send: true, messages: rows, truncated: false } }));
  await page.route('**/api/runtime-console/window-collaboration-post', route => {
    writes.push(route.request().postDataJSON());
    return route.fulfill(writes.length === 1 ? { status: 503, json: {} } : { json: { message_id: 'wc_msg_receipt' } });
  });
  await page.goto(fixture.url + '/runtime/');
  await page.getByRole('tab', { name: 'Collaboration', exact: true }).click();
  await page.getByRole('textbox', { name: 'Message this Window' }).fill('Please review the changes');
  const thread = page.locator('.window-collaboration-thread');
  await thread.evaluate(node => { node.scrollTop = 500; node.dispatchEvent(new Event('scroll')); });
  const previous = await thread.evaluate(node => node.scrollTop);
  rows = [...rows, { ...rows.at(-1), message_id: 'wc_msg_latest', message: 'New peer response' }];
  await page.getByRole('button', { name: 'View new messages' }).waitFor();
  assert.equal(await thread.evaluate(node => node.scrollTop), previous);
  await page.getByRole('button', { name: 'View new messages' }).click();
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await page.getByRole('button', { name: 'Retry', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.window-collaboration-composer textarea').value === '');
  assert.equal(writes.length, 2);
  assert.deepEqual(writes[0], writes[1]);
  assert.equal(writes[0].client_window_key, '4700'.repeat(16));
  assert.deepEqual(errors, []);
  await page.screenshot({ path: new URL('window-collaboration-review.png', output).pathname, fullPage: true });
  await page.unrouteAll({ behavior: 'wait' });
  console.log('WebUI browser: reading position, clickable new-message action and exact Window retry passed.');
} finally {
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
