import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';

// Isolated renderer regression for the native Desktop's unbounded recent-session
// titles. Only the test fixture responds; no production Tauri/API is invoked.
const root=path.resolve(import.meta.dirname,'../..');
const fixture=await startFixtureServer();
const chrome='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const executablePath=process.env.UI_SMOKE_BROWSER||(fs.existsSync(chrome)?chrome:undefined);
let browser;
const results=[];
try {
 browser=await chromium.launch({headless:true,...(executablePath?{executablePath}:{})});
 const page=await browser.newPage();
 await page.route('**/desktop-shim.js',async route=>{
  const response=await route.fetch();let source=await response.text();
  const marker='recent_sessions:{sessions:[],truncated:false,scan_truncated:false}';
  assert.equal(source.split(marker).length,2);
  const sessions=[0,1,2].map(index=>({session_id:'wc_sess_long_activity_'+index,project_id:'agent:fixture-runner:alpha',title:'Owner review with a deliberately long session description '.repeat(35),updated_at:Math.floor(Date.now()/1000),lifecycle:'active',mode:'normal',overview:{attention:{}}}));
  source=source.replace(marker,'recent_sessions:'+JSON.stringify({sessions,truncated:false,scan_truncated:false}));
  await route.fulfill({response,body:source});
 });
 for(const width of [768,1180,1440,1920]) {
  await page.setViewportSize({width,height:900});
  await page.goto(fixture.url+'/desktop/');
  await page.locator('[data-webcodex-action="navigate-activity"]').click();
  await page.locator('#activity-tab-sessions').click();
  await page.locator('.workspace-session-row').first().waitFor();
  const metrics=await page.evaluate(()=>{
   const section=document.querySelector('#activity-view-sessions');
   const rect=section.getBoundingClientRect();
   return {viewport:innerWidth,documentWidth:document.documentElement.scrollWidth,sectionRight:rect.right,
    links:[...document.querySelectorAll('.workspace-session-row')].map(link=>({right:link.getBoundingClientRect().right,width:link.getBoundingClientRect().width}))};
  });
  results.push({width,...metrics});
  assert(metrics.documentWidth<=width+1,JSON.stringify(metrics));
  assert(metrics.links.every(link=>link.right<=metrics.sectionRight+1),JSON.stringify(metrics));
 }
 for(const width of [390,1180]) {
  await page.setViewportSize({width,height:900});
  await page.goto(fixture.url+'/desktop/');
  await page.locator('[data-webcodex-action="navigate-extensions"]').click();
  await page.getByRole('button',{name:'Add Coding Agent',exact:true}).click();
  const dialog=page.getByRole('dialog');await dialog.waitFor();
  const bounds=await dialog.evaluate(element=>{
   const rect=element.getBoundingClientRect();const header=element.querySelector('.mantine-Modal-header').getBoundingClientRect();
   return {left:rect.left,right:rect.right,headerLeft:header.left,headerRight:header.right,clientWidth:element.clientWidth,scrollWidth:element.scrollWidth};
  });
  results.push({width,dialog:bounds});
  assert(bounds.headerLeft>=bounds.left && bounds.headerRight<=bounds.right,JSON.stringify(bounds));
  assert(bounds.scrollWidth<=bounds.clientWidth+1,JSON.stringify(bounds));
  await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});
 }
 console.log(JSON.stringify({fixture:true,nativeBackend:false,passed:true,results},null,2));
} finally {
 if(browser)await browser.close();await new Promise(resolve=>fixture.server.close(resolve));
}
