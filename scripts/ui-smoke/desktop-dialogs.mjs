import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { startFixtureServer } from './server.mjs';
import { dialogFixture } from './desktop-dialog-fixtures.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=path.join(root,'artifacts/desktop-soft-surfaces/dialogs');
fs.mkdirSync(output,{recursive:true});
const report={fixture:true,nativeBackend:false,checks:[],failures:[],errors:[],geometry:[],screenshots:[],nativeCalls:[],surfaces:[],limitations:['Native OS file picker and permission admission effects are replaced by an isolated invoke shim.','Destructive confirmation, authorization, credential issuance/copy, process and service final actions are never submitted.','Busy/loading/error variants are not implied by idle-dialog coverage.']};
const fixture=await startFixtureServer();let browser;let current='setup';
const check=(ok,label,detail)=>{(ok?report.checks:report.failures).push(ok?`${current}: ${label}`:{surface:current,label,detail});};
const nav=async(p,id)=>{await p.locator(`[data-webcodex-action="navigate-${id}"]`).click();await p.locator(`[data-webcodex-page="${id}"]`).waitFor();};
const tab=async(p,group,id)=>{await p.locator(`#${group}-tab-${id}`).click();};
const button=(p,re)=>p.getByRole('button',{name:re}).first();
const ext=async(p,id)=>{await nav(p,'extensions');await tab(p,'extension',id);};
const settings=async(p,id)=>{await nav(p,'settings');await tab(p,'settings',id);};
const cases=[
 ['project-add-folder',async p=>{await nav(p,'projects');await button(p,/Add local project|添加本机项目/).click();}],
 ['project-unregister',async p=>{await nav(p,'projects');await p.locator('.workspace-project-table tbody tr').first().getByRole('button',{name:/Remove project|移除项目/}).click();}],
 ['project-unregister-batch',async p=>{await nav(p,'projects');await button(p,/Select all shown|全选当前列表/).click();await button(p,/Remove selected|批量移除/).click();}],
 ['project-add-device',async p=>{await nav(p,'projects');await button(p,/Add device|添加设备/).click();}],
 ['connection-create',async p=>{await nav(p,'connection');await p.locator('.page-heading-row > button').click();}],
 ['connection-cloudflare-named-create',async p=>{await nav(p,'connection');await p.locator('.page-heading-row > button').click();await p.locator('.workspace-dialog select').selectOption('cloudflare_named');}],
 ['connection-cloudflare-quick-create',async p=>{await nav(p,'connection');await p.locator('.page-heading-row > button').click();await p.locator('.workspace-dialog select').selectOption('cloudflare_quick');}],
 ['connection-cloudflare-edit',async p=>{await nav(p,'connection');await p.locator('[data-tunnel-profile-id=fixture-cloudflare] .connection-actions button').first().click();}],
 ['connection-edit',async p=>{await nav(p,'connection');await p.locator('[data-tunnel-profile-id="fixture-connection"] .connection-actions button').filter({hasText:/Edit|编辑/}).click();}],
 ['connection-delete',async p=>{await nav(p,'connection');await p.locator('[data-tunnel-profile-id="fixture-connection"] .connection-actions button').filter({hasText:/Delete|删除/}).click();}],
 ['connection-cloudflare-oauth',async p=>{await nav(p,'connection');await p.locator('[data-tunnel-profile-id="fixture-cloudflare"] .connection-actions button').filter({hasText:/OAuth/}).click();}],
 ['coding-agent-create',async p=>{await ext(p,'codingAgents');await button(p,/Add Coding Agent|添加编程代理/).click();}],
 ['coding-agent-edit',async p=>{await ext(p,'codingAgents');await p.getByRole('button',{name:/Edit Fixture agent|编辑 Fixture agent/}).click();}],
 ['coding-agent-remove',async p=>{await ext(p,'codingAgents');await p.getByRole('button',{name:'Remove Fixture agent',exact:true}).click();}],
 ['coding-agent-authorize',async p=>{await ext(p,'codingAgents');await p.getByRole('button',{name:'Authorize Runner Capabilities',exact:true}).click();}],
 ['ssh-create',async p=>{await ext(p,'sshResources');await p.getByRole('button',{name:'Add SSH Resource',exact:true}).click();}],
 ['ssh-remove',async p=>{await ext(p,'sshResources');await p.getByRole('button',{name:'Remove fixture-ssh',exact:true}).click();}],
 ['ssh-authorize',async p=>{await ext(p,'sshResources');await p.getByRole('button',{name:'Authorize Runner Capabilities',exact:true}).click();}],
 ['mcp-create',async p=>{await ext(p,'mcpProviders');await p.locator('.mcp-providers .extension-toolbar button').click();}],
 ['mcp-edit',async p=>{await ext(p,'mcpProviders');await p.locator('[data-mcp-provider-id] .connection-actions button').first().click();}],
 ['mcp-delete',async p=>{await ext(p,'mcpProviders');await p.locator('[data-mcp-provider-id] .connection-actions button').last().click();}],
 ['plugin-register',async p=>{await ext(p,'nativePlugins');await p.locator('.plugin-registration').getByRole('button').first().click();}],
 ['instruction-preview',async p=>{await ext(p,'instructions');await p.getByRole('button',{name:/Open AGENTS.md|打开 AGENTS.md/}).click();}],
 ['instruction-runner-preview',async p=>{await ext(p,'instructions');await p.getByRole('button',{name:/Open RUNNER.md|打开 RUNNER.md/}).click();}],
 ['runtime-restart-warning',async p=>{await settings(p,'runtime');await p.locator('.runtime-candidate > button').click();}],
 ['runner-capacity-restart',async p=>{await settings(p,'runtime');await button(p,/^Restart Runner…$|^重启 Runner…$/).click();}],
 ['diagnostics-full-trace',async p=>{await settings(p,'diagnostics');await p.locator('#desktop-trace-mode').selectOption('full');await p.locator('.diagnostics-panel').getByRole('button',{name:/^Save$|^保存$/}).click();}],
 ['diagnostics-restart',async p=>{await settings(p,'diagnostics');await p.locator('#desktop-trace-mode').selectOption('metadata');await button(p,/Save and restart Server|保存并重启/).click();}],
 ['diagnostics-restore',async p=>{await settings(p,'diagnostics');await button(p,/Restore previous configuration|恢复之前的配置/).click();}],
 ['console-sensitive-clipboard',async p=>{await nav(p,'console');await button(p,/Copy Runtime Console credential|复制 Runtime Console 凭据/).click();}],
 ['activity-window',async p=>{await nav(p,'activity');await p.locator('.activity-work-row').first().click();}],
 ['activity-window-to-session',async p=>{await nav(p,'activity');await p.locator('.activity-work-row').first().click();await p.locator('.workspace-dialog .workspace-activity-link').click();}],
 ['activity-session',async p=>{await nav(p,'activity');await tab(p,'activity','sessions');await p.locator('.workspace-session-row').first().click();}],
 ['computer-permissions-welcome',async()=>{}],
];
report.inventory=cases.map(([id])=>id);
async function theme(p,value){await p.evaluate(value=>{localStorage.setItem('webcodex.desktop.appearance.v1',value);window.dispatchEvent(new CustomEvent('webcodex:appearance-change',{detail:value}));},value);await p.waitForTimeout(80);}
async function geometry(p){return p.locator('[role="dialog"]:visible,dialog[open]').last().evaluate(el=>{const r=el.getBoundingClientRect(),s=getComputedStyle(el);const h=el.querySelector('.workspace-dialog-header,h2');const hr=h?.getBoundingClientRect();const b=el.querySelector('.workspace-dialog-body')||el;const bs=getComputedStyle(b);return {x:r.x,y:r.y,width:r.width,height:r.height,client:el.clientWidth,scroll:el.scrollWidth,viewport:[innerWidth,innerHeight],padding:bs.padding,header:hr&&{x:hr.x,y:hr.y,width:hr.width,height:hr.height,padding:getComputedStyle(h).padding},background:s.backgroundColor,focused:el.contains(document.activeElement)};});}
try{
 browser=await chromium.launch({headless:true,...(process.env.WEBCODEX_SMOKE_BROWSER?{executablePath:process.env.WEBCODEX_SMOKE_BROWSER}:{})});
 for(const locale of (process.env.DIALOG_LOCALES||'en-US,zh-CN').split(','))for(const [id,open] of cases){
  if(process.env.DIALOG_FILTER&&!id.includes(process.env.DIALOG_FILTER))continue;
  current=`${id}-${locale}`;const context=await browser.newContext({viewport:{width:1440,height:960},reducedMotion:'reduce'});await context.addInitScript(()=>document.addEventListener('click',event=>{if(!document.querySelector('[role=dialog],dialog[open]')){const button=event.target.closest('button');if(button)window.__fixtureDialogRoot=button;}},true));const p=await context.newPage();p.setDefaultTimeout(2200);
  p.on('pageerror',e=>{console.error(current,e.message);report.errors.push({surface:current,message:e.message});});
  await p.route('**/desktop-shim.js',async route=>{const response=await route.fetch();let body=dialogFixture(await response.text(),locale);if(id==='project-add-device'||id.startsWith('connection-cloudflare-'))body=body.replace('state.workspace_runner=',"state.persistent_environment='fixture-env';state.workspace_runner=");await route.fulfill({response,body});});
  try{
   await p.goto(fixture.url+'/desktop/'+(id==='computer-permissions-welcome'?'?permissions=1':''));await p.locator('[data-webcodex-page="home"]').waitFor();
   await open(p);const dialog=p.locator('[role="dialog"]:visible,dialog[open]').last();await dialog.waitFor();await p.waitForTimeout(120);
   report.surfaces.push({id,locale,state:'mounted',automaticOpener:id==='computer-permissions-welcome',title:await dialog.locator('h2').first().textContent().catch(()=>null)});
   check(await dialog.evaluate(el=>el.contains(document.activeElement)),'focus inside dialog on open');await p.keyboard.press('Tab');check(await dialog.evaluate(el=>el.contains(document.activeElement)),'Tab keeps focus inside modal');await p.keyboard.press('Shift+Tab');check(await dialog.evaluate(el=>el.contains(document.activeElement)),'Shift+Tab keeps focus inside modal');
   for(const [step,[width,height]] of [[1440,960],[1024,760],[780,580],[1440,960]].entries()){
    await p.setViewportSize({width,height});await p.waitForTimeout(60);let before;
    for(const mode of ['light','dark']){
     await theme(p,mode);check(await p.evaluate(value=>document.documentElement.dataset.theme===value,mode),`${width} ${mode} owner hook applies theme`);const g=await geometry(p);report.geometry.push({surface:current,mode,...g});
     check(g.x>=-1&&g.y>=-1&&g.x+g.width<=width+1&&g.y+g.height<=height+1&&g.scroll<=g.client+1,`${width}x${height} ${mode} containment`,g);
     check(g.focused,`${width} ${mode} theme keeps focus in dialog`);if(id!=='computer-permissions-welcome')check(parseFloat(g.padding)>=16&&g.header?.height>=40,`${width} readable body/header padding`,g);
     if(before)check(['x','y','width','height'].every(k=>Math.abs(g[k]-before[k])<1),`${width} theme flip stable bounds`,{before,after:g});before=g;
     const filename=`${current}-${step}-${width}x${height}-${mode}.png`;await p.screenshot({path:path.join(output,filename),animations:'disabled',caret:'hide'});report.screenshots.push(filename);
    }
    const last=dialog.locator('button:not([disabled])').last();if(!id.startsWith('activity-')&&!id.startsWith('instruction-')&&await last.count()){
     await dialog.hover();for(let i=0;i<3;i++)await p.mouse.wheel(0,1800);
     const reached=await last.evaluate(el=>{const r=el.getBoundingClientRect();return r.top>=0&&r.bottom<=innerHeight&&r.left>=0&&r.right<=innerWidth;});check(reached,`${width} footer reachable by ordinary wheel`);if(width===780){const filename=`${current}-footer-780.png`;await p.screenshot({path:path.join(output,filename),animations:'disabled',caret:'hide'});report.screenshots.push(filename);}
    }
   }
   await p.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});check(true,'Escape dismisses idle dialog');
   if(id!=='computer-permissions-welcome'){let restored=true;await p.waitForFunction(()=>document.activeElement===window.__fixtureDialogRoot&&document.activeElement?.isConnected&&document.activeElement.getBoundingClientRect().width>0,null,{timeout:2200}).catch(()=>{restored=false;});check(restored,'focus restored to original connected visible trigger');if(restored){await p.evaluate(()=>window.__fixtureDialogRoot.click());await dialog.waitFor();await dialog.locator('.workspace-dialog-header button').click();await dialog.waitFor({state:'hidden'});await p.waitForFunction(()=>document.activeElement===window.__fixtureDialogRoot,null,{timeout:2200});check(true,'close button dismisses and restores original trigger');}}
  }catch(e){console.error(current,e.message);report.failures.push({surface:current,label:'trigger or lifecycle failed',detail:e.message});await p.screenshot({path:path.join(output,current+'-failure.png')}).catch(()=>{});}
  const native=await p.evaluate(()=>({calls:window.__fixtureCalls||[],forbidden:window.__fixtureForbiddenCalls||[]}));report.nativeCalls.push({surface:current,...native});check(native.forbidden.length===0,'no native mutations, services, tokens or clipboard effects',native.forbidden);await context.close();fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));console.error(current,'finished');
 }
}finally{await browser?.close();await new Promise(resolve=>fixture.server.close(resolve));fs.writeFileSync(path.join(output,'report.json'),JSON.stringify(report,null,2));}
console.log(JSON.stringify({surfaces:report.surfaces.length,checks:report.checks.length,failures:report.failures,errors:report.errors,screenshots:report.screenshots.length,report:path.join(output,'report.json')},null,2));
if(report.failures.length||report.errors.length)process.exitCode=1;
