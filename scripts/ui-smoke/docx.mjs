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
const host = `<!doctype html><html><body style="margin:0;background:#e8ebef"><iframe id="reader" style="border:0;width:460px;height:720px" sandbox="allow-scripts allow-same-origin" src="/viewer"></iframe><script>
const viewer=document.getElementById('reader');let count=0,stale=false,hold=false,held=[],downloads=[],downloadDenied=false;const limited=new URLSearchParams(location.search).has('limited');const selected=${JSON.stringify(identity)},raw=${JSON.stringify(bytes.toString("base64"))},file=Uint8Array.from(atob(raw),char=>char.charCodeAt(0)),chunkBytes=512*1024;const b64=value=>{let text='';for(let index=0;index<value.length;index+=32768)text+=String.fromCharCode(...value.subarray(index,index+32768));return btoa(text)};window.select=(patch={})=>viewer.contentWindow.postMessage({jsonrpc:'2.0',method:'ui/notifications/tool-result',params:{toolResult:{meta:{'webcodex/docxDocument':{success:true,output:{docx_document:{...selected,...patch}}}}}}},'*');window.fixture={get downloads(){return downloads},setDownloadDenied(value){downloadDenied=value},get count(){return count},get held(){return held.length},setStale(value){stale=value},setHold(value){hold=value},release(){for(const response of held)response();held=[]}};
addEventListener('message',event=>{if(event.source!==viewer.contentWindow||event.data?.jsonrpc!=='2.0')return;const message=event.data;
const reply=result=>viewer.contentWindow.postMessage({jsonrpc:'2.0',id:message.id,result},'*');
if(message.method==='ui/initialize')reply({hostCapabilities:limited?{}:{downloadFile:{}},hostContext:{theme:'light',displayMode:'inline',availableDisplayModes:limited?['inline']:['inline','fullscreen']}});
if(message.method==='ui/notifications/initialized')window.select();
if(message.method==='ui/request-display-mode'){viewer.style.width='1000px';reply({mode:'fullscreen'})}
if(message.method==='ui/download-file'){downloads.push(message.params.contents[0].resource);reply({isError:downloadDenied})}
if(message.method==='tools/call'){count++;const args=message.params.arguments,offset=args.byte_offset,next=Math.min(offset+chunkBytes,file.length),encoded=b64(file.subarray(offset,next));const value=stale?{success:false,output:{error_kind:'snapshot_changed'}}:{success:true,output:{artifact_chunk:{project:args.project,path:args.path,sha256:args.sha256,bytes_total:args.bytes,byte_offset:offset,next_byte_offset:next===file.length?null:next,complete:next===file.length}}};const response=()=>reply({result:{structured_content:value,meta:{'webcodex/artifactChunk':{content_base64:encoded}}}});hold?held.push(response):response()}});</script></body></html>`;const server = createServer((req, res) => {
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
  await page.evaluate(() => document.getElementById("reader").style.width = "350px");
  await page.waitForFunction(() => {
    const doc = document.getElementById("reader").contentDocument;
    return doc.querySelector("section.docx").getBoundingClientRect().width <= doc.getElementById("viewport").clientWidth;
  });
  await frame.getByRole("button", { name: "Zoom in", exact: true }).click();
  const manualZoom = await frame.locator("#zoom").textContent();
  await page.evaluate(() => document.getElementById("reader").style.width = "460px");
  await frame.locator("#viewport").evaluate(element => new Promise(resolve => { const observer = new ResizeObserver(() => { observer.disconnect(); resolve(); }); observer.observe(element); }));
  assert.equal(await frame.locator("#zoom").textContent(), manualZoom, "Host resize preserves manual zoom");
  await frame.getByRole("button", { name: "Fit width", exact: true }).click();
  const fit = await frame.locator("section.docx").first().evaluate(element => ({ page: element.getBoundingClientRect().width, viewport: document.getElementById("viewport").clientWidth }));
  assert.ok(fit.page <= fit.viewport, "fit width must fit the document page in a narrow reader");
  const chromePadding = await frame.locator("#stage section.docx header, #stage section.docx footer").evaluateAll(elements => elements.map(element => getComputedStyle(element).padding));
  assert.ok(chromePadding.every(padding => !["8px 16px", "12px 16px"].includes(padding)), "reader chrome spacing must not affect document headers or footers");
  await frame.locator("section.docx").nth(1).evaluate(element => { element.style.width = "1200px"; });
  await frame.getByRole("button", { name: "Fit width", exact: true }).click();
  const mixedFit = await frame.locator("section.docx").nth(1).evaluate(element => ({ page: element.getBoundingClientRect().width, viewport: document.getElementById("viewport").clientWidth }));
  assert.ok(mixedFit.page <= mixedFit.viewport, "fit width must use the widest page in a mixed-orientation document");
  await frame.locator("section.docx").nth(1).evaluate(element => { element.style.width = ""; });
  await frame.locator("#download").click();
  await page.waitForFunction(() => fixture.downloads.length === 1);
  const downloaded = await page.evaluate(() => fixture.downloads[0]);
  assert.equal(downloaded.uri, "file:///sample.docx");
  assert.deepEqual(Buffer.from(downloaded.blob, "base64"), bytes, "Host download retains verified original bytes");
  await page.evaluate(() => fixture.setDownloadDenied(true));
  await frame.locator("#download").click();
  await frame.getByText("Download unavailable. You can request a file export in the chat.", { exact: true }).waitFor({state:"visible"});
  assert.equal(await frame.locator("section.docx").count(), 2, "Download denial preserves the document");
  await page.evaluate(() => fixture.setDownloadDenied(false));
  await frame.getByRole("button", { name: "Expand", exact: true }).click();
  await page.waitForFunction(() => document.getElementById("reader").style.width === "1000px");
  await frame.getByRole("button", { name: "Fit width", exact: true }).click();
  const wideFit = await frame.locator("#viewport").evaluate(element => ({ width: element.clientWidth, scroll: element.scrollWidth }));
  assert.ok(wideFit.scroll <= wideFit.width, "Fit width includes scaled stage padding in a wide reader");
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
  await page.evaluate(() => document.getElementById("reader").contentWindow.postMessage({jsonrpc:'2.0',method:'ui/notifications/host-context-changed',params:{displayMode:'inline',availableDisplayModes:['inline']}},'*'));
  await frame.locator("#fullscreen").waitFor({state:"hidden"});
  const old = await page.evaluate(() => fixture.count);
  await page.evaluate(() => { const reader=document.getElementById("reader");reader.contentWindow.postMessage({jsonrpc:'2.0',id:900,method:'ui/resource-teardown',params:{}},'*'); });
  await frame.locator("#download").waitFor({ state: "hidden" });
  await page.evaluate(() => select()); assert.equal(await page.evaluate(() => fixture.count), old);
  await page.goto(`http://127.0.0.1:${server.address().port}/?limited`);
  await frame.locator("section.docx").first().waitFor({state:"visible"});
  await frame.locator("#download").waitFor({state:"hidden"});
  await frame.locator("#fullscreen").waitFor({state:"hidden"});
  assert.deepEqual(await page.evaluate(() => fixture.downloads), [], "Unsupported Hosts receive no download requests");
  assert.deepEqual(errors, []); assert.deepEqual(external, []);
  console.log("DOCX browser smoke passed: private/text fallback, rendering, responsive/manual zoom, capability-gated Host download/fullscreen, stale version, superseded read, teardown; no external resources.");
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
