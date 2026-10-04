import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { startWorkResultPreview } from "./work-result-preview.mjs";

const artifacts = new URL("artifacts/work-result/", import.meta.url); await mkdir(artifacts, { recursive: true });
const preview = await startWorkResultPreview();
const browser = await chromium.launch({ channel: "msedge", headless: true });
const report = { browser: await browser.version(), actualChatGPT: false, shippedWorkResult: true, cases: [] };
try {
  for (const [sample, width, theme] of [["text", 360, "light"], ["cjk", 420, "light"], ["scan", 420, "dark"], ["ratio", 860, "light"]]) {
    const height = sample === "ratio" ? 1100 : 900;
    const context = await browser.newContext({ viewport: { width, height } });
    const page = await context.newPage(), workers = new Set(), errors = [], outbound = [];
    page.on("worker", worker => { workers.add(worker); worker.on("close", () => workers.delete(worker)); });
    page.on("pageerror", error => errors.push(error.message));
    page.on("request", request => { if (/^https?:/.test(request.url()) && !request.url().startsWith(preview.url)) outbound.push(request.url()); });
    await page.goto(`${preview.url}/?sample=${sample}&theme=${theme}`);
    const frame = page.frameLocator("iframe");
    await frame.getByRole("button", { name: `${sample}.pdf`, exact: false }).click();
    await frame.locator(".pdf-status").filter({ hasText: "Working-tree snapshot PDF" }).waitFor({ timeout: 25000 });
    const app = page.frames().find(frame => frame.url().includes("/viewer"));
    const metrics = await frame.locator(".pdf-page canvas").evaluate(canvas => {
      const pixels = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
      let ink = 0; for (let i = 0; i < pixels.length; i += 4) if (pixels[i] < 225 || pixels[i + 1] < 225 || pixels[i + 2] < 225) ink++;
      const rect = canvas.getBoundingClientRect();
      const row = canvas.closest(".frozen-file"), navigator = document.querySelector("#workspaceNavigator");
      return { ink, width: rect.width, height: rect.height, backing: canvas.width * canvas.height,
        titleVisible: row.querySelector(".file-toggle").getBoundingClientRect().top >= navigator.getBoundingClientRect().bottom,
        overflow: document.documentElement.scrollWidth > innerWidth, text: document.querySelector(".textLayer").textContent };
    });
    assert.ok(metrics.ink > 1000); assert.ok(metrics.backing <= 8_000_000); assert.equal(metrics.overflow, false);
    assert.equal(metrics.titleVisible, true, "sticky navigation must not cover the PDF filename");
    assert.equal(workers.size, 1); assert.deepEqual(outbound, []); assert.deepEqual(errors, []);
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
    await frame.getByRole("button", { name: `${sample}.pdf`, exact: false }).click();
    await page.waitForFunction(() => !document.querySelector("iframe").contentDocument.querySelector(".pdf-page canvas"));
    const deadline = Date.now() + 3000;
    while (workers.size && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
    assert.equal(workers.size, 0, "collapse releases the PDF Worker");
    report.cases.push({ sample, viewportWidth: width, viewportHeight: height, theme, ...metrics, text: metrics.text.slice(0, 100), workersAfterClose: workers.size });
    await context.close();
  }
  // Worker denial is actionable, preserves the snapshot and offers Retry.
  const page = await browser.newPage({ viewport: { width: 420, height: 900 } });
  await page.goto(`${preview.url}/?policy=deny`);
  const frame = page.frameLocator("iframe"); await frame.getByRole("button", { name: "text.pdf", exact: false }).click();
  await frame.locator(".pdf-status").filter({ hasText: "PDF unavailable" }).waitFor({ timeout: 10000 });
  assert.equal(await frame.getByRole("button", { name: "Retry preview", exact: true }).isVisible(), true);
  report.workerDenied = "visible error and retry";
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
