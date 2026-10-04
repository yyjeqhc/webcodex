import test from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import JSZip from "jszip";
import { docxFixture } from "./fixtures/docx.mjs";
import { inspectDocxZip, prepareDocx, renderDocx, sanitizeDocx } from "../src/mcp-apps/docx-preview.mjs";
function dom() {
  const instance = new JSDOM('<!doctype html><html><head></head><body><div class="docx-stage"></div></body></html>', { url: "https://viewer.example" });
  for (const name of ["window", "document", "DOMParser", "Node", "Element", "HTMLElement", "Blob", "FileReader"]) globalThis[name] = instance.window[name];
  return instance;
}
test("DOCX renders Chinese text, tables, image, header/footer and explicit page breaks", async () => {
  const instance = dom(), stage = document.querySelector(".docx-stage");
  const pages = await renderDocx(await docxFixture(), stage);
  assert.equal(pages, 2); assert.match(stage.textContent, /DOCX 阅读器验证/); assert.match(stage.textContent, /第二页/);
  assert.match(stage.textContent, /WebCodex 文档样本/); assert.match(stage.textContent, /只读预览/);
  assert.equal(stage.querySelectorAll("table td").length, 4); assert.match(stage.querySelector("img").src, /^data:image\/png;/);
  assert.ok(stage.querySelector("style").textContent.includes(".docx-stage")); instance.window.close();
});
test("DOCX strips external relationships and never renders altChunk HTML", async () => {
  const instance = dom(), bytes = await docxFixture({ unsafe: true });
  const prepared = await prepareDocx(bytes, window), zip = await JSZip.loadAsync(prepared);
  assert.ok(!(await zip.file("word/_rels/document.xml.rels").async("string")).includes('TargetMode="External"'));
  const stage = document.querySelector(".docx-stage"); await renderDocx(bytes, stage);
  assert.equal(stage.querySelectorAll("iframe,script,object,embed,a[href]").length, 0); assert.equal(window.injected, undefined); instance.window.close();
});
test("DOCX ZIP preflight rejects malformed files, traversal and excessive expansion", async () => {
  const bytes = await docxFixture(); assert.ok(inspectDocxZip(bytes).expanded > 0);
  const changed = bytes.slice(), view = new DataView(changed.buffer);
  const index = changed.findIndex((value, i) => value === 80 && changed[i + 1] === 75 && changed[i + 2] === 1 && changed[i + 3] === 2);
  view.setUint32(index + 24, 33 * 1024 * 1024, true); assert.throws(() => inspectDocxZip(changed), /Unsupported/);
  assert.throws(() => inspectDocxZip(bytes.subarray(0, bytes.length - 1)), /directory/);
  const zip = await JSZip.loadAsync(bytes); zip.file("../escape.xml", "bad");
  const unsafe = await zip.generateAsync({ type: "uint8array" });
  assert.throws(() => inspectDocxZip(unsafe), /Unsafe/);
});
test("DOCX XML complexity and declarations are rejected before rendering", async () => {
  const instance = dom(); await assert.rejects(prepareDocx(await docxFixture({ manyNodes: true }), window), /complex/);
  const zip = await JSZip.loadAsync(await docxFixture()); zip.file("word/document.xml", '<!DOCTYPE x [<!ENTITY y "z">]><x/>');
  await assert.rejects(prepareDocx(await zip.generateAsync({ type: "uint8array" }), window), /declaration/); instance.window.close();
});
test("Rendered DOCX sanitization strips active content and unsafe CSS", () => {
  const instance = dom(), stage = document.querySelector(".docx-stage"), styles = document.createElement("div");
  stage.innerHTML = '<p style="position:fixed;z-index:999;color:red">safe</p><a href="javascript:alert(1)">link</a><iframe srcdoc="bad"></iframe><img src="https://example.invalid/a">';
  styles.innerHTML = '<style>body {color:blue}</style><style>@import "https://example.invalid";</style>';
  sanitizeDocx(stage, styles, window); assert.equal(stage.querySelectorAll("iframe,a[href],img[src]").length, 0);
  assert.equal(stage.querySelector("p").style.position, ""); assert.equal(stage.querySelector("p").style.color, "red");
  assert.ok(stage.querySelector("style").textContent.startsWith(".docx-stage body")); instance.window.close();
});

test("Actual ZIP expansion is bounded even when declared sizes are dishonest", async () => {
  const instance = dom(), zip = await JSZip.loadAsync(await docxFixture());
  zip.file("word/media/oversized.bin", new Uint8Array(8 * 1024 * 1024 + 1));
  const bytes = await zip.generateAsync({ type: "uint8array", compression: "DEFLATE" }), view = new DataView(bytes.buffer);
  for (let offset = 0; offset + 46 < bytes.length; offset++) {
    if (view.getUint32(offset, true) !== 0x02014b50) continue;
    const name = new TextDecoder().decode(bytes.subarray(offset + 46, offset + 46 + view.getUint16(offset + 28, true)));
    if (name === "word/media/oversized.bin") { view.setUint32(offset + 24, 1, true); view.setUint32(view.getUint32(offset + 42, true) + 22, 1, true); break; }
  }
  await assert.rejects(prepareDocx(bytes, window), /expansion limit/); instance.window.close();
});
