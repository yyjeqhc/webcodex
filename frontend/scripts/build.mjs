#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const checkOnly = process.argv.includes("--check");

function read(relPath) {
  return readFileSync(resolve(root, relPath), "utf8");
}

function write(relPath, content) {
  const fullPath = resolve(root, relPath);
  mkdirSync(dirname(fullPath), { recursive: true });
  writeFileSync(fullPath, content);
}

function normalizeNewline(content) {
  return content.replace(/\r\n/g, "\n").trim() + "\n";
}

function stripTypeScript(source) {
  let js = source;
  js = js.replace(/^type\s+RequestOptions\s*=.*?;\n\n/s, "");
  js = js.replace(/^declare\s+global\s*\{[\s\S]*?^\}\n\n/m, "");
  js = js.replace(/^export\s*\{\};\s*\n?/gm, "");
  js = js.replace(/: RequestOptions(?=\s*[=,)])/g, "");
  js = js.replace(/: (string|number|unknown|boolean|any)(?=\s*[=,)])/g, "");
  // DOM event-handler parameter types (single identifiers). Safe because the
  // only JS context where `: <Word>` appears before `=`, `,`, or `)` is a TS
  // type annotation; object-literal values like `{ key: Event, }` are avoided
  // in the source by contract.
  js = js.replace(/: (Event|SubmitEvent|MouseEvent|KeyboardEvent|ChangeEvent)(?=\s*[=,)])/g, "");
  // `as <Identifier>` type assertions (e.g. `node as HTMLInputElement`).
  // `as` is not a JS operator, so stripping `as <Word>` is safe; generic
  // casts like `as Array<T>` are intentionally not used in the source.
  js = js.replace(/\bas\s+[A-Za-z_]\w*/g, "");
  js = js.replace(/: Promise<Response \| null>(?=\s*\{)/g, "");
  js = js.replace(/: Promise<void>(?=\s*\{)/g, "");
  js = js.replace(/: Promise<any>(?=\s*\{)/g, "");
  js = js.replace(/: (boolean|string|void|number|any)(?=\s*\{)/g, "");
  return js;
}

function buildJs(source) {
  // Keep generated JS readable and avoid whitespace-sensitive rewrites inside
  // template literals. CSS is safe to minify below; JS only needs deterministic
  // TypeScript stripping for the current no-bundler frontend.
  return normalizeNewline(source);
}

function minifyCss(source) {
  return normalizeNewline(source)
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\s+/g, " ")
    .replace(/\s*([{}:;,>])\s*/g, "$1")
    .replace(/;}/g, "}")
    .replace(/0\.([0-9]+)/g, ".$1")
    .trim() + "\n";
}

// Turn an ESM module into classic-script statements for inlining: drop the
// `export {}` module marker and the `export` keyword on top-level declarations.
function stripModuleExports(js) {
  return js
    .replace(/^export\s*\{\};\s*\n?/gm, "")
    .replace(/^export\s+(function|const|let|class)\b/gm, "$1");
}

// The pure review-identity state machine. Emitted as an ESM module so the Node
// test runner can import it, and inlined into app.js for the no-bundler browser.
const reviewStateModule = buildJs(stripTypeScript(read("src/review_state.ts")));

// app.js is a single classic script: inline the state module and drop its ESM
// import so the browser needs no bundler and no extra fetch.
const appStripped = stripTypeScript(read("src/app.ts")).replace(
  /^import\s*\{[\s\S]*?\}\s*from\s*["']\.\/review_state(?:\.js)?["'];\s*\n/m,
  ""
);
const appInlined = buildJs(stripModuleExports(reviewStateModule) + "\n" + appStripped);

const outputs = new Map([
  ["dist/review_state.js", reviewStateModule],
  ["dist/app.js", appInlined],
  ["dist/styles.css", minifyCss(read("src/styles.css"))],
  // The console HTML shell is copied verbatim (no transform needed).
  ["dist/console.html", normalizeNewline(read("src/console.html"))],
]);

let drift = false;
for (const [relPath, expected] of outputs) {
  const fullPath = resolve(root, relPath);
  if (checkOnly) {
    const actual = existsSync(fullPath) ? readFileSync(fullPath, "utf8") : "";
    if (actual !== expected) {
      console.error(`${relPath} is out of date. Run: npm --prefix frontend run build`);
      drift = true;
    }
  } else {
    write(relPath, expected);
    console.log(`wrote ${relPath}`);
  }
}

if (drift) process.exit(1);
