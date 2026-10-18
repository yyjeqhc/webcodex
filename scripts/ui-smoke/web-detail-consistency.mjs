import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Cover the record/diagnostic surfaces nested below the full-page matrix.
// All requests use synthetic fixture data; no real backend or credentials.
const output = new URL('../../artifacts/web-page-consistency/details/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, checks: [], errors: [], limitations: ['Chromium fixtures; inspector intentionally collapses below1350px; no real model or backend mutations.'] };
async function capture(page, surface, width, theme) {
  await page.waitForTimeout(50);
  const geometry = await page.evaluate(() => ({
    overflow: Math.max(document.body.scrollWidth, document.documentElement.scrollWidth) > innerWidth + 1,
    tabs: [...document.querySelectorAll('.session-view-tabs button.active, .segmented button.active')].filter(e => e.getBoundingClientRect().width).map(e => ({
      color: getComputedStyle(e).color, background: getComputedStyle(e).backgroundColor,
      underline: getComputedStyle(e).borderBottomWidth,
    })),
    rails: document.querySelectorAll('.ui-selection-rail').length,
    row: (() => {
      const row = document.querySelector('.work-row.selected');
      if (!row) return null;
      const body = row.querySelector('.work-row-body').getBoundingClientRect();
      const time = row.querySelector('time').getBoundingClientRect();
      return { border: getComputedStyle(row).borderTopWidth, bodyTop: body.top, timeTop: time.top, timeRight: time.right, bodyRight: body.right };
    })(),
    max: Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--web-content-max')),
  }));
  assert.equal(geometry.overflow, false, `${surface}/${width}/${theme} overflow`);
  assert.equal(geometry.rails, 0, 'Nested Session selection must not mount a rail');
  if (geometry.row) {
    assert.equal(geometry.row.border, '1px');
    assert(Math.abs(geometry.row.bodyTop - geometry.row.timeTop) <= 2, 'Session timestamp shares the title row');
    assert(geometry.row.timeRight > geometry.row.bodyRight, 'Session timestamp remains at trailing edge');
  }
  for (const tab of geometry.tabs) {
    assert.equal(tab.background, theme === 'light' ? 'rgb(25, 25, 25)' : 'rgb(245, 245, 245)');
    assert.equal(tab.color, theme === 'light' ? 'rgb(255, 255, 255)' : 'rgb(25, 25, 25)');
    assert.equal(tab.underline, '0px');
  }
  report.checks.push({ surface, width, theme, ...geometry });
  await page.screenshot({ path: new URL(`${surface}-${width}-${theme}.png`, output).pathname, fullPage: true, animations: 'disabled' });
}
try {
  for (const width of [1440, 1280, 1024, 768, 390, 1920]) for (const theme of ['light', 'dark']) {
    const page = await browser.newPage({ viewport: { width, height: 1000 }, reducedMotion: 'reduce' });
    page.on('pageerror', e => report.errors.push(e.message));
    await page.addInitScript(theme => localStorage.setItem('webcodex.runtime.appearance.v1', theme), theme);
    let messageFailure = false;
    await page.route('**/api/runtime-console/workflow-session-messages', route => route.fulfill(messageFailure
      ? { status: 503, json: {} } : { json: { session_id: 'wc_sess_fixture470_active', messages: [{ message_id: 'fixture-question', kind: 'question', status: 'open', message: 'Review the layout and spacing.', created_at: Math.floor(Date.now() / 1000), requires_ack: true }] } }));
    await page.route('**/api/runtime-console/trace', route => {
      const body = route.request().postDataJSON();
      const data = body.query ? { calls: [{ trace_ref: 'fixture-layout-trace', tool_name: 'read_files', observed_at_ms: Date.now(), duration_ms: 35, status: 'success', project: 'agent:fixture-runner:alpha', window_key: null }] }
        : body.payload_index !== undefined ? { payload_available: true, payload: { fixture: true, layout: 'retained synthetic payload' } }
        : { status: 'available', trace_mode: 'metadata', capture_mode: 'metadata', events: [{ phase: 'request', diagnostic: { fixture: true, observed: 'request' }, payload_index: 0, payload_bytes: 80 }] };
      return route.fulfill({ json: data });
    });
    await page.goto(fixture.url + '/runtime/');
    await page.getByRole('combobox', { name: 'Session filter' }).waitFor();
    await page.getByRole('button', { name: 'Inspect call diagnostics', exact: true }).first().click();
    await page.getByRole('button', { name: /Read retained full payload/ }).first().click();
    await page.getByText('Retained full payload', { exact: true }).waitFor();
    await capture(page, 'window-trace-payload', width, theme);
    await page.locator('.trace-search > summary').click();
    await page.getByRole('button', { name: 'Search retained calls', exact: true }).click();
    await page.locator('.trace-search-results').waitFor();
    await capture(page, 'trace-search', width, theme);
    await page.getByRole('combobox', { name: 'Session filter' }).selectOption('wc_sess_fixture470_active');
    await page.getByRole('button', { name: 'View Session record', exact: true }).click();
    await page.getByRole('tab', { name: 'Workflow', exact: true }).waitFor();
    await capture(page, 'session-workflow', width, theme);
    if (width > 1350) {
      await page.getByRole('tab', { name: 'Evidence', exact: true }).click();
      await capture(page, 'session-inspector-evidence', width, theme);
      await page.getByRole('tab', { name: 'Evidence', exact: true }).press('ArrowLeft');
      assert.equal(await page.getByRole('tab', { name: 'Context', exact: true }).getAttribute('aria-selected'), 'true');
    }
    await page.getByRole('tab', { name: /^Collaboration/ }).click();
    await page.getByText('Review the layout and spacing.', { exact: true }).waitFor();
    await capture(page, 'session-collaboration', width, theme);
    const composer = page.locator('#collaboration-panel textarea');
    await composer.fill('Retain this fixture draft during refresh failure.');
    messageFailure = true;
    await page.getByRole('button', { name: 'Refresh', exact: true }).first().click();
    await page.getByText('Message refresh failed; previous messages retained.', { exact: false }).waitFor();
    assert.equal(await composer.inputValue(), 'Retain this fixture draft during refresh failure.');
    await capture(page, 'session-stale', width, theme);
    messageFailure = false;
    await page.unrouteAll({ behavior: 'wait' });
    // Open the chooser with no authoritative Window links, then the record.
    const nav = page.locator(width <= 700 ? '.mobile-primary-nav' : '.app-nav');
    await nav.getByRole('button', { name: /^Projects/ }).click();
    await page.getByRole('button', { name: /Fixture workspace review/ }).click();
    await page.getByRole('dialog', { name: 'Session activity', exact: true }).waitFor();
    await page.getByText('No linked Windows in retained evidence.', { exact: true }).waitFor();
    await capture(page, 'session-window-chooser', width, theme);
    await page.keyboard.press('Escape');
    await page.close();
  }
  assert.deepEqual(report.errors, []);
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ checks: report.checks.length, errors: report.errors }));
} finally { await browser.close(); await new Promise(resolve => fixture.server.close(resolve)); }
