import { build } from "esbuild";
import { readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const path = resolve(root, "../src/mcp_work_result_app.html");
const begin = "/* BEGIN GENERATED WORK RESULT MARKDOWN */";
const end = "/* END GENERATED WORK RESULT MARKDOWN */";
if (process.argv.slice(2).some(arg => arg !== "--check")) throw new Error("Only --check is supported");
const result = await build({
  entryPoints: [resolve(root, "src/mcp-apps/work-result-markdown.mjs")],
  bundle: true, platform: "browser", format: "iife", globalName: "WebCodexMarkdown",
  target: "es2022", charset: "utf8", minify: true, legalComments: "inline", write: false,
  supported: { "inline-script": true }, metafile: true,
});
const html = readFileSync(path, "utf8");
if (html.split(begin).length !== 2 || html.split(end).length !== 2) throw new Error("Expected exactly one generated Markdown region");
const start = html.indexOf(begin) + begin.length;
const finish = html.indexOf(end);
if (finish <= start) throw new Error("Invalid Markdown generated region");
const licenses = new Map();
for (const input of Object.keys(result.metafile.inputs)) {
  if (!input.includes("node_modules/")) continue;
  let dir = dirname(resolve(input));
  while (!existsSync(resolve(dir, "package.json")) && dirname(dir) !== dir) dir = dirname(dir);
  const pkg = JSON.parse(readFileSync(resolve(dir, "package.json"), "utf8"));
  const files = readdirSync(dir).filter(name => /^licen[sc]e(?:[._-]|$)/i.test(name)).sort();
  if (!files.length) throw new Error(`Missing license for ${pkg.name}`);
  licenses.set(pkg.name, `${pkg.name} ${pkg.version}\n${files.map(name => readFileSync(resolve(dir, name), "utf8")).join("\n")}`);
}
const banner = "/* Third-party licenses\n" + [...licenses].sort(([a], [b]) => a.localeCompare(b, "en")).map(([, text]) => text).join("\n\n") + "\n*/\n";
const script = (banner + result.outputFiles[0].text).replace(/<\/script/gi, "<\\/script").replace(/^[\t ]+$/gm, "");
const output = html.slice(0, start) + "\n" + script + html.slice(finish);
if (process.argv.includes("--check")) {
  if (output !== html) throw new Error("Work Result Markdown bundle out of date; run npm --prefix frontend run build:work-result");
  console.log("Work Result Markdown bundle checked");
} else { writeFileSync(path, output); console.log("Work Result Markdown bundle generated"); }
