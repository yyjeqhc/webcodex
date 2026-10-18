import assert from 'node:assert/strict';
import fs from 'node:fs';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';
import { installDesktopPageFixtures } from './desktop-page-fixtures.mjs';

const output = new URL('../../artifacts/desktop-soft-surfaces/resize-theme/', import.meta.url);
fs.mkdirSync(output, { recursive: true });
const fixture = await startFixtureServer();
const browser = await chromium.launch({ headless: true, ignoreDefaultArgs: ['--hide-scrollbars'] });
const sizes = [[1440,960],[1280,760],[1024,580],[780,580],[1180,760],[1440,960]];
const report = { fixture:true, nativeBackend:false, sizes, checks:0, surfaces:[], screenshots:[], failures:[], errors:[] };
const check=(ok,label,detail)=>{report.checks++;if(!ok)report.failures.push({label,detail});};
const settle=page=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
async function theme(page,value) {
  // Exercise the existing preference owner while preserving the mounted view.
  await page.evaluate(value=>{
    localStorage.setItem('webcodex.desktop.appearance.v1',value);
    dispatchEvent(new CustomEvent('webcodex:appearance-change',{detail:value}));
  },value);
  await page.waitForFunction(value=>document.documentElement.dataset.theme===value&&document.documentElement.dataset.mantineColorScheme===value,value);
  await page.evaluate(async () => {
    const transitions = document.getAnimations().filter(animation => animation instanceof CSSTransition);
    await Promise.allSettled(transitions.map(animation => animation.finished));
  });
  await settle(page);
}
async function snapshot(page) {
  return page.evaluate(()=>{
    const box=node=>{if(!node)return null;const b=node.getBoundingClientRect();return {x:b.x,y:b.y,w:b.width,h:b.height};};
    const main=document.querySelector('.main-content'), rail=document.querySelector('.sidebar');
    const heading=main.querySelector('h1'), tabs=[...main.querySelectorAll('[role=tablist]')];
    const selected=document.querySelector('.sidebar nav [aria-current=page]');
    return { viewport:{w:innerWidth,h:innerHeight}, heading:box(heading), main:box(main), mainScroll:{top:main.scrollTop,w:main.scrollWidth,client:main.clientWidth}, rail:box(rail), railScroll:{top:rail.scrollTop,h:rail.scrollHeight,client:rail.clientHeight,overflow:getComputedStyle(rail).overflowY}, selected:box(selected), selectedFill:getComputedStyle(selected).backgroundColor, selectedInk:getComputedStyle(selected).color, tabs:tabs.map(node=>({box:box(node),buttons:[...node.querySelectorAll('button')].map(button=>({box:box(button),selected:button.getAttribute('aria-selected')}))})), drafts:[...main.querySelectorAll('input:not([type=checkbox]):not([type=radio]),textarea,select')].filter(node=>node.id!=='desktop-settings-appearance').map(node=>({id:node.id,value:node.value})), settingsAppearance:main.querySelector('#desktop-settings-appearance')?.value, confirmation: (() => {const node=main.querySelector('.update-confirmation');return node?{box:box(node),width:node.scrollWidth,client:node.clientWidth,actions:[...node.querySelectorAll('button')].map(box)}:null;})(), rootScroll:document.documentElement.scrollWidth };
  });
}
function aligned(before,after,label) {
  for(const key of ['heading','main','selected']) if(before[key]&&after[key])for(const coordinate of ['x','y','w','h'])check(Math.abs(before[key][coordinate]-after[key][coordinate])<=1,label+' theme-stable '+key+'.'+coordinate,{before:before[key],after:after[key]});
  check(JSON.stringify(before.confirmation)===JSON.stringify(after.confirmation),label+' stable update confirmation and actions');
  check(JSON.stringify(before.drafts)===JSON.stringify(after.drafts),label+' preserves draft controls');
  check(JSON.stringify(before.tabs)===JSON.stringify(after.tabs),label+' preserves tab positions and selection');
}
async function verifySurface(page,id,locale) {
  report.surfaces.push(locale+'/'+id);
  for(const [index,[width,height]] of sizes.entries()) {
    await page.setViewportSize({width,height});await settle(page);
    const preferences=page.locator('.sidebar-preferences');
    await preferences.scrollIntoViewIfNeeded();await settle(page);
    await theme(page,'light');const light=await snapshot(page);
    check(light.mainScroll.w<=light.mainScroll.client+1&&light.rootScroll<=width+1,`${locale}/${id}/${width}x${height} contained`,light);
    check(light.rail.y>=0&&light.rail.y+light.rail.h<=height+1,`${locale}/${id}/${width}x${height} sidebar contained`,light.rail);
    const prefBounds=await preferences.boundingBox();
    check(prefBounds.y>=0&&prefBounds.y+prefBounds.height<=height+1,`${locale}/${id}/${width}x${height} preferences reachable`,{prefBounds,rail:light.railScroll});
    await theme(page,'dark');const dark=await snapshot(page);
    check(dark.mainScroll.w<=dark.mainScroll.client+1&&dark.rootScroll<=width+1,`${locale}/${id}/${width}x${height} dark contained`,dark);
    if(dark.settingsAppearance)check(dark.settingsAppearance==='dark',`${locale}/${id} theme setting follows its owner`);
    aligned(light,dark,`${locale}/${id}/${width}x${height}`);
    check(dark.selectedFill==='rgb(245, 245, 245)'&&dark.selectedInk==='rgb(25, 25, 25)',`${locale}/${id}/${width}x${height} dark selected navigation`,{fill:dark.selectedFill,ink:dark.selectedInk});
    check(light.selectedFill==='rgb(25, 25, 25)'&&light.selectedInk==='rgb(255, 255, 255)',`${locale}/${id}/${width}x${height} light selected navigation`);
    const filename=`${locale}-${id}-${index}-${width}x${height}-dark.png`;
    await page.screenshot({path:new URL(filename,output).pathname,animations:'disabled'});report.screenshots.push(filename);
    await theme(page,'light');const returned=await snapshot(page);aligned(dark,returned,`${locale}/${id}/${width}x${height} return`);
    if(dark.confirmation) {
      const panel=page.locator('.update-confirmation');await panel.scrollIntoViewIfNeeded();
      for(const action of await panel.locator('button').all()) {
        await action.scrollIntoViewIfNeeded();const box=await action.boundingBox();
        check(box.x>=0&&box.x+box.width<=width+1&&box.y>=0&&box.y+box.height<=height+1,`${locale}/${id} update action reachable`,box);
      }
      const box=await panel.boundingBox();check(box.x>=0&&box.x+box.width<=width+1,`${locale}/${id} update panel contained`,box);
    }

  }
  await page.emulateMedia({colorScheme:'dark'});
  await page.evaluate(()=>dispatchEvent(new CustomEvent('webcodex:appearance-change',{detail:'system'})));
  await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark');
  await page.emulateMedia({colorScheme:'light'});
  await page.waitForFunction(()=>document.documentElement.dataset.theme==='light');
  check(true,locale+'/'+id+' follows system scheme while mounted');
  fs.writeFileSync(new URL('report.json',output),JSON.stringify(report,null,2));
}
try {
  for(const locale of ['en-US','zh-CN']) {
    const context=await browser.newContext({viewport:{width:1440,height:960},reducedMotion:'reduce'});
    const page=await context.newPage();page.setDefaultTimeout(10000);page.on('pageerror',error=>report.errors.push(error.message));
    await installDesktopPageFixtures(page,locale);
    await page.goto(fixture.url+'/desktop/');await page.locator('[data-webcodex-page=home]').waitFor();
    for(const destination of ['home','projects','activity','connection','console','extensions','settings']) {
      await page.locator(`[data-webcodex-action=navigate-${destination}]`).click();await page.locator(`[data-webcodex-page=${destination}]`).waitFor();
      if(destination==='extensions'||destination==='settings') {
        const ids=destination==='extensions'?['codingAgents','sshResources','mcpProviders','nativePlugins','skills','instructions']:['general','access','network','runtime','configuration','diagnostics','about'];
        for(const category of ids) {
          await page.locator(`#${destination==='extensions'?'extension':'settings'}-tab-${category}`).click();
          if(category==='network') { await page.locator('#desktop-tunnel-proxy-mode').selectOption('custom'); await page.locator('#desktop-tunnel-proxy-url').fill('http://127.0.0.1:7890'); }
          await verifySurface(page,destination+'-'+category,locale);
        }
      } else if(destination==='activity')for(const category of ['windows','sessions','system']) {
        await page.locator('#activity-tab-'+category).click();await verifySurface(page,'activity-'+category,locale);
      } else await verifySurface(page,destination,locale);
    }
    check((await page.evaluate(()=>window.__fixtureCalls.filter(call=>/^(configure_environment|install_verified_update|save_|update_|start_|stop_|restart_|remove_|register_|authorize_|cancel_)/.test(call.cmd)))).length===0,locale+' browsing has no native effects');
    await context.close();
    const setupContext=await browser.newContext({viewport:{width:1440,height:960},reducedMotion:'reduce'});
    const setup=await setupContext.newPage();setup.setDefaultTimeout(10000);setup.on('pageerror',error=>report.errors.push(error.message));
    await installDesktopPageFixtures(setup,locale,'onboarding');await setup.goto(fixture.url+'/desktop/');await setup.locator('[data-webcodex-page=first-run]').waitFor();
    await verifySurface(setup,'first-run',locale);
    for(const mode of ['local','remote','quick-share']) {
      if(mode==='quick-share')await setup.locator('details').first().evaluate(node=>{node.open=true;});
      await setup.locator(`[data-webcodex-action=choose-${mode}-setup]`).click();await setup.locator('[data-webcodex-page=setup]').waitFor();
      await verifySurface(setup,'setup-'+mode,locale);
      const back=setup.locator('.back-button');
      if(await back.count())await back.click();else await setup.getByRole('button',{name:locale==='en-US'?'Back':'返回',exact:true}).click();
      await setup.locator('[data-webcodex-page=first-run]').waitFor();
    }
    check((await setup.evaluate(()=>window.__fixtureCalls.filter(call=>/^(configure_environment|install_verified_update|save_|update_|start_|stop_|restart_|remove_|register_|authorize_|cancel_)/.test(call.cmd)))).length===0,locale+' onboarding browsing has no native effects');
    await setupContext.close();
    for(const mode of ['operation','update','completion']) {
      const extraContext=await browser.newContext({viewport:{width:1440,height:960},reducedMotion:'reduce'});
      const extra=await extraContext.newPage();extra.setDefaultTimeout(10000);extra.on('pageerror',error=>report.errors.push(error.message));
      await installDesktopPageFixtures(extra,locale,mode);await extra.goto(fixture.url+'/desktop/');
      if(mode==='completion') {
        await extra.locator('[data-webcodex-action=choose-local-setup]').click();
        await extra.locator('[data-webcodex-action=configure-local]').click();
        await extra.locator('[data-webcodex-page=setup-completion]').waitFor();
      } else await extra.locator('[data-webcodex-page=home]').waitFor();
      if(mode==='update') {
        await extra.locator('.runtime-update-banner .primary-button').click();
        await extra.getByRole('alertdialog').waitFor();
      }
      await verifySurface(extra,mode==='update'?'update-confirmation':mode,locale);
      if(mode==='update') {
        await extra.keyboard.press('Escape');await extra.getByRole('alertdialog').waitFor({state:'hidden'});
        check(await extra.locator('.runtime-update-banner .primary-button').evaluate(node=>node===document.activeElement),'Update cancellation restores opener');
      }
      const effects=await extra.evaluate(()=>window.__fixtureCalls.filter(call=>/^(configure_environment|install_verified_update|save_|update_|start_|stop_|restart_|remove_|register_|authorize_|cancel_)/.test(call.cmd)));
      check(effects.length===(mode==='completion'?1:0)&&effects.every(call=>call.cmd==='configure_environment'),locale+'/'+mode+' no unplanned fixture effects');
      await extraContext.close();
    }
  }
  assert.equal(report.errors.length,0,JSON.stringify(report.errors));assert.equal(report.failures.length,0,JSON.stringify(report.failures.slice(0,3)));
  report.passed=true;console.log(JSON.stringify({surfaces:report.surfaces.length,checks:report.checks,screenshots:report.screenshots.length}));
} catch(error) {report.failure=error.stack;throw error;} finally {fs.writeFileSync(new URL('report.json',output),JSON.stringify(report,null,2));await browser.close();await new Promise(resolve=>fixture.server.close(resolve));}
