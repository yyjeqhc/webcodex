#!/usr/bin/env node
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  watch,
  writeFileSync,
} from "node:fs";
import { basename, dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Script } from "node:vm";
import ts from "typescript";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ADMIN_MODULES = Object.freeze([
  "admin_controller.ts",
  "admin_mutation_controller.ts",
  "admin_mutation_view.ts",
  "admin_view.ts",
]);
const watchedSources = new Set([...ADMIN_MODULES, "admin.ts", "admin.css", "admin.html"]);

function readSource(sourceDirectory, fileName) {
  return readFileSync(resolve(sourceDirectory, fileName), "utf8");
}

function normalizeNewline(content) {
  return content.replace(/\r\n/g, "\n").trim() + "\n";
}

const diagnosticHost = {
  getCanonicalFileName: (fileName) => fileName,
  getCurrentDirectory: () => root,
  getNewLine: () => "\n",
};

function transpileTypeScriptSource(source, fileName) {
  const result = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2020,
      module: ts.ModuleKind.ES2020,
      newLine: ts.NewLineKind.LineFeed,
      removeComments: false,
      sourceMap: false,
      inlineSourceMap: false,
    },
    fileName,
    reportDiagnostics: true,
  });
  const errors = (result.diagnostics || []).filter(
    (diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error
  );
  if (errors.length) throw new Error(ts.formatDiagnostics(errors, diagnosticHost).trim());
  return normalizeNewline(result.outputText);
}

function transpileTypeScript(sourceDirectory, fileName) {
  const sourcePath = resolve(sourceDirectory, fileName);
  return transpileTypeScriptSource(readSource(sourceDirectory, fileName), sourcePath);
}

function stripModuleExports(js) {
  return js
    .replace(/^export\s*\{\};\s*\n?/gm, "")
    .replace(/^export\s+((?:async\s+)?(?:function|const|let|var|class))\b/gm, "$1");
}

function stripAdminImports(js) {
  let output = js;
  for (const moduleName of ADMIN_MODULES) {
    const stem = moduleName.replace(/\.ts$/, "");
    const pattern = new RegExp(
      '^import\\s*\\{[\\s\\S]*?\\}\\s*from\\s*["\\\']\\./' +
        stem +
        '(?:\\.js)?["\\\'];?\\s*\\n',
      "m"
    );
    output = output.replace(pattern, "");
  }
  return output;
}

function assertClassicScript(label, source) {
  try {
    new Script(source, { filename: label });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(label + " is not valid browser JavaScript: " + message);
  }
}

function minifyCss(source) {
  return (
    normalizeNewline(source)
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\s+/g, " ")
      .replace(/\s*([{}:;,>])\s*/g, "$1")
      .replace(/;}/g, "}")
      .replace(/0\.([0-9]+)/g, ".$1")
      .trim() + "\n"
  );
}

export function createOutputs(outputDirectory, sourceDirectory = resolve(root, "src")) {
  const modules = new Map();
  for (const fileName of ADMIN_MODULES) {
    modules.set(fileName, transpileTypeScript(sourceDirectory, fileName));
  }

  const adminScript = normalizeNewline(
    ADMIN_MODULES.map((fileName) => stripModuleExports(modules.get(fileName))).join("\n") +
      "\n" +
      stripModuleExports(stripAdminImports(transpileTypeScript(sourceDirectory, "admin.ts")))
  );
  assertClassicScript(resolve(outputDirectory, "admin.js"), adminScript);

  return new Map([
    ["admin_controller.js", modules.get("admin_controller.ts")],
    ["admin_mutation_controller.js", modules.get("admin_mutation_controller.ts")],
    ["admin_mutation_view.js", modules.get("admin_mutation_view.ts")],
    ["admin_view.js", modules.get("admin_view.ts")],
    ["admin.js", adminScript],
    ["admin.css", minifyCss(readSource(sourceDirectory, "admin.css"))],
    ["admin.html", normalizeNewline(readSource(sourceDirectory, "admin.html"))],
  ]);
}

