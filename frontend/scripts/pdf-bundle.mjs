import { build } from "esbuild";
import { readFile, readdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pdfRoot = resolve(root, "node_modules/pdfjs-dist");
function gzipBase64(bytes) {
  const compressed = gzipSync(bytes, { level: 9 });
  // RFC 1952's unspecified OS value keeps checked-in bundles platform-independent.
  compressed[9] = 255;
  return compressed.toString("base64");
}
export async function buildPdfBundle(entryPoint, globalName) {
const settings = { bundle: true, platform: "browser", target: "es2022", minify: true,
  write: false, legalComments: "inline", supported: { "inline-script": true } };
const worker = await build({ ...settings, entryPoints: [resolve(pdfRoot, "build/pdf.worker.mjs")], format: "esm" });
const assets = {}, notices = [`pdfjs-dist ${JSON.parse(await readFile(resolve(pdfRoot, "package.json"), "utf8")).version}\n${await readFile(resolve(pdfRoot, "LICENSE"), "utf8")}`];
for (const [kind, folder] of [["cMapUrl", "cmaps"], ["standardFontDataUrl", "standard_fonts"]]) {
  for (const name of (await readdir(resolve(pdfRoot, folder))).sort()) {
    if (/\.(bcmap|pfb|ttf)$/.test(name)) assets[`${kind}/${name}`] = gzipBase64(await readFile(resolve(pdfRoot, folder, name)));
    if (/^LICENSE/.test(name)) notices.push(`${folder}/${name}\n${await readFile(resolve(pdfRoot, folder, name), "utf8")}`);
  }
}
const bundle = await build({ ...settings, entryPoints: [resolve(root, entryPoint)],
  format: "iife", globalName,
  define: {
    WEBCODEX_PDF_WORKER: JSON.stringify(gzipBase64(worker.outputFiles[0].text + "\nself.postMessage({webcodexPdfReady:true});")),
    WEBCODEX_PDF_ASSETS: JSON.stringify(assets),
  },
});
return { script: `/* Third-party licenses\n${notices.join("\n\n")}\n*/\n${bundle.outputFiles[0].text}`, assetCount: Object.keys(assets).length };
}
