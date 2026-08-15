import test from "node:test";
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(frontendRoot, "..");
const buildScript = resolve(frontendRoot, "scripts/build.mjs");
const requiredAssets = [
  "console.html",
  "app.js",
  "workflow_session_state.js",
  "styles.css",
  "admin.html",
  "admin.js",
  "admin.css",
  "admin_controller.js",
  "admin_mutation_controller.js",
  "admin_mutation_view.js",
  "admin_view.js",
];

function exec(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    execFile(command, args, options, (error, stdout, stderr) => {
      if (error) {
        reject(
          new Error(
            `${command} ${args.join(" ")} failed: ${stderr || error.message}`
          )
        );
      } else {
        resolvePromise({ stdout, stderr });
      }
    });
  });
}

async function assertRequiredAssets(outputDirectory) {
  for (const asset of requiredAssets) {
    assert.equal((await stat(resolve(outputDirectory, asset))).isFile(), true);
  }
  const app = await readFile(resolve(outputDirectory, "app.js"), "utf8");
  assert.equal(app.includes("interface Review"), false);
  assert.equal(/\.innerHTML\b|\binnerHTML\s*=/.test(app), false);
  assert.match(app, /workflow-session/);
  assert.match(app, /textContent/);
  assert.match(app, /workflow-session-overview-validation/);
  assert.match(app, /workflow-session-overview-progress/);
  const consoleHtml = await readFile(resolve(outputDirectory, "console.html"), "utf8");
  assert.match(consoleHtml, /workflow-session-overview-work/);
  assert.match(consoleHtml, /Reported progress/);
  assert.match(consoleHtml, /Model-reported; informational only\./);
  const styles = await readFile(resolve(outputDirectory, "styles.css"), "utf8");
  assert.match(styles, /workflow-session-summary-runtime/);
  await exec(process.execPath, ["--check", resolve(outputDirectory, "app.js")]);
  const admin = await readFile(resolve(outputDirectory, "admin.js"), "utf8");
  await exec(process.execPath, ["--check", resolve(outputDirectory, "admin.js")]);
  assert.equal(/localStorage|sessionStorage|document\.cookie/.test(admin), false);
  assert.equal(/innerHTML/.test(admin), false);
  assert.match(admin, /textContent/);
}

async function copySources(sourceDirectory) {
  await mkdir(sourceDirectory, { recursive: true });
  for (const source of [
    "app.ts",
    "review_state.ts",
    "workflow_session_state.ts",
    "styles.css",
    "console.html",
    "admin.ts",
    "admin_controller.ts",
    "admin_mutation_controller.ts",
    "admin_mutation_view.ts",
    "admin_view.ts",
    "admin.css",
    "admin.html",
  ]) {
    await copyFile(
      resolve(frontendRoot, "src", source),
      resolve(sourceDirectory, source)
    );
  }
}

async function waitFor(predicate, diagnostic, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 50));
  }
  throw new Error(diagnostic());
}

test("custom development build creates parseable fixed console assets", async () => {
  const outputDirectory = await mkdtemp(resolve(tmpdir(), "webcodex-assets-"));
  try {
    const result = await exec(process.execPath, [
      buildScript,
      "--out-dir",
      outputDirectory,
    ]);
    assert.match(result.stdout, /\[console\] built/);
    await assertRequiredAssets(outputDirectory);
  } finally {
    await rm(outputDirectory, { recursive: true, force: true });
  }
});

test(
  "watch mode rebuilds changed sources and preserves the last good bundle",
  { timeout: 20_000 },
  async () => {
    const workspace = await mkdtemp(resolve(tmpdir(), "webcodex-watch-"));
    const sourceDirectory = resolve(workspace, "src");
    const outputDirectory = resolve(workspace, "out");
    await copySources(sourceDirectory);
    const child = spawn(
      process.execPath,
      [
        buildScript,
        "--source-dir",
        sourceDirectory,
        "--out-dir",
        outputDirectory,
        "--watch",
      ],
      { stdio: ["ignore", "pipe", "pipe"] }
    );
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    try {
      await waitFor(
        () => stdout.includes("[console] watching"),
        () => `watcher did not start: ${stdout}\n${stderr}`
      );
      await assertRequiredAssets(outputDirectory);

      const changedCss = "body { color: rgb(1, 2, 3); }\n";
      await writeFile(resolve(sourceDirectory, "styles.css"), changedCss);
      await waitFor(
        async () =>
          (await readFile(resolve(outputDirectory, "styles.css"), "utf8")).includes(
            "rgb(1,2,3)"
          ),
        () => `watcher did not rebuild CSS: ${stdout}\n${stderr}`
      );

      const lastGoodApp = await readFile(resolve(outputDirectory, "app.js"), "utf8");
      await writeFile(resolve(sourceDirectory, "app.ts"), "const broken: = 1;\n");
      await waitFor(
        () => stderr.includes("[console] build failed:"),
        () => `watcher did not report the failed build: ${stdout}\n${stderr}`
      );
      assert.equal(
        await readFile(resolve(outputDirectory, "app.js"), "utf8"),
        lastGoodApp
      );
      await exec(process.execPath, ["--check", resolve(outputDirectory, "app.js")]);
    } finally {
      child.kill("SIGTERM");
      await new Promise((resolvePromise) => child.once("exit", resolvePromise));
      await rm(workspace, { recursive: true, force: true });
    }
  }
);

test("development output is ignored by Git", async () => {
  await exec(
    "git",
    ["check-ignore", "--quiet", "frontend/.dev-dist/app.js"],
    { cwd: repositoryRoot }
  );
});
