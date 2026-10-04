import { build } from "esbuild";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pdfRoot = resolve(root, "node_modules/pdfjs-dist");
const path = resolve(root, "../src/mcp_work_result_app.html");
const begin = "/* BEGIN GENERATED WORK RESULT PDF */", end = "/* END GENERATED WORK RESULT PDF */";
if (process.argv.slice(2).some(arg => arg !== "--check")) throw new Error("Only --check is supported");
const settings = { bundle: true, platform: "browser", target: "es2022", minify: true,
  write: false, legalComments: "inline", supported: { "inline-script": true } };
const worker = await build({ ...settings, entryPoints: [resolve(pdfRoot, "build/pdf.worker.mjs")], format: "esm" });
const assets = {}, notices = [`pdfjs-dist ${JSON.parse(await readFile(resolve(pdfRoot, "package.json"), "utf8")).version}\n${await readFile(resolve(pdfRoot, "LICENSE"), "utf8")}`];
for (const [kind, folder] of [["cMapUrl", "cmaps"], ["standardFontDataUrl", "standard_fonts"]]) {
  for (const name of (await readdir(resolve(pdfRoot, folder))).sort()) {
    if (/\.(bcmap|pfb|ttf)$/.test(name)) assets[`${kind}/${name}`] = gzipSync(await readFile(resolve(pdfRoot, folder, name)), { level: 9 }).toString("base64");
    if (/^LICENSE/.test(name)) notices.push(`${folder}/${name}\n${await readFile(resolve(pdfRoot, folder, name), "utf8")}`);
  }
}
const bundle = await build({ ...settings, entryPoints: [resolve(root, "src/mcp-apps/work-result/pdf-preview.mjs")],
  format: "iife", globalName: "WebCodexPdf",
  define: {
    WEBCODEX_PDF_WORKER: JSON.stringify(gzipSync(worker.outputFiles[0].text + "\nself.postMessage({webcodexPdfReady:true});", { level: 9 }).toString("base64")),
    WEBCODEX_PDF_ASSETS: JSON.stringify(assets),
  },
});
const html = await readFile(path, "utf8");
if (html.split(begin).length !== 2 || html.split(end).length !== 2) throw new Error("Expected exactly one generated PDF region");
const start = html.indexOf(begin) + begin.length, finish = html.indexOf(end);
if (finish <= start) throw new Error("Invalid PDF generated region");
const script = (`/* Third-party licenses\n${notices.join("\n\n")}\n*/\n${bundle.outputFiles[0].text}`)
  .replace(/<\/script/gi, "<\\/script").replace(/[\t ]+$/gm, "");
if (script.includes(begin) || script.includes(end)) throw new Error("Reserved PDF marker in bundle");
const output = html.slice(0, start) + "\n" + script + html.slice(finish);
if (process.argv.includes("--check")) {
  if (output !== html) throw new Error("Work Result PDF bundle out of date; run npm --prefix frontend run build:work-result");
  console.log("Work Result PDF bundle checked");
} else {
  await writeFile(path, output);
  console.log(`Work Result PDF bundle generated · ${Buffer.byteLength(script)} bytes · ${Object.keys(assets).length} offline assets`);
}
