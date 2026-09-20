import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const adminBuild = resolve(frontendRoot, "scripts/build.mjs");
const runtimeBuild = resolve(frontendRoot, "scripts/build-runtime-v2.mjs");

function exec(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    execFile(command, args, options, (error, stdout, stderr) => {
      if (error) {
        reject(new Error(command + " " + args.join(" ") + " failed: " + (stderr || error.message)));
      } else {
        resolvePromise({ stdout, stderr });
      }
    });
  });
}

async function assertFile(directory, name) {
  assert.equal((await stat(resolve(directory, name))).isFile(), true, name);
}

test("Admin build is independent from the retired Runtime classic bundle", async () => {
  const output = await mkdtemp(resolve(tmpdir(), "webcodex-admin-build-"));
  try {
    await exec(process.execPath, [adminBuild, "--out-dir", output]);
    const files = (await readdir(output)).sort();
    assert.deepEqual(files, [
      "admin.css",
      "admin.html",
      "admin.js",
      "admin_controller.js",
      "admin_mutation_controller.js",
      "admin_mutation_view.js",
      "admin_view.js",
    ]);
    const admin = await readFile(resolve(output, "admin.js"), "utf8");
    await exec(process.execPath, ["--check", resolve(output, "admin.js")]);
    assert.equal(/localStorage|sessionStorage|document\.cookie/.test(admin), false);
    assert.equal(/innerHTML/.test(admin), false);
    assert.match(admin, /textContent/);
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("Runtime Vite build emits deterministic self-contained Server assets", async () => {
  const output = await mkdtemp(resolve(tmpdir(), "webcodex-runtime-build-"));
  try {
    await exec(process.execPath, [runtimeBuild, "--out-dir", output]);
    for (const name of ["runtime.html", "app.js", "styles.css"]) await assertFile(output, name);
    const files = await readdir(output);
    assert.equal(files.includes("runtime.js"), false);
    assert.equal(files.includes("runtime.css"), false);
    assert.equal(files.some((name) => name.startsWith("runtime_") && name.endsWith(".js")), false);

    const html = await readFile(resolve(output, "runtime.html"), "utf8");
    assert.match(html, /WebCodex — Runtime Workspace/);
    assert.match(html, /id="root"/);
    assert.match(html, /\/runtime\/app\.js/);
    assert.match(html, /\/runtime\/styles\.css/);
    assert.match(html, /viewport-fit=cover/);
    assert.match(html, /webcodex\.runtime\.appearance\.v1/);
    assert.match(html, /webcodex\.runtime\.language\.v1/);
    assert.equal(html.includes("prototype-v2"), false);

    const app = await readFile(resolve(output, "app.js"), "utf8");
    await exec(process.execPath, ["--check", resolve(output, "app.js")]);
    assert.match(app, /\/api\/runtime-console\//);
    assert.match(app, /workflow-session-post-message/);
    assert.match(app, /communication\/agent\/create/);
    assert.match(app, /communication\/conversation\/create/);
    assert.match(app, /communication\/message\/post/);
    assert.match(app, /communication\/inbox\/consume/);
    assert.match(app, /sessionStorage/);
    assert.match(app, /localStorage/);
    assert.equal(/document\.cookie/.test(app), false);

    const styles = await readFile(resolve(output, "styles.css"), "utf8");
    assert.match(styles, /prefers-reduced-motion/);
    assert.match(styles, /prefers-reduced-transparency/);
    assert.match(styles, /safe-area-inset-bottom/);
    assert.match(styles, /data-resolved-theme/);
    assert.match(styles, /max-width:700px/);

    await exec(process.execPath, [runtimeBuild, "--out-dir", output, "--check"]);
    await writeFile(resolve(output, "app.js"), app + "\n/* drift */\n");
    await assert.rejects(
      exec(process.execPath, [runtimeBuild, "--out-dir", output, "--check"]),
      /app\.js out of date/
    );
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});
