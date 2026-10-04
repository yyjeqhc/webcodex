import { build } from "esbuild";
import { readFile, readdir, mkdir, writeFile, copyFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const root = fileURLToPath(new URL(".", import.meta.url));
const pdfRoot = resolve(root, "node_modules/pdfjs-dist");
const options = { bundle: true, platform: "browser", target: "es2022", minify: true,
  write: false, legalComments: "inline", supported: { "inline-script": true } };
const worker = await build({ ...options, entryPoints: [resolve(pdfRoot, "build/pdf.worker.mjs")], format: "esm" });
const assets = {};
for (const [kind, folder] of [["cMapUrl", "cmaps"], ["standardFontDataUrl", "standard_fonts"]]) {
  for (const name of (await readdir(resolve(pdfRoot, folder))).sort()) {
    if (!/\.(bcmap|pfb|ttf)$/.test(name)) continue;
    assets[`${kind}/${name}`] = gzipSync(await readFile(resolve(pdfRoot, folder, name)), { level: 9 }).toString("base64");
  }
}
const output = await build({ ...options, entryPoints: [resolve(root, "viewer.mjs")], format: "iife",
  define: { PDF_POC_WORKER: JSON.stringify(gzipSync(worker.outputFiles[0].text + "\nself.postMessage({pdfPocReady:true});", { level: 9 }).toString("base64")),
    PDF_POC_ASSETS: JSON.stringify(assets) } });
const css = await readFile(resolve(pdfRoot, "web/pdf_viewer.css"), "utf8");
const template = await readFile(resolve(root, "viewer.html"), "utf8");
const html = template.replace("<!-- PDF_POC_STYLE -->", () => `<style>${css.replace(/<\/style/gi, "<\\/style")}</style>`)
  .replace("<!-- PDF_POC_SCRIPT -->", () => `<script>${output.outputFiles[0].text.replace(/<\/script/gi, "<\\/script")}</script>`);
await mkdir(resolve(root, "dist"), { recursive: true });
await writeFile(resolve(root, "dist/viewer.html"), html);
const host = await build({ ...options, entryPoints: [resolve(root, "host.mjs")], format: "iife" });
await writeFile(resolve(root, "dist/host.js"), host.outputFiles[0].text);
const license = await readFile(resolve(pdfRoot, "LICENSE"), "utf8");
await writeFile(resolve(root, "dist/PDFJS-LICENSE.txt"), license);
const notices = [license];
for (const folder of ["cmaps", "standard_fonts"]) {
  for (const name of (await readdir(resolve(pdfRoot, folder))).sort()) {
    if (/^LICENSE/.test(name)) notices.push(`${folder}/${name}\n${await readFile(resolve(pdfRoot, folder, name), "utf8")}`);
  }
}
await writeFile(resolve(root, "dist/THIRD_PARTY_NOTICES.txt"), notices.join("\n\n"));
for (const name of ["text", "cjk", "scan"]) await copyFile(resolve(root, `fixtures/${name}.pdf`), resolve(root, `dist/${name}.pdf`));
const manifest = { pdfjs: JSON.parse(await readFile(resolve(pdfRoot, "package.json"))).version,
  htmlBytes: Buffer.byteLength(html), htmlGzipBytes: gzipSync(html, { level: 9 }).length,
  assetFiles: Object.keys(assets).length, runtimeExternalResources: 0 };
await writeFile(resolve(root, "dist/build.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(JSON.stringify(manifest));
