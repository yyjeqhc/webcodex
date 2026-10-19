import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Production renderers, isolated observations: no native backend or live service.
const output = process.env.WEBCODEX_UI_OUTPUT ? path.resolve(process.env.WEBCODEX_UI_OUTPUT)
  : path.resolve(import.meta.dirname, '../../artifacts/desktop-clarity/i18n-project-layout');
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true });
const report = { fixture: true, nativeBackend: false, checks: [], errors: [] };
const locales = ['en-US', 'zh-CN', 'zh-TW', 'ja-JP', 'ko-KR', 'de-DE', 'fr-FR'];
const projects = ['顧客向けダッシュボードのアクセシビリティ改善', 'internationalization-workspace-with-a-long-name'].map((name, index) => ({
  id: `agent:fixture-runner:project-${index}`, name, path: `/fixture/workspaces/${name}`,
  client_id: 'fixture-runner', connected: true,
  sessions: { active_sessions: index + 1, running_sessions: 0, latest_updated_at: 1792368000 },
}));
const runners = [
  { client_id: 'fixture-runner', connected: true, status: 'online', computer_session_availability: false },
  { client_id: 'remote-device-with-a-long-identifier', connected: false, status: 'offline' },
];

async function bounded(page, label) {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const failures = await page.evaluate(() => {
    const failures = [];
    for (const el of document.querySelectorAll('html, .main-content, .workspace-project-table, .workspace-project-table > div, .project-family-details, .project-browser-list, .workspace-tabs, .mantine-Button-label, .nav-label')) {
      if (!el.getClientRects().length) continue;
      if (el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1 && el.matches('.mantine-Button-label, .nav-label, .workspace-tabs')) {
        failures.push({ element: el.className || el.tagName, width: el.clientWidth, content: el.scrollWidth });
      }
      if (el.matches('.mantine-Button-label, .nav-label')) {
        const box = el.getBoundingClientRect(), button = el.closest('button').getBoundingClientRect();
        if (box.left < button.left - 1 || box.right > button.right + 1 || box.top < button.top - 1 || box.bottom > button.bottom + 1) {
          failures.push({ element: el.className, text: el.textContent, outsideButton: true });
        }
      }
    }
    return failures;
  });
  assert.deepEqual(failures, [], label);
  report.checks.push(label);
}

