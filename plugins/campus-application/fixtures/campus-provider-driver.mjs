// Owned-fixture adapter: use the actual stdio provider with an isolated fictional profile.
// Input/output are JSONL: { name, arguments } -> tool structuredContent.
// Keep this process alive between plan_fill and both reconcile_fill calls.
import { spawn } from "node:child_process";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import { once } from "node:events";

const directory = await mkdtemp(join(tmpdir(), "campus-browser-e2e-"));
let child;
try {
  const profile = JSON.parse(await readFile(new URL("../profile.example.json", import.meta.url), "utf8"));
  profile.personal.native_place = "Sichuan / Chengdu";
  profile.personal.native_place_province = "Sichuan";
  profile.personal.native_place_city = "Chengdu";
  profile.education[0].graduation_date = "2027-06-30";
  profile.attachments.resume_path = process.env.CAMPUS_FIXTURE_RESUME_PATH
    ?? "plugins/campus-application/fixtures/sample-resume.pdf";
  const profilePath = join(directory, "profile.json");
  await writeFile(profilePath, JSON.stringify(profile));
  child = spawn(process.execPath, [fileURLToPath(new URL("../dist/plugin.js", import.meta.url))], {
    stdio: ["pipe", "pipe", "pipe"],
    env: { ...process.env,
      WEBCODEX_CAMPUS_APPLICATION_PROFILE: profilePath,
      WEBCODEX_CAMPUS_APPLICATION_MAPPING_MEMORY: join(directory, "mapping-memory.json"),
    },
  });
  let stderr = "";
  child.stderr.setEncoding("utf8").on("data", chunk => { stderr = (stderr + chunk).slice(-16000); });
  const responses = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const requests = createInterface({ input: process.stdin, crlfDelay: Infinity });
  let id = 0;
  for await (const line of requests) {
    const request = JSON.parse(line);
    if (!["plan_fill", "reconcile_fill"].includes(request.name)) throw new Error("Unsupported fixture tool");
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: ++id, method: "tools/call",
      params: { name: request.name, arguments: request.arguments } }) + "\n");
    let timer;
    try {
      const responseLine = await Promise.race([
        responses.next(),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("Provider response timed out")), 30000); }),
      ]);
      if (responseLine.done) throw new Error("Provider ended before replying: " + stderr);
      const response = JSON.parse(responseLine.value);
      if (response.id !== id || response.error || response.result?.isError !== false
        || response.result.structuredContent === undefined) {
        throw new Error("Provider call failed: " + JSON.stringify(response));
      }
      process.stdout.write(JSON.stringify(response.result.structuredContent) + "\n");
    } finally { clearTimeout(timer); }
  }
  child.stdin.end();
  if (child.exitCode === null) await once(child, "exit");
  if (child.exitCode !== 0) throw new Error("Provider exited unsuccessfully: " + stderr);
} catch (error) {
  process.stderr.write(String(error) + "\n");
  process.exitCode = 1;
} finally {
  if (child && child.exitCode === null) {
    child.kill();
    await once(child, "exit");
  }
  await rm(directory, { recursive: true, force: true });
}
