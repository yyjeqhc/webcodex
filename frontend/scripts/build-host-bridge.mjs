import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const begin = "/* BEGIN GENERATED HOST BRIDGE */";
const end = "/* END GENERATED HOST BRIDGE */";
const args = process.argv.slice(2);
if (args.some(arg => arg !== "--check")) throw new Error("Only --check is supported");
const bundle = await build({
  absWorkingDir: root, entryPoints: ["src/mcp-apps/host-bridge.mjs"], bundle: true,
  platform: "browser", format: "iife", globalName: "WebCodexHostBridge", target: "es2022",
  minify: true, legalComments: "inline", write: false, supported: { "inline-script": true }, metafile: true,
});
if (Object.keys(bundle.metafile.inputs).some(path => path.split(/[\\/]/).includes("node_modules"))) {
  throw new Error("Host bridge must contain only first-party modules");
}
const script = bundle.outputFiles[0].text.replace(/<\/script/gi, "<\\/script").trimEnd();
for (const name of ["agent_continuation", "job_terminal_continuation", "goal_plan", "work_result"]) {
  const path = resolve(root, `../src/mcp_${name}_app.html`);
  const html = readFileSync(path, "utf8").replace(/\r\n/g, "\n");
  if (html.split(begin).length !== 2 || html.split(end).length !== 2 || html.indexOf(end) <= html.indexOf(begin)) {
    throw new Error(`Expected one ordered host bridge region in ${name}`);
  }
  const output = html.slice(0, html.indexOf(begin) + begin.length) + "\n" + script + "\n" + html.slice(html.indexOf(end));
  if (args.includes("--check")) {
    if (output !== html) throw new Error(`${name} host bridge is stale; run npm --prefix frontend run build:host-bridge`);
  } else writeFileSync(path, output);
  console.log(`${name} host bridge ${args.includes("--check") ? "checked" : "generated"}`);
}
