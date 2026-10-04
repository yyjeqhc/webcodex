import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { startPdfDocumentPreview } from "./pdf-document-preview.mjs";

const artifacts = new URL("artifacts/pdf-document/", import.meta.url); await mkdir(artifacts, { recursive: true });
const preview = await startPdfDocumentPreview();
const browser = await chromium.launch({ channel: "msedge", headless: true });
const report = { browser: await browser.version(), actualChatGPT: false, shippedPdfApp: true, cases: [] };
try {
  for (const [sample, width, theme, fallback = false] of [["cjk", 860, "light"], ["cjk", 420, "dark"], ["text", 360, "light", true], ["scan", 420, "dark"], ["ratio", 860, "light"]]) {
    const context = await browser.newContext({ viewport: { width, height: 900 } });
    const page = await context.newPage(), workers = new Set(), errors = [], outbound = [];
    page.on("worker", worker => { workers.add(worker); worker.on("close", () => workers.delete(worker)); });
    page.on("pageerror", error => errors.push(error.message));
    page.on("request", request => { if (/^https?:/.test(request.url()) && !request.url().startsWith(preview.url)) outbound.push(request.url()); });
    await page.goto(`${preview.url}/?sample=${sample}&theme=${theme}&fallback=${fallback ? 1 : 0}`);
    const frame = page.frameLocator("iframe");
    await frame.locator(".pdf-status").filter({ hasText: /^PDF · \d+ pages? ·/ }).waitFor({ timeout: 25000 });
    const app = page.frames().find(frame => frame.url().includes("/viewer"));
    const metrics = await app.evaluate(() => {
      const canvas = document.querySelector("canvas"), stage = document.querySelector(".pdf-stage"), layer = document.querySelector(".textLayer");
      const pixels = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
      let ink = 0; for (let i = 0; i < pixels.length; i += 4) if (pixels[i] < 225 || pixels[i + 1] < 225 || pixels[i + 2] < 225) ink++;
      const rect = canvas.getBoundingClientRect();
      return { ink, width: rect.width, height: rect.height, backing: canvas.width * canvas.height, text: layer.textContent,
        horizontalOverflow: document.documentElement.scrollWidth > innerWidth,
        outerScroll: document.documentElement.scrollHeight > innerHeight + 1,
        stageHeight: stage.clientHeight, tabs: document.querySelectorAll('[role="tab"]').length };
    });
    assert.ok(metrics.ink > 1000); assert.ok(metrics.backing <= 8_000_000); assert.equal(metrics.horizontalOverflow, false);
    assert.equal(metrics.outerScroll, false); assert.equal(metrics.tabs, 0); assert.ok(metrics.stageHeight > 600);
    assert.deepEqual(errors, []); assert.deepEqual(outbound, []); assert.equal(workers.size, 1);
    if (sample === "cjk") assert.ok(metrics.text.includes("中文 PDF"));
    if (sample === "scan") assert.equal(metrics.text.trim(), "");
    if (sample === "ratio") assert.ok(Math.abs(metrics.width / metrics.height - 630 / 496) < 0.001);
    if (sample === "text") {
      await frame.getByLabel("Find text on current PDF page").fill("offline-preview");
      await frame.getByRole("button", { name: "Find", exact: true }).click();
      assert.ok((await app.evaluate(() => getSelection().toString())).includes("offline-preview"));
      await frame.getByRole("button", { name: "Next", exact: true }).click();
      await frame.locator(".pdf-toolbar span").filter({ hasText: "2 / 3" }).waitFor();
      await frame.getByRole("button", { name: "Previous", exact: true }).click();
      await frame.locator(".pdf-toolbar span").filter({ hasText: "1 / 3" }).waitFor();
      await frame.getByLabel("Zoom in").click();
      await frame.getByRole("button", { name: "Fit page width", exact: true }).click();
    }
    await page.screenshot({ path: new URL(`${sample}-${width}-${theme}.png`, artifacts).pathname.replace(/^\/(\w:)/, "$1") });
    // Resource teardown must cancel and release the document's Worker.
    await page.evaluate(() => pdfDocumentTestBridge.teardownResource({}));
    const deadline = Date.now() + 3000;
    while (workers.size && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
    assert.equal(workers.size, 0);
    report.cases.push({ sample, viewportWidth: width, viewportHeight: 900, theme, fallback, ...metrics, text: metrics.text.slice(0, 100), workersAfterClose: workers.size });
    await context.close();
  }
  for (const query of ["policy=deny", "stale=1"]) {
    const page = await browser.newPage({ viewport: { width: 420, height: 900 } });
    await page.goto(`${preview.url}/?${query}`);
    const frame = page.frameLocator("iframe");
    await frame.locator(".pdf-status").filter({ hasText: "PDF unavailable" }).waitFor({ timeout: 10000 });
    assert.equal(await frame.getByRole("button", { name: "Retry preview", exact: true }).isVisible(), true);
    if (query === "stale=1") assert.ok((await frame.locator(".pdf-status").innerText()).includes("reopen"));
    report[query] = "visible failure without retargeting";
    await page.close();
  }
} catch (error) {
  report.failure = error.message;
  for (const context of browser.contexts()) for (const page of context.pages()) {
    const frame = page.frames().find(frame => frame.url().includes("/viewer"));
    if (frame) report.failureState = await frame.locator("body").innerText();
  }
  throw error;
} finally {
  await writeFile(new URL("report.json", artifacts), JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report)); await browser.close(); await preview.close();
}
