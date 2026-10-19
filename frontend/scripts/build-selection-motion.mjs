import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const JS_BEGIN = "/* BEGIN GENERATED SELECTION MOTION */";
export const JS_END = "/* END GENERATED SELECTION MOTION */";
export const CSS_BEGIN = "/* BEGIN GENERATED SELECTION MOTION CSS */";
export const CSS_END = "/* END GENERATED SELECTION MOTION CSS */";

// Windows Git checkouts may use CRLF; committed/generated regions use LF.
export function normalizeLineEndings(text) { return text.replace(/\r\n/g, "\n"); }

export function replaceSelectionRegion(html, content, begin, end) {
  if (html.split(begin).length !== 2 || html.split(end).length !== 2) throw new Error(`Expected exactly one selection motion region: ${begin}`);
  const start = html.indexOf(begin) + begin.length;
  const finish = html.indexOf(end);
  if (finish <= start) throw new Error("Invalid selection motion region order");
  if (content.includes(begin) || content.includes(end)) throw new Error("Bundle contains a reserved region marker");
  return html.slice(0, start) + "\n" + content.trimEnd() + "\n" + html.slice(finish);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.some(arg => arg !== "--check")) throw new Error("Only --check is supported");
  const apps = [
    { name: "workbench", brand: "WORKBENCH" },
    { name: "work_result", brand: "WORK RESULT", skin: "src/mcp-apps/work-result/work-result.css" },
  ];
  const results = await Promise.all([
    build({ absWorkingDir: root, entryPoints: ["src/ui/selectionMotion.ts"], bundle: true,
      platform: "browser", format: "iife", globalName: "WebCodexSelectionMotion", target: "es2022",
      minify: true, legalComments: "inline", write: false, supported: { "inline-script": true }, metafile: true }),
    build({ absWorkingDir: root, entryPoints: ["src/ui/selectionMotion.css"], bundle: true,
      minify: true, write: false, metafile: true }),
  ]);
  if (results.some(result => Object.keys(result.metafile.inputs).some(path => path.split(/[\\/]/).includes("node_modules")))) {
    throw new Error("Selection motion bundle must contain only first-party modules");
  }

  const script = results[0].outputFiles[0].text.replace(/<\/script/gi, "<\\/script");
  const css = results[1].outputFiles[0].text.replace(/<\/style/gi, "<\\/style");
  const brand = normalizeLineEndings(readFileSync(resolve(root, "src/ui/brand-mark.svg"), "utf8"));
  for (const app of apps) {
    const path = resolve(root, `../src/mcp_${app.name}_app.html`);
    const html = normalizeLineEndings(readFileSync(path, "utf8"));
    const appCss = app.skin ? css + "\n" + normalizeLineEndings(readFileSync(resolve(root, app.skin), "utf8")) : css;
    const output = replaceSelectionRegion(replaceSelectionRegion(replaceSelectionRegion(html, script, JS_BEGIN, JS_END), appCss, CSS_BEGIN, CSS_END), brand, `<!-- BEGIN ${app.brand} BRAND -->`, `<!-- END ${app.brand} BRAND -->`);
    if (args.includes("--check")) {
      if (output !== html) throw new Error(`${app.name} selection motion bundle out of date; run node frontend/scripts/build-selection-motion.mjs`);
      console.log(`${app.name} selection motion bundle checked`);
    } else {
      writeFileSync(path, output);
      console.log(`${app.name} selection motion bundle generated`);
    }
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
