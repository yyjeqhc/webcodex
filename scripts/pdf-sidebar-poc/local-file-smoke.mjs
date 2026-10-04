import assert from "node:assert/strict";
import { chromium } from "playwright";
import { fileURLToPath } from "node:url";
import { startPreview } from "./preview-server.mjs";

const preview = await startPreview();
const browser = await chromium.launch({ channel: process.env.PDF_POC_BROWSER || "msedge", headless: true });
try {
  const page = await browser.newPage();
  let chunkReads = 0;
  page.on("request", request => { if (request.url().endsWith("/tool") && request.postData()?.includes("read_pdf_sample_bytes")) chunkReads++; });
  await page.goto(preview.url);
  const frame = page.frameLocator("#viewer");
  await frame.locator("#page-info").filter({ hasText: "1 / 3" }).waitFor();
  const appFrame = page.frames().find(frame => frame.url().includes("/viewer?"));
  const renders = await appFrame.evaluate(() => {
    globalThis.closeDisabledDuringResize = false;
    const close = document.getElementById("close");
    new MutationObserver(() => { if (close.disabled) closeDisabledDuringResize = true; }).observe(close, { attributes: true, attributeFilter: ["disabled"] });
    return pdfPocDiagnostics().renders;
  });
  await page.getByRole("button", { name: "切换主题" }).click();
  await appFrame.waitForFunction(count => pdfPocDiagnostics().renders > count, renders);
  assert.equal(await appFrame.evaluate(() => closeDisabledDuringResize), false, "Background reflow must keep the close button interactive");
  const before = chunkReads;
  await frame.locator("#file").setInputFiles(fileURLToPath(new URL("fixtures/scan.pdf", import.meta.url)));
  await appFrame.waitForFunction(() => pdfPocDiagnostics().document === "scan.pdf" && pdfPocDiagnostics().pages === 1);
  assert.equal(chunkReads, before, "Local file must not be uploaded or read through MCP");
  await frame.locator("#file").setInputFiles({ name: "invalid.pdf", mimeType: "application/pdf", buffer: Buffer.from("this is not a PDF") });
  await frame.locator("#status").filter({ hasText: "无法预览" }).waitFor();
  assert.equal((await appFrame.evaluate(() => pdfPocDiagnostics())).activeWorkers, 0);
  await frame.locator("#file").setInputFiles({ name: "too-large.pdf", mimeType: "application/pdf", buffer: Buffer.alloc(20 * 1024 * 1024 + 1) });
  await frame.locator("#status").filter({ hasText: "文件超过 20 MiB" }).waitFor();
  assert.equal(chunkReads, before);
  console.log("PASS local file stays local; invalid PDF releases worker; oversized file rejected before loading");
} finally { await browser.close(); await preview.close(); }
