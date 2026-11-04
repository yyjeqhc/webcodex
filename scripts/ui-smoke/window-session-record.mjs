import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';
import { projects, sessions, webResponse, windowActivity, windowSummary } from './fixtures.mjs';

// Isolated loopback fixture only. This exercises the checked-in production
// renderer; it never contacts a configured Server, Runner, or user's browser.
const phase = process.argv.includes('--baseline') ? 'before' : 'after';
const output = new URL('../../artifacts/window-session-record/' + phase + '/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const fixtureDelaysMs = { 'workflow-session': 120, overview: 60, 'project-git': 60 };
const report = { fixture: true, nativeBackend: false, phase, fixtureDelaysMs,
  timingScope: 'Single local fixture observation with artificial delays; not production latency or P95.',
  checks: [], errors: [] };
const selectors = {
  list: '.window-board-list',
  panel: '#window-activity-panel',
  calls: '.window-activity-results',
  detail: '.window-board-detail',
  shell: '.app-content',
  document: null,
};
const scrollSnapshot = page => page.evaluate(selectors => Object.fromEntries(
  Object.entries(selectors).map(([name, selector]) => [name,
    (selector ? document.querySelector(selector) : document.scrollingElement)?.scrollTop || 0]),
), selectors);
const paint = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
const tabBounds = page => page.locator('.window-center-tabs > button').evaluateAll(buttons =>
  buttons.map(button => {
    const lineTops = [...button.childNodes].filter(node => node.nodeType === Node.TEXT_NODE && node.textContent.trim())
      .flatMap(node => {
        const range = document.createRange();
        range.selectNodeContents(node);
        return [...range.getClientRects()].map(rect => Math.round(rect.top));
      });
    return { label: button.textContent, labelLines: new Set(lineTops).size, ...button.getBoundingClientRect().toJSON() };
  }));
const assertTabRow = (tabs, count) => {
  assert.equal(tabs.length, count);
  assert(tabs.every(tab => tab.labelLines === 1), 'English tab labels must fit without splitting words: ' + JSON.stringify(tabs));
  assert(Math.max(...tabs.map(tab => tab.top)) - Math.min(...tabs.map(tab => tab.top)) <= 1,
    'Window destinations must share one tab row: ' + JSON.stringify(tabs));
};
let browser;
try {
  browser = await chromium.launch({ headless: true });
  for (const width of [1440, 390]) {
    const page = await browser.newPage({ viewport: { width, height: 960 }, reducedMotion: 'reduce' });
    page.setDefaultTimeout(15_000);
    page.on('pageerror', error => report.errors.push(error.message));
    const firstKey = 'a'.repeat(64);
    const targetKey = 'b'.repeat(64);
    const sessionId = sessions[1].session_id;
    const rows = Array.from({ length: 40 }, (_, index) => ({
      ...windowSummary,
      client_window_key: index === 0 ? firstKey : index === 1 ? targetKey : index.toString(16).padStart(64, '0'),
      label: index === 0 ? 'Window A' : index === 1 ? 'Window B' : 'Fixture Window ' + (index + 1),
      active_count: 0, last_meaningful_activity_at_ms: windowSummary.last_seen_at_ms - index * 1000,
    }));
    const requests = [];
    await page.route('**/api/runtime-console/**', async route => {
      const name = new URL(route.request().url()).pathname.slice('/api/runtime-console/'.length);
      const body = route.request().postDataJSON();
      requests.push({ route: name, body });
      let data;
      if (name === 'windows') {
        const visible = body.projection === 'liveness' ? [] : rows.slice(body.offset || 0, (body.offset || 0) + (body.limit || 50));
        data = { windows: visible, total: body.projection === 'liveness' ? 0 : rows.length,
          returned: visible.length, truncated: false, visibility: { scope: 'principal' } };
      } else if (name === 'window') {
        const key = body.client_window_key;
        const id = key === targetKey ? sessionId : sessions[0].session_id;
        const all = Array.from({ length: 160 }, (_, index) => ({
          ...windowActivity, server_trace_id: 'fixture-' + key.slice(0, 4) + '-' + index,
          started_at_ms: windowSummary.last_seen_at_ms - (160 - index) * 1000,
          ended_at_ms: windowSummary.last_seen_at_ms - (160 - index) * 1000 + 50,
          workflow_sessions: [{ workflow_session_id: id, project: projects[0].id, relation: 'recording' }],
        }));
        const primary = body.detail_level === 'primary';
        const activity = primary ? all.slice(-80) : all;
        data = {
          ...webResponse('window', body), ...rows.find(row => row.client_window_key === key),
          client_window_key: key, detail_level: primary ? 'primary' : 'full',
          active_count: 0, active_requests: [], activity, activity_returned: activity.length, activity_truncated: primary,
          linked_sessions: [{ workflow_session_id: id, project: projects[0].id,
            first_linked_at_ms: windowSummary.last_seen_at_ms - 160_000, last_linked_at_ms: windowSummary.last_seen_at_ms,
            relations: ['recording'], relation_count: 1, title: key === targetKey ? 'Second Window Session' : 'First Window Session',
            lifecycle: 'closed' }],
        };
      } else if (name === 'workflow-session') {
        data = {
          ...webResponse(name, body), session_id: body.session_id, title: 'Second Window Session record',
          lifecycle: 'closed', running_call: false, running_jobs: 0,
          linked_windows: [{ client_window_key: targetKey, source: 'openai-session',
            first_linked_at_ms: 1, last_linked_at_ms: 2, last_seen_at_ms: 2, active_count: 0,
            relations: ['recording'], relation_count: 1, recorder_gap_count: 0 }],
          window_activity_after_last_session_record: [], window_activity_after_last_session_record_truncated: false,
        };
      } else if (name === 'window-collaboration') {
        data = { available: true, client_window_key: body.client_window_key, history_scope: 'fixture',
          messages: [], returned: 0, truncated: false, next_before: null, can_send: false };
      } else {
        data = structuredClone(webResponse(name, body));
        if (name === 'overview') {
          data.detail_level = body.include_sessions === false ? 'primary' : 'full';
          data.projects_included = body.include_projects !== false;
          if (!data.projects_included) data.projects = [];
          if (body.include_sessions === false) data.recent_sessions.sessions = [];
        }
      }
      const delay = fixtureDelaysMs[name] || 0;
      if (delay) await new Promise(resolve => setTimeout(resolve, delay));
      await route.fulfill({ json: data });
    });
    await page.addInitScript(() => {
      localStorage.setItem('webcodex.runtime.v2.view.v1', 'work');
      localStorage.setItem('webcodex.runtime.appearance.v1', 'light');
    });
    await page.goto(fixture.url + '/runtime/');
    await page.getByTestId('work-window-row-' + targetKey).waitFor();
    const firstRows = await page.locator('.window-board-row').evaluateAll(elements =>
      elements.slice(0, 2).map(element => element.dataset.testid.slice('work-window-row-'.length)));
    assert.deepEqual(firstRows, [firstKey, targetKey]);
    await page.getByTestId('work-window-row-' + targetKey).click();
    await page.waitForFunction(() => document.querySelector('[data-testid="window-call-count"]')?.textContent.includes('160'));
    assertTabRow(await tabBounds(page), 2);
    await page.getByTestId('window-session-tag').first().click();
    const open = page.getByRole('button', { name: 'View Session record', exact: true });
    await open.waitFor();
    await page.locator('#window-activity-panel').evaluate(element => element.dataset.fixtureIdentity = 'retained-activity');
    await page.locator('.window-activity-results').evaluate(element => element.dataset.fixtureIdentity = 'retained-calls');
    await page.getByTestId('window-primary-workbench').evaluate(element => element.dataset.fixtureIdentity = 'retained-shell');

    if (width > 700) {
      await page.locator('.window-board-list').evaluate(element => element.scrollTop = Math.min(320, element.scrollHeight - element.clientHeight));
      const actual = await page.locator('.window-activity-results').evaluate(element => {
        element.scrollTop = Math.min(700, element.scrollHeight - element.clientHeight);
        return element.scrollTop;
      });
      assert(actual >= 300, 'Desktop fixture must exercise a real, scrollable call list: ' + actual);
    } else {
      // Keep the real control visible while exercising the phone's actual page
      // scroller. A normal Playwright click then measures the position seen by
      // the user's click, including any browser focus/scroll adjustment.
      const actual = await open.evaluate(button => {
        let scroller = button.parentElement;
        while (scroller && !(scroller.scrollHeight > scroller.clientHeight + 1
          && ['auto', 'scroll'].includes(getComputedStyle(scroller).overflowY))) scroller = scroller.parentElement;
        scroller ||= document.scrollingElement;
        const origin = scroller === document.scrollingElement ? 0 : scroller.getBoundingClientRect().top;
        const target = scroller.scrollTop + button.getBoundingClientRect().top - origin - 220;
        scroller.scrollTop = Math.max(0, Math.min(target, scroller.scrollHeight - scroller.clientHeight));
        return { top: scroller.scrollTop, owner: scroller.className || scroller.tagName };
      });
      assert(actual.top >= 40, 'Phone fixture must exercise a nonzero page scroll: ' + JSON.stringify(actual));
    }
    await paint(page);
    await page.screenshot({ path: new URL('window-session-' + width + '-activity.png', output).pathname });
    await open.evaluate((button, selectors) => {
      button.addEventListener('click', () => {
        window.__fixtureRecordStartedAt = performance.now();
        window.__fixtureRecordScroll = Object.fromEntries(Object.entries(selectors).map(([name, selector]) => [name,
          (selector ? document.querySelector(selector) : document.scrollingElement)?.scrollTop || 0]));
      }, { capture: true, once: true });
    }, selectors);
    const beforeRequest = requests.length;
    await open.click();
    await page.getByRole('heading', { name: 'Second Window Session record', exact: true }).waitFor();
    const navigation = await page.evaluate(() => ({
      availabilityObservedMs: Math.round(performance.now() - window.__fixtureRecordStartedAt),
      workbenchCount: document.querySelectorAll('[data-testid="window-primary-workbench"]').length,
      windowDetailCount: document.querySelectorAll('.window-board-detail').length,
      selectedWindow: document.querySelector('.window-board-row[aria-current="true"]')?.getAttribute('data-testid') || null,
      sessionRecordCount: document.querySelectorAll('.session-main').length,
      embedded: Boolean(document.querySelector('.window-board-detail .window-session-record')),
    }));
    const recordRequests = requests.slice(beforeRequest);
    const extraNavigationRequests = recordRequests.filter(request => request.route === 'project-git'
      || request.route === 'overview' && request.body.include_sessions !== false).map(request => request.route);
    await page.screenshot({ path: new URL('window-session-' + width + '-record.png', output).pathname });
    if (navigation.workbenchCount !== 1 || !navigation.embedded) {
      report.checks.push({ width, sourceWindow: 'second', retainedShell: false,
        navigation, recordRequests: recordRequests.map(request => request.route), extraNavigationRequests,
        regression: 'Viewing a Window-associated Session removed its Window workbench.' });
      await page.unrouteAll({ behavior: 'wait' });
      await page.close();
      continue;
    }
    await page.getByRole('tab', { name: 'Session record', selected: true }).waitFor();
    const recordTabs = await tabBounds(page);
    assertTabRow(recordTabs, 3);
    const before = await page.evaluate(() => window.__fixtureRecordScroll);
    assert(width > 700 ? before.calls >= 300 : Math.max(before.shell, before.document) >= 40, JSON.stringify(before));
    assert.equal(await page.getByTestId('window-primary-workbench').count(), 1);
    assert.equal(await page.getByTestId('window-primary-workbench').getAttribute('data-fixture-identity'), 'retained-shell');
    assert.equal(await page.locator('.window-board-detail').count(), 1);
    assert.equal(await page.getByTestId('current-window-identity').getAttribute('title'), targetKey);
    assert.equal(await page.getByTestId('work-window-row-' + targetKey).getAttribute('aria-current'), 'true');
    assert.equal(await page.getByRole('searchbox', { name: 'Search Sessions' }).count(), 0);
    assert.deepEqual(extraNavigationRequests, []);
    assert.equal(recordRequests.filter(request => request.route === 'workflow-session').length, 1);
    const geometry = await page.evaluate(() => ({
      overflow: document.documentElement.scrollWidth > innerWidth + 1,
      record: document.querySelector('.window-session-record').getBoundingClientRect().toJSON(),
      window: document.querySelector('.window-board-detail').getBoundingClientRect().toJSON(),
    }));
    assert.equal(geometry.overflow, false);
    assert(geometry.record.width > 0 && geometry.record.height > 0);
    await page.screenshot({ path: new URL('window-session-' + width + '-record.png', output).pathname });

    await page.getByRole('button', { name: 'Back to Window', exact: true }).click();
    await page.getByRole('tab', { name: 'Window', selected: true }).waitFor();
    await paint(page);
    assert.equal(await page.locator('#window-activity-panel').getAttribute('data-fixture-identity'), 'retained-activity');
    assert.equal(await page.locator('.window-activity-results').getAttribute('data-fixture-identity'), 'retained-calls');
    assert.equal(await page.getByTestId('work-window-row-' + targetKey).getAttribute('aria-current'), 'true');
    assert.equal(await page.getByTestId('current-window-identity').getAttribute('title'), targetKey);
    const after = await scrollSnapshot(page);
    for (const [name, value] of Object.entries(before)) {
      assert(Math.abs(after[name] - value) <= 2, name + ' scroll ' + value + ' restored to ' + after[name]);
    }
    assert((await page.getByRole('combobox', { name: 'Session filter' }).inputValue()).includes('Second Window Session'));
    await page.screenshot({ path: new URL('window-session-' + width + '-returned.png', output).pathname });
    report.checks.push({ width, sourceWindow: 'second', retainedShell: true, retainedActivityDom: true,
      retainedSelection: true, before, after, noFullOverviewOrProjectGit: true,
      navigation, recordRequests: recordRequests.map(request => request.route), extraNavigationRequests,
      singleTabRow: true, recordTabs, geometry });
    await page.unrouteAll({ behavior: 'wait' });
    await page.close();
  }
  assert.deepEqual(report.errors, []);
  assert(report.checks.every(check => check.retainedShell),
    'Session navigation must retain the Window shell: ' + JSON.stringify(report.checks));
} catch (error) {
  report.failure = error.stack || String(error);
  throw error;
} finally {
  fs.writeFileSync(new URL('report.json', output), JSON.stringify(report, null, 2));
  await browser?.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
console.log(JSON.stringify({ passed: report.checks.length, output: output.pathname, checks: report.checks }));