function atomicWriteOutputs(outputDirectory, outputs) {
  mkdirSync(outputDirectory, { recursive: true });
  const nonce = process.pid + "-" + Date.now();
  const staged = [];
  try {
    let index = 0;
    for (const [name, content] of outputs) {
      const finalPath = resolve(outputDirectory, name);
      const temporaryPath = resolve(outputDirectory, "." + basename(name) + "." + nonce + "-" + index + ".tmp");
      index += 1;
      writeFileSync(temporaryPath, content);
      staged.push({ finalPath, temporaryPath });
    }
    for (const entry of staged) renameSync(entry.temporaryPath, entry.finalPath);
  } finally {
    for (const entry of staged) rmSync(entry.temporaryPath, { force: true });
  }
}

function checkOutputs(outputDirectory, outputs) {
  const drift = [];
  for (const [name, expected] of outputs) {
    const fullPath = resolve(outputDirectory, name);
    const actual = existsSync(fullPath) ? readFileSync(fullPath, "utf8") : "";
    if (actual !== expected) drift.push(name);
  }
  if (drift.length) {
    throw new Error(drift.join(", ") + " out of date; run: npm --prefix frontend run build");
  }
}

export function runBuild({
  outputDirectory,
  sourceDirectory = resolve(root, "src"),
  checkOnly = false,
}) {
  const startedAt = Date.now();
  const outputs = createOutputs(outputDirectory, sourceDirectory);
  if (checkOnly) checkOutputs(outputDirectory, outputs);
  else atomicWriteOutputs(outputDirectory, outputs);
  const displayDirectory = relative(root, outputDirectory) || basename(outputDirectory);
  console.log(
    "[admin] " + (checkOnly ? "checked " : "built ") + displayDirectory +
    " (" + outputs.size + " files, " + (Date.now() - startedAt) + "ms)"
  );
}

function parseArguments(argv) {
  let outputDirectory = resolve(root, "dist");
  let sourceDirectory = resolve(root, "src");
  let checkOnly = false;
  let watchMode = false;
  for (let index = 0; index < argv.length; index += 1) {
    switch (argv[index]) {
      case "--check":
        checkOnly = true;
        break;
      case "--watch":
        watchMode = true;
        break;
      case "--admin-only":
        // Compatibility for one transition release. Runtime assets are always Vite-built now.
        break;
      case "--out-dir": {
        const value = argv[index + 1];
        if (!value || value.startsWith("--")) throw new Error("--out-dir requires a path");
        outputDirectory = resolve(root, value);
        index += 1;
        break;
      }
      case "--source-dir": {
        const value = argv[index + 1];
        if (!value || value.startsWith("--")) throw new Error("--source-dir requires a path");
        sourceDirectory = resolve(root, value);
        index += 1;
        break;
      }
      default:
        throw new Error("unknown option: " + argv[index]);
    }
  }
  if (checkOnly && watchMode) throw new Error("--check and --watch cannot be used together");
  return { outputDirectory, sourceDirectory, checkOnly, watchMode };
}

function startWatcher(outputDirectory, sourceDirectory) {
  let debounceTimer;
  let closed = false;
  const rebuild = () => {
    debounceTimer = undefined;
    try {
      runBuild({ outputDirectory, sourceDirectory });
    } catch (error) {
      console.error("[admin] build failed: " + (error instanceof Error ? error.message : String(error)));
    }
  };
  const schedule = () => {
    if (closed) return;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(rebuild, 100);
  };
  const sourceWatcher = watch(sourceDirectory, (_event, fileName) => {
    const name = fileName === null ? null : fileName.toString();
    if (name === null || watchedSources.has(name)) schedule();
  });
  console.log("[admin] watching " + [...watchedSources].sort().join(", "));
  const close = () => {
    if (closed) return;
    closed = true;
    if (debounceTimer) clearTimeout(debounceTimer);
    sourceWatcher.close();
  };
  process.once("SIGINT", close);
  process.once("SIGTERM", close);
}

function main() {
  const options = parseArguments(process.argv.slice(2));
  runBuild(options);
  if (options.watchMode) startWatcher(options.outputDirectory, options.sourceDirectory);
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error("[admin] build failed: " + (error instanceof Error ? error.message : String(error)));
    process.exitCode = 1;
  }
}
