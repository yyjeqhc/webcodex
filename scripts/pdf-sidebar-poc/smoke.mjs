import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { startPreview } from "./preview-server.mjs";

const artifacts = new URL("artifacts/", import.meta.url);
await mkdir(artifacts, { recursive: true });
const preview = await startPreview();
const channel = process.env.PDF_POC_BROWSER || "msedge";
const browser = await chromium.launch({ channel, headless: true });
const report = { browser: await browser.version(), channel, actualChatGPT: false, cases: [] };
let liveCase;
try {
  for (const policy of ["allow", "deny"]) for (const sample of ["text", "cjk", "scan"]) {
    if (process.env.PDF_POC_CASE && process.env.PDF_POC_CASE !== `${policy}-${sample}`) continue;
    const context = await browser.newContext({ viewport: { width: 1100, height: 1050 } });
    const page = await context.newPage();
    const errors = [], outbound = [], requests = [], actualWorkers = new Set();
    liveCase = { policy, sample, page, errors };
    let maxWorkers = 0;
    page.on("worker", worker => { actualWorkers.add(worker); maxWorkers = Math.max(maxWorkers, actualWorkers.size); worker.on("close", () => actualWorkers.delete(worker)); });
    page.on("pageerror", error => errors.push(error.message));
    context.on("request", request => {
      requests.push(request.url());
      if (/^https?:/.test(request.url()) && !request.url().startsWith(preview.url)) outbound.push(request.url());
    });
    await page.goto(`${preview.url}/?policy=${policy}&sample=${sample}`);
    const frame = page.frameLocator("#viewer");
    try {
      await frame.locator("#page-info").filter({ hasText: sample === "text" ? "1 / 3" : "1 / 1" }).waitFor({ timeout: 20000 });
    } catch (error) {
      const failedFrame = page.frames().find(frame => frame.url().includes("/viewer?"));
      report.failure = { policy, sample, errors, hostStatus: await page.locator("#host-status").innerText(),
        viewerStatus: failedFrame ? await failedFrame.locator("#status").innerText() : "missing frame" };
      console.log(report.failure); throw error;
    }
    const appFrame = page.frames().find(frame => frame.url().includes("/viewer?"));
    await appFrame.waitForFunction(() => pdfPocDiagnostics().page === 1 && pdfPocDiagnostics().renders > 0);
    let diag = await appFrame.evaluate(() => pdfPocDiagnostics());
    assert.equal(diag.engine, policy === "allow" ? "module-worker" : "main-thread-fallback");
    assert.equal(actualWorkers.size, policy === "allow" ? 1 : 0);
    const pixels = await frame.locator("#page canvas").evaluate(canvas => {
      const image = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
      let nonWhite = 0;
      for (let i = 0; i < image.length; i += 4) if (image[i] < 225 || image[i + 1] < 225 || image[i + 2] < 225) nonWhite++;
      return nonWhite;
    });
    assert.ok(pixels > 1000, "Canvas must contain visible PDF content");
    if (sample === "scan") assert.equal(diag.text.trim(), "");
    if (sample === "cjk") {
      assert.ok(diag.text.includes("中文 PDF"));
      assert.ok(diag.assetReads.some(asset => asset.startsWith("cMapUrl/")));
    }
    if (sample === "text") {
      await frame.getByLabel("搜索本页文字", { exact: true }).fill("offline-preview");
      await frame.getByRole("button", { name: "查找", exact: true }).click();
      assert.ok((await appFrame.evaluate(() => getSelection().toString())).includes("offline-preview"));
      await frame.getByRole("button", { name: "下一页", exact: true }).click();
      await appFrame.waitForFunction(() => pdfPocDiagnostics().page === 2);
      await frame.getByRole("button", { name: "上一页", exact: true }).click();
      await appFrame.waitForFunction(() => pdfPocDiagnostics().page === 1);
    }
    const before = diag.scale;
    await page.getByRole("button", { name: "宽面板" }).click();
    await appFrame.waitForFunction(scale => pdfPocDiagnostics().scale > scale, before);
    const rendered = (await appFrame.evaluate(() => pdfPocDiagnostics())).renders;
    await frame.getByRole("button", { name: "放大", exact: true }).click();
    await appFrame.waitForFunction(renders => pdfPocDiagnostics().renders > renders, rendered);
    await frame.locator("#fit").click();
    await frame.getByRole("button", { name: "展开", exact: true }).click();
    await page.locator("#panel.fullscreen").waitFor();
    await frame.getByRole("button", { name: "收起", exact: true }).waitFor();
    await page.getByRole("button", { name: "切换主题" }).click();
    await appFrame.waitForFunction(() => document.documentElement.dataset.theme === "dark" || document.documentElement.style.colorScheme === "dark");
    await frame.getByRole("button", { name: "收起", exact: true }).waitFor();
    await page.screenshot({ path: new URL(`${channel}-${policy}-${sample}.png`, artifacts).pathname.replace(/^\/(\w:)/, "$1"), fullPage: true });
    diag = await appFrame.evaluate(() => pdfPocDiagnostics());
    await frame.getByRole("button", { name: "收起", exact: true }).click();
    await page.waitForFunction(() => !document.getElementById("panel").classList.contains("fullscreen"));
    assert.deepEqual(errors, []);
    assert.deepEqual(outbound, []);
    assert.ok(!requests.some(url => /\.(bcmap|ttf|pfb|wasm)(\?|$)/.test(url)), "Assets must not trigger runtime fetches");
    const closed = Promise.all([...actualWorkers].map(worker => worker.waitForEvent("close")));
    await frame.getByRole("button", { name: "关闭文档" }).click();
    await appFrame.waitForFunction(() => pdfPocDiagnostics().pages === 0 && pdfPocDiagnostics().activeWorkers === 0);
    assert.equal((await appFrame.evaluate(() => pdfPocDiagnostics())).activeBlobUrls, 0);
    await closed; assert.equal(actualWorkers.size, 0);
    // Reload through the Host, then exercise the SDK teardown callback too.
    await page.evaluate(() => pdfPocHost.showSample());
    await appFrame.waitForFunction(() => pdfPocDiagnostics().pages > 0);
    const tornDown = Promise.all([...actualWorkers].map(worker => worker.waitForEvent("close")));
    await page.getByRole("button", { name: "卸载组件" }).click();
    await appFrame.waitForFunction(() => pdfPocDiagnostics().pages === 0 && pdfPocDiagnostics().activeBlobUrls === 0);
    await tornDown; assert.equal(actualWorkers.size, 0);
    report.cases.push({ policy, sample, pixels, outboundRequests: outbound.length, errors, diagnostics: diag, maxActualWorkers: maxWorkers, cleanupPassed: true });
    console.log(`PASS ${channel} ${policy} ${sample}: ${diag.engine}, ${diag.renderMs} ms`);
    await context.close();
  }
} catch (error) {
  if (liveCase) {
    const frame = liveCase.page.frames().find(frame => frame.url().includes("/viewer?"));
    report.failure = { policy: liveCase.policy, sample: liveCase.sample, error: error.message, errors: liveCase.errors,
      diagnostics: frame ? await frame.evaluate(() => globalThis.pdfPocDiagnostics?.()) : null,
      status: frame ? await frame.locator("#status").innerText() : "missing frame" };
    console.log(JSON.stringify(report.failure));
  }
  throw error;
} finally {
  await writeFile(new URL(`report-${channel}.json`, artifacts), JSON.stringify(report, null, 2) + "\n");
  await browser.close(); await preview.close();
}
