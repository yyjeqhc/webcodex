// Real browser regressions for the shipped Apps; all inputs are synthetic.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { deflateSync } from "node:zlib";
import { createHash } from "node:crypto";
import { chromium } from "playwright";
import { startPdfDocumentPreview } from "./pdf-document-preview.mjs";
import { startWorkResultPreview } from "./work-result-preview.mjs";

const artifacts = new URL("artifacts/pdf-regressions/", import.meta.url);
await mkdir(artifacts, { recursive: true });

function imagePdf(width, height) {
  const image = deflateSync(Buffer.alloc(width * height), { level: 9 });
  const content = Buffer.from("q 400 0 0 500 0 0 cm /Im0 Do Q\n");
  const objects = [
    Buffer.from("<< /Type /Catalog /Pages 2 0 R >>"),
    Buffer.from("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
    Buffer.from("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 500] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"),
    Buffer.concat([Buffer.from(`<< /Type /XObject /Subtype /Image /Width ${width} /Height ${height} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length ${image.length} >>\nstream\n`), image, Buffer.from("\nendstream")]),
    Buffer.concat([Buffer.from(`<< /Length ${content.length} >>\nstream\n`), content, Buffer.from("endstream")]),
  ];
  const parts = [Buffer.from("%PDF-1.7\n")], offsets = [];
  let size = parts[0].length;
  for (const [index, object] of objects.entries()) {
    offsets.push(size);
    const part = Buffer.concat([Buffer.from(`${index + 1} 0 obj\n`), object, Buffer.from("\nendobj\n")]);
    parts.push(part); size += part.length;
  }
  parts.push(Buffer.from(`xref\n0 6\n0000000000 65535 f \n${offsets.map(offset => String(offset).padStart(10, "0") + " 00000 n \n").join("")}trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${size}\n%%EOF\n`));
  return Buffer.concat(parts);
}

async function useImage(page, app, data) {
  if (app === "document") await page.route("**/present?*", async route => {
    const result = await (await route.fetch()).json();
    const identity = result.structuredContent.output.pdf_document;
    identity.bytes = data.length;
    identity.sha256 = createHash("sha256").update(data).digest("hex");
    result._meta["webcodex/pdfDocument"] = result.structuredContent;
    await route.fulfill({ json: result });
  });
  await page.route("**/tool*", async route => {
    const { name, arguments: args } = route.request().postDataJSON();
    if (name !== "read_pdf_chunk" && args.files?.view !== "pdf") return route.continue();
    const offset = app === "document" ? args.byte_offset : args.files.byte_offset;
    const next = Math.min(offset + 128 * 1024, data.length);
    const chunk = { byte_offset: offset, bytes_total: data.length, complete: next === data.length, next_byte_offset: next === data.length ? null : next };
    const output = app === "document"
      ? { pdf_chunk: { ...chunk, project: args.project, path: args.path, sha256: args.sha256 } }
      : { work_result_files: { ...chunk, project: args.project, session_id: args.session_id ?? null, snapshot_id: args.files.snapshot_id, path: args.files.path, view: "pdf" } };
    const structuredContent = { success: true, output };
    await route.fulfill({ json: { structuredContent, content: [{ type: "text", text: JSON.stringify(structuredContent) }],
      _meta: { "webcodex/pdfChunk": { content_base64: data.subarray(offset, next).toString("base64") } } } });
  });
}

async function metrics(frame) {
  return frame.evaluate(() => {
    const canvas = document.querySelector(".pdf-page canvas"), row = canvas.closest(".frozen-file");
    const pixels = canvas.width && canvas.height ? canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data : [];
    let ink = 0;
    for (let i = 0; i < pixels.length; i += 4) if (pixels[i] < 225 || pixels[i + 1] < 225 || pixels[i + 2] < 225) ink++;
    const retry = row ? [...row.querySelectorAll("button")].find(button => button.textContent === "Retry preview") : document.querySelector("#retry");
    return { status: document.querySelector(".pdf-status").textContent, ink, rowHidden: row?.hidden ?? null,
      pageHidden: canvas.parentElement.hidden, canvasWidth: canvas.width, canvasHeight: canvas.height, retryHidden: retry.hidden };
  });
}

const work = await startWorkResultPreview(), document = await startPdfDocumentPreview();
let browser;
const report = { actualChatGPT: false, shippedBundles: true, imageCases: [] };
try {
  browser = await chromium.launch({ channel: "msedge", headless: true });
  report.browser = browser.version();
  for (const [width, height] of [[4000, 3900], [5000, 3500]]) {
    const data = imagePdf(width, height), overLimit = width * height > 16_000_000;
    assert.ok(data.length < 20_000, "compressed image fixture must stay far below the file limit");
    for (const app of ["document", "work-result"]) {
      const context = await browser.newContext({ viewport: { width: 860, height: 900 } }), page = await context.newPage();
      const workers = new Set();
      page.on("worker", worker => { workers.add(worker); worker.on("close", () => workers.delete(worker)); });
      await useImage(page, app, data);
      await page.goto(`${app === "document" ? document.url : work.url}/?sample=scan&theme=light`);
      const frame = page.frameLocator("iframe");
      if (app === "work-result") await frame.getByRole("button", { name: "scan.pdf", exact: false }).click();
      await frame.locator(".pdf-status").filter({ hasText: overLimit ? "a complete preview is unavailable" : / · 1 page ·/ }).waitFor({ timeout: 25000 });
      const viewer = page.frames().find(frame => frame.url().includes("/viewer")), result = await metrics(viewer);
      if (overLimit) {
        assert.match(result.status, /^PDF unavailable:/); assert.equal(result.retryHidden, false);
        assert.equal(result.pageHidden, true); assert.equal(result.canvasWidth, 0);
        const deadline = Date.now() + 3000;
        while (workers.size && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
        assert.equal(workers.size, 0, "failed preview releases its Worker");
      } else { assert.ok(result.ink > 1000); assert.equal(result.retryHidden, true); }
      await page.screenshot({ path: new URL(`image-${app}-${width}x${height}.png`, artifacts).pathname.replace(/^\/(\w:)/, "$1") });
      report.imageCases.push({ app, width, height, pdfBytes: data.length, ...result });
      await context.close();
    }
  }

  const context = await browser.newContext({ viewport: { width: 860, height: 900 } }), page = await context.newPage();
  let release, arrived, fulfilled;
  const hold = new Promise(resolve => { release = resolve; });
  const arrival = new Promise(resolve => { arrived = resolve; });
  const responseDone = new Promise(resolve => { fulfilled = resolve; });
  const reads = [];
  await page.route("**/tool", async route => {
    const call = route.request().postDataJSON();
    if (call.arguments.files?.view !== "pdf") return route.continue();
    reads.push(call.arguments); const response = await route.fetch(); arrived(); await hold;
    await route.fulfill({ response }); fulfilled();
  });
  await page.goto(`${work.url}/?sample=text&theme=light`);
  const frame = page.frameLocator("iframe");
  await frame.getByRole("button", { name: "text.pdf", exact: false }).click();
  // The HTTP request has started; only its response is delayed.
  let startTimer;
  try {
    await Promise.race([arrival, new Promise((_, reject) => { startTimer = setTimeout(() => reject(new Error("PDF request did not start")), 10000); })]);
  } finally { clearTimeout(startTimer); }
  const viewer = page.frames().find(frame => frame.url().includes("/viewer"));
  const filter = frame.getByLabel("Filter changed files by path");
  await filter.fill("no-match");
  await viewer.waitForFunction(() => document.querySelector(".pdf-page canvas").closest(".frozen-file").hidden);
  const hiddenScroll = await viewer.evaluate(() => scrollY);
  release(); await responseDone;
  await viewer.waitForFunction(() => document.querySelector(".pdf-status").textContent.includes("Working-tree snapshot PDF"));
  assert.equal(await viewer.evaluate(() => scrollY), hiddenScroll, "hidden completion must not focus the row");
  await filter.fill("");
  await viewer.waitForFunction(() => {
    const canvas = document.querySelector(".pdf-page canvas");
    return !canvas.closest(".frozen-file").hidden && !canvas.parentElement.hidden && canvas.width > 500
      && !document.querySelector('[aria-label="Fit page width"]').disabled;
  });
  const result = await metrics(viewer);
  assert.ok(result.ink > 1000); assert.equal(result.retryHidden, true); assert.equal(reads.length, 1);
  const row = frame.locator(".frozen-file").filter({ has: frame.getByRole("button", { name: "text.pdf", exact: false }) });
  const duplicate = page.waitForRequest(request => request.method() === "POST" && request.postDataJSON()?.arguments?.files?.view === "pdf", { timeout: 600 }).then(() => true, () => false);
  await row.getByRole("button", { name: "PDF", exact: true }).click();
  assert.equal(await duplicate, false, "restoring the same row must retain its selected snapshot and renderer");
  report.filterCase = { ...result, reads, retainedRenderer: true };
  await page.screenshot({ path: new URL("filter-restored.png", artifacts).pathname.replace(/^\/(\w:)/, "$1") });
  await context.close();
} catch (error) {
  report.failure = error.stack;
  for (const context of browser?.contexts() || []) for (const page of context.pages()) {
    const viewer = page.frames().find(frame => frame.url().includes("/viewer"));
    if (viewer && await viewer.locator(".pdf-page canvas").count()) report.failureState = await metrics(viewer);
  }
  throw error;
}
finally {
  await writeFile(new URL("report.json", artifacts), JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report)); await browser?.close(); await Promise.all([work.close(), document.close()]);
}
