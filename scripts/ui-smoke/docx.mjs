import { chromium } from "playwright";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile, mkdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { resolve, dirname } from "node:path";
import { docxFixture } from "../../frontend/test/fixtures/docx.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const bytes = Buffer.from(await docxFixture({ unsafe: true }));
const identity = { project: "agent:docx:demo", path: "reports/sample.docx", name: "sample.docx", bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") };
const app = await readFile(resolve(root, "src/mcp_docx_app.html"));
const host = `<!doctype html><html><body style="margin:0;background:#e8ebef"><iframe id="reader" style="border:0;width:460px;height:720px" sandbox="allow-scripts allow-same-origin allow-downloads" src="/viewer"></iframe><script>
const viewer=document.getElementById('reader');let count=0,stale=false,hold=false,held=[];const selected=${JSON.stringify(identity)},encoded=${JSON.stringify(bytes.toString("base64"))};
window.select=(patch={})=>viewer.contentWindow.postMessage({jsonrpc:'2.0',method:'ui/notifications/tool-result',params:{_meta:{'webcodex/docxDocument':{success:true,output:{docx_document:{...selected,...patch}}}}}},'*');
window.fixture={get count(){return count},get held(){return held.length},setStale(value){stale=value},setHold(value){hold=value},release(){for(const response of held)response();held=[]}};
addEventListener('message',event=>{if(event.source!==viewer.contentWindow||event.data?.jsonrpc!=='2.0')return;const message=event.data;
const reply=result=>viewer.contentWindow.postMessage({jsonrpc:'2.0',id:message.id,result},'*');
if(message.method==='ui/initialize')reply({hostContext:{theme:'light',displayMode:'inline'}});
if(message.method==='ui/notifications/initialized')window.select();
if(message.method==='ui/request-display-mode'){viewer.style.width='1000px';reply({mode:'fullscreen'})}
if(message.method==='tools/call'){count++;const args=message.params.arguments;const value=stale?{success:false,output:{error_kind:'snapshot_changed'}}:{success:true,output:{docx_chunk:{project:args.project,path:args.path,sha256:args.sha256,bytes_total:args.bytes,byte_offset:0,next_byte_offset:null,complete:true}}};const response=()=>reply({content:[{type:'text',text:JSON.stringify(value)}],_meta:{'webcodex/docxChunk':{content_base64:encoded}}});hold?held.push(response):response()}
});</script></body></html>`;
const server = createServer((req, res) => {
  if (req.url === "/viewer") {
    res.setHeader("Content-Security-Policy", "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'; font-src 'none'; frame-src 'none';");
    res.setHeader("Content-Type", "text/html; charset=utf-8"); res.end(app);
  } else { res.setHeader("Content-Type", "text/html; charset=utf-8"); res.end(host); }
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let browser;
try {
  browser = await chromium.launch({ headless: true, ...(process.env.WEBCODEX_UI_BROWSER_CHANNEL ? { channel: process.env.WEBCODEX_UI_BROWSER_CHANNEL } : {}) });
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 }, acceptDownloads: true });
  const errors = [], external = []; page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => { if (!/^(?:http:\/\/127\.0\.0\.1:|data:|blob:)/.test(request.url())) external.push(request.url()); });
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const frame = page.frameLocator("#reader");
  await frame.locator("#download").waitFor({ state: "visible" });
  assert.equal(await frame.locator("section.docx").count(), 2); assert.equal(await frame.locator("table td").count(), 4);
  assert.ok(await frame.locator("#stage img").evaluate(img => img.complete && img.naturalWidth === 32), "embedded raster image must decode");
  assert.match(await frame.locator("#stage").textContent(), /DOCX 阅读器验证/);
  assert.equal(await frame.locator("iframe,script:not(body > script),a[href^='javascript:']").count(), 0);
  assert.equal(await page.evaluate(() => fixture.count), 1);
  await page.evaluate(() => select());
  assert.equal(await page.evaluate(() => fixture.count), 1);
  await frame.getByRole("button", { name: "Zoom in", exact: true }).click();
  await frame.getByRole("button", { name: "Fit width", exact: true }).click();
  const fit = await frame.locator("section.docx").first().evaluate(element => ({ page: element.getBoundingClientRect().width, viewport: document.getElementById("viewport").clientWidth }));
  assert.ok(fit.page <= fit.viewport, "fit width must fit the document page in a narrow reader");
  const downloadEvent = page.waitForEvent("download"); await frame.locator("#download").click();
  assert.equal((await downloadEvent).suggestedFilename(), "sample.docx");
  await frame.getByRole("button", { name: "Expand", exact: true }).click();
  await page.waitForFunction(() => document.getElementById("reader").style.width === "1000px");
  await frame.getByRole("button", { name: "Fit width", exact: true }).click();
  const output = resolve(root, "artifacts/docx-viewer"); await mkdir(output, { recursive: true });
  await page.screenshot({ path: resolve(output, "reader.png") });
  await page.evaluate(() => { fixture.setStale(true); select({ path: "reports/changed.docx", name: "changed.docx" }); });
  await frame.getByRole("button", { name: "Retry", exact: true }).waitFor({ state: "visible" });
  assert.match(await frame.locator("#status").textContent(), /Document changed/);
  await page.evaluate(() => { fixture.setStale(false); fixture.setHold(true); select({ path: "reports/pending.docx", name: "pending.docx" }); });
  await page.waitForFunction(() => fixture.held === 1);
  await page.evaluate(() => { fixture.setHold(false); select({ path: "reports/latest.docx", name: "latest.docx" }); fixture.release(); });
  await frame.locator("#download").waitFor({ state: "visible" });
  assert.equal(await frame.locator("#filename").textContent(), "latest.docx");
  assert.equal(await frame.locator("section.docx").count(), 2);
  const old = await page.evaluate(() => fixture.count);
  await page.evaluate(() => { const reader=document.getElementById("reader");reader.contentWindow.postMessage({jsonrpc:'2.0',id:900,method:'ui/resource-teardown',params:{}},'*'); });
  await frame.locator("#download").waitFor({ state: "hidden" });
  await page.evaluate(() => select()); assert.equal(await page.evaluate(() => fixture.count), old);
  assert.deepEqual(errors, []); assert.deepEqual(external, []);
  console.log("DOCX browser smoke passed: private/text fallback, rendering, zoom/fit, download, fullscreen, stale version, superseded read, teardown; no external resources.");
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
