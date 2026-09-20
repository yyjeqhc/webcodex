import { build } from "vite";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const configFile = resolve(frontendRoot, "vite.runtime.config.ts");
const htmlSource = resolve(frontendRoot, "src/runtime-v2/runtime.html");
const expectedFiles = ["runtime.html", "app.js", "styles.css"];

function parseArguments(argv) {
  let outputDirectory = resolve(frontendRoot, "dist");
  let checkOnly = false;
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--check") {
      checkOnly = true;
      continue;
    }
    if (value === "--out-dir") {
      const target = argv[index + 1];
      if (!target || target.startsWith("--")) throw new Error("--out-dir requires a path");
      outputDirectory = resolve(frontendRoot, target);
      index += 1;
      continue;
    }
    throw new Error("unknown option: " + value);
  }
  return { outputDirectory, checkOnly };
}

async function buildInto(outputDirectory) {
  mkdirSync(outputDirectory, { recursive: true });
  await build({
    configFile,
    build: {
      outDir: outputDirectory,
      emptyOutDir: false,
    },
  });
  const html = readFileSync(htmlSource, "utf8").replace(/\r\n/g, "\n");
  writeFileSync(resolve(outputDirectory, "runtime.html"), html.endsWith("\n") ? html : html + "\n");
  for (const file of expectedFiles) {
    if (!existsSync(resolve(outputDirectory, file))) throw new Error("Runtime v2 build did not produce " + file);
  }
}

function compareOutputs(actualDirectory, expectedDirectory) {
  const drift = [];
  for (const file of expectedFiles) {
    const actualPath = resolve(actualDirectory, file);
    const expectedPath = resolve(expectedDirectory, file);
    const actual = existsSync(actualPath) ? readFileSync(actualPath) : Buffer.alloc(0);
    const expected = existsSync(expectedPath) ? readFileSync(expectedPath) : Buffer.alloc(0);
    if (!actual.equals(expected)) drift.push(file);
  }
  if (drift.length) {
    throw new Error(drift.join(", ") + " out of date; run: npm --prefix frontend run build");
  }
}

async function main() {
  const { outputDirectory, checkOnly } = parseArguments(process.argv.slice(2));
  if (!checkOnly) {
    await buildInto(outputDirectory);
    console.log("[runtime-v2] built " + outputDirectory);
    return;
  }
  const temporary = mkdtempSync(resolve(tmpdir(), "webcodex-runtime-v2-"));
  try {
    await buildInto(temporary);
    compareOutputs(outputDirectory, temporary);
    console.log("[runtime-v2] checked " + outputDirectory);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error("[runtime-v2] build failed: " + (error instanceof Error ? error.message : String(error)));
  process.exitCode = 1;
});