try {
  for (const locale of locales) for (const width of [1280, 1024, 768, 390]) for (const theme of ['light', 'dark']) {
    const context = await browser.newContext({ viewport: { width, height: 960 }, colorScheme: theme, reducedMotion: 'reduce' });
    await context.addInitScript(({ locale, theme }) => {
      localStorage.setItem('webcodex.desktop.locale', locale);
      localStorage.setItem('webcodex.desktop.appearance.v1', theme);
      localStorage.setItem('webcodex.runtime.v2.view.v1', 'projects');
      localStorage.setItem('webcodex.runtime.appearance.v1', theme);
    }, { locale, theme });
    const page = await context.newPage();
    page.on('pageerror', error => report.errors.push(error.message));
    await page.route('**/__fixture/desktop-state*', async route => {
      const response = await route.fetch(); const state = await response.json();
      state.workspace_runner = { client_id: 'fixture-runner', config_path: '/fixture/runner.toml', server_url: 'http://127.0.0.1:1' };
      await route.fulfill({ response, json: state });
    });
    await page.route('**/desktop-shim.js', async route => {
      const response = await route.fetch(); let source = await response.text();
      source = source.replace("localStorage.setItem('webcodex.desktop.locale','en-US');", `localStorage.setItem('webcodex.desktop.locale',${JSON.stringify(locale)});`);
      source = source.replace('switch(cmd){', `switch(cmd){
        case 'desktop_shell_restore_only': return true;
        case 'desktop_shell_bootstrap_complete': case 'set_desktop_locale': return null;
        case 'read_desktop_navigation': return null;`);
      const marker = "if(request.kind==='overview')return";
      assert(source.includes(marker));
      source = source.replace(marker, `if(request.kind==='overview')return ${JSON.stringify({ runners, projects_available: true, visible_projects: projects.length, projects, projects_truncated: false, recent_sessions: { sessions: [], truncated: false, scan_truncated: false } })};
        if(request.kind==='projects')return ${JSON.stringify({ projects, total: projects.length, truncated: false })};` + marker);
      await route.fulfill({ response, body: source });
    });
    await page.goto(fixture.url + '/desktop/');
    await page.waitForFunction(({ locale, theme }) => document.documentElement.lang === locale && document.documentElement.dataset.theme === theme, { locale, theme });
    await page.locator('[data-webcodex-action="navigate-projects"]').click();
    await page.locator('.project-access-help').waitFor();
    await page.locator('.runner-device').first().waitFor();
    assert.equal(await page.locator('.error-card').count(), 0, 'the fixture must not mask layout with a startup error');
    assert.equal(await page.locator('.project-access-help').getAttribute('open'), null);
    assert.equal(await page.locator('.runner-device-details[open]').count(), 0);
    assert.equal(await page.locator('.project-selection-actions .mantine-Button-root').count(), 1, 'no empty destructive batch action');
    assert.equal(await page.locator('.workspace-project-table').count(), width > 900 ? 1 : 0);
    await bounded(page, `desktop projects ${locale} ${width} ${theme}`);
    if (['ja-JP', 'zh-CN', 'de-DE'].includes(locale) && [1280, 768, 390].includes(width)) {
      await page.screenshot({ path: path.join(output, `desktop-${locale}-${width}-${theme}.png`) });
    }
    await page.locator('.project-access-help > summary').click();
    await page.locator('.runner-device-details > summary').first().click();
    await bounded(page, `desktop expanded help ${locale} ${width} ${theme}`);
    // Selection is local UI state; it must not prepare or execute removal.
    await page.locator('input[type="checkbox"]').first().check();
    assert.equal(await page.locator('.project-selection-actions .mantine-Button-root').count(), 2);
    await bounded(page, `desktop selection ${locale} ${width} ${theme}`);
    for (const destination of ['activity', 'settings', 'extensions']) {
      await page.locator(`[data-webcodex-action="navigate-${destination}"]`).click();
      await page.locator(`[data-webcodex-page="${destination}"]`).waitFor();
      await bounded(page, `desktop ${destination} ${locale} ${width} ${theme}`);
    }
    const effects = await page.evaluate(() => window.__fixtureCalls.filter(call => /^(prepare_project|save_|update_|start_|stop_|restart_|remove_|unregister_|register_|authorize_|environment_service_action)/.test(call.cmd)));
    assert.deepEqual(effects, [], 'browsing and selecting must not mutate authority or services');

    await page.route('**/runtime/', async route => {
      const response = await route.fetch();
      const language = locale === 'en-US' ? 'en' : locale;
      await route.fulfill({ response, body: (await response.text()).replace('"webcodex.runtime.language.v1","en"', `"webcodex.runtime.language.v1",${JSON.stringify(language)}`) });
    });
    await page.route('**/api/runtime-console/projects', async route => {
      const response = await route.fetch(); const data = await response.json();
      data.projects = data.projects.map((project, index) => ({ ...project, name: projects[index % projects.length].name, path: projects[index % projects.length].path }));
      await route.fulfill({ response, json: data });
    });
    await page.goto(fixture.url + '/runtime/');
    await page.waitForFunction(({ locale, theme }) => document.documentElement.lang === (locale === 'en-US' ? 'en' : locale) && document.documentElement.dataset.resolvedTheme === theme, { locale, theme });
    await page.locator('.project-card').first().waitFor();
    await page.locator('.project-family-details').waitFor();
    await bounded(page, `web projects ${locale} ${width} ${theme}`);
    if (locale === 'ja-JP' && [1280, 390].includes(width)) await page.screenshot({ path: path.join(output, `web-${locale}-${width}-${theme}.png`), fullPage: true });
    await context.close();
  }
  assert.deepEqual(report.errors, []);
  console.log(JSON.stringify({ checks: report.checks.length, errors: report.errors }));
} finally {
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2));
  await browser.close();
  await new Promise(resolve => fixture.server.close(resolve));
}
