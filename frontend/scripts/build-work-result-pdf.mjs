import { buildPdfBundle } from "./pdf-bundle.mjs";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const path = resolve(root, "../src/mcp_work_result_app.html");
const begin = "/* BEGIN GENERATED WORK RESULT PDF */", end = "/* END GENERATED WORK RESULT PDF */";
if (process.argv.slice(2).some(arg => arg !== "--check")) throw new Error("Only --check is supported");
const { script: builtScript, assetCount } = await buildPdfBundle("src/mcp-apps/work-result/pdf-preview.mjs", "WebCodexPdf");
const html = await readFile(path, "utf8");
if (html.split(begin).length !== 2 || html.split(end).length !== 2) throw new Error("Expected exactly one generated PDF region");
const start = html.indexOf(begin) + begin.length, finish = html.indexOf(end);
if (finish <= start) throw new Error("Invalid PDF generated region");
const script = builtScript
  .replace(/<\/script/gi, "<\\/script").replace(/[\t ]+$/gm, "");
if (script.includes(begin) || script.includes(end)) throw new Error("Reserved PDF marker in bundle");
const output = html.slice(0, start) + "\n" + script + html.slice(finish);
if (process.argv.includes("--check")) {
  if (output !== html) throw new Error("Work Result PDF bundle out of date; run npm --prefix frontend run build:work-result");
  console.log("Work Result PDF bundle checked");
} else {
  await writeFile(path, output);
  console.log(`Work Result PDF bundle generated · ${Buffer.byteLength(script)} bytes · ${assetCount} offline assets`);
}
