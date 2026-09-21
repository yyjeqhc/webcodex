import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { checkedProjectPath, createGuardedFindTool, createGuardedGrepTool, createGuardedLsTool, createGuardedReadTool, readProjectFile, safeRelative } from "./dist/project-files.js";

async function fixture(run) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "webpi-files-security-"));
  try { await run(root); } finally { await fs.rm(root, { recursive: true, force: true }); }
}
const text = (result) => result.content.filter((block) => block.type === "text").map((block) => block.text).join("\n");

test("portable path guard rejects credentials, device aliases, streams and traversal", () => {
  for (const candidate of ["../outside", "nested/../../outside", "C:relative", "C:\\absolute", "\\\\server\\share", "~/.ssh/id_rsa", "file.txt:stream", "NUL", "dir/CON.txt", "dir./file", "dir /file", "a\n.txt", ".webpi-state/server/webpi.env", ".WEBPI-STATE/webpi-action-token", "sub/.env.production", "nested/.git/config", ".npmrc", "cert.key", "secret.json"]) {
    assert.throws(() => safeRelative(candidate), undefined, candidate);
  }
  assert.equal(safeRelative("src/main.ts"), path.join("src", "main.ts"));
  assert.equal(safeRelative(undefined), ".");
});

test("bounded read rejects directories and hard links without returning their content", async () => fixture(async (root) => {
  await fs.writeFile(path.join(root, "normal.txt"), "ordinary source\n");
  assert.equal((await readProjectFile(root, "normal.txt")).toString(), "ordinary source\n");
  await fs.link(path.join(root, "normal.txt"), path.join(root, "alias.txt"));
  await assert.rejects(readProjectFile(root, "alias.txt"), /single-link/u);
  await assert.rejects(readProjectFile(root, "."), /regular|directory|EISDIR|EPERM/u);
}));

test("junction or symlink traversal is rejected at intermediate components", async () => fixture(async (root) => {
  const target = path.join(root, "actual");
  await fs.mkdir(target);
  await fs.writeFile(path.join(target, "data.txt"), "not exposed through an alias");
  await fs.symlink(target, path.join(root, "alias"), process.platform === "win32" ? "junction" : "dir");
  await assert.rejects(checkedProjectPath(root, "alias/data.txt"), /symlink|junction/u);
  await assert.rejects(readProjectFile(root, "alias/data.txt"), /symlink|junction/u);
}));

test("read snapshot bounds allocation before loading oversized content", async () => fixture(async (root) => {
  const handle = await fs.open(path.join(root, "large.txt"), "w");
  await handle.truncate(8 * 1024 * 1024 + 1);
  await handle.close();
  await assert.rejects(readProjectFile(root, "large.txt"), /8 MiB/u);
}));

test("native Pi read keeps image content through its guarded snapshot backend", async () => fixture(async (root) => {
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC", "base64");
  await fs.writeFile(path.join(root, "pixel.png"), png);
  const result = await createGuardedReadTool(root).execute("read-image", { path: "pixel.png" });
  assert.ok(result.content.some((block) => block.type === "image" && block.mimeType === "image/png"));
}));

test("recursive search, find and ls never publish synthetic secret contents or paths", async () => fixture(async (root) => {
  await fs.mkdir(path.join(root, "src"));
  await fs.mkdir(path.join(root, ".webpi-state", "server"), { recursive: true });
  await fs.writeFile(path.join(root, "src", "main.ts"), "needle-public\n");
  await fs.writeFile(path.join(root, ".env"), "needle-DO-NOT-RETURN\n");
  await fs.writeFile(path.join(root, ".webpi-state", "server", "webpi.env"), "needle-DO-NOT-RETURN\n");
  await fs.symlink(path.join(root, ".webpi-state"), path.join(root, "innocent-alias"), process.platform === "win32" ? "junction" : "dir");
  const grep = await createGuardedGrepTool(root).execute("grep", { pattern: "needle", path: ".", literal: true });
  assert.match(text(grep), /needle-public/u);
  assert.doesNotMatch(text(grep), /DO-NOT-RETURN|webpi-state|innocent-alias|\.env/u);
  const forced = await createGuardedGrepTool(root).execute("grep-forced", { pattern: "needle", glob: ".env", literal: true });
  assert.doesNotMatch(text(forced), /DO-NOT-RETURN/u);
  const find = await createGuardedFindTool(root).execute("find", { pattern: "*", path: "." });
  assert.match(text(find), /main\.ts/u);
  assert.doesNotMatch(text(find), /webpi-state|innocent-alias|\.env/u);
  const listed = await createGuardedLsTool(root).execute("ls", { path: "." });
  assert.match(text(listed), /src/u);
  assert.doesNotMatch(text(listed), /webpi-state|innocent-alias|\.env/u);
}));
