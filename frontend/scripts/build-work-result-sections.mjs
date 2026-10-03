import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const BEGIN = "/* BEGIN GENERATED WORK RESULT SECTIONS */";
export const END = "/* END GENERATED WORK RESULT SECTIONS */";

// A closed generated region, not a fuzzy source editor. A missing/duplicate or
// reversed marker rejects before writing; everything outside stays byte-identical.
export function replaceSectionBundle(html, script) {
  if (html.split(BEGIN).length !== 2 || html.split(END).length !== 2) throw new Error("Expected exactly one generated sections region");
  const start = html.indexOf(BEGIN) + BEGIN.length;
  const finish = html.indexOf(END);
  if (finish <= start) throw new Error("Invalid sections generated region");
  if (script.includes(BEGIN) || script.includes(END)) throw new Error("Bundle contains a reserved region marker");
  const safeScript = script.replace(/<\/script/gi, "<\\/script").replace(/^[\t ]+$/gm, "");
  return html.slice(0, start) + "\n" + safeScript.trimEnd() + "\n" + html.slice(finish);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.some(arg => arg !== "--check")) throw new Error("Only --check is supported");
  const path = resolve(root, "../src/mcp_work_result_app.html");
  const result = await build({
    absWorkingDir: root,
    entryPoints: ["src/mcp-apps/work-result/sections.mjs"],
    bundle: true, platform: "browser", format: "iife", globalName: "WebCodexWorkResultSections",
    target: "es2022", minify: true, legalComments: "inline", write: false,
    supported: { "inline-script": true }, metafile: true,
  });
  // This bundle is first-party only. New package dependencies need an explicit
  // license/size review rather than silently entering the embedded App.
  if (Object.keys(result.metafile.inputs).some(path => path.split(/[\\/]/).includes("node_modules"))) throw new Error("Sections bundle must contain only bundled first-party modules");
  const html = readFileSync(path, "utf8");
  const output = replaceSectionBundle(html, result.outputFiles[0].text);
  if (args.includes("--check")) {
    if (output !== html) throw new Error("Work Result sections bundle out of date; run npm --prefix frontend run build:work-result");
    console.log("Work Result sections bundle checked");
  } else {
    writeFileSync(path, output);
    console.log("Work Result sections bundle generated");
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
