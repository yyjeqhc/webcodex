import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const exec = promisify(execFile);
const frontendRoot = fileURLToPath(new URL("../", import.meta.url));

async function buildWithNativeGzipOS(osCode) {
  const { stdout } = await exec(process.execPath, ["--input-type=module", "-e", `
    import assert from "node:assert/strict";
    import { createHash } from "node:crypto";
    import { syncBuiltinESMExports } from "node:module";
    import zlib from "node:zlib";

    const nativeGzip = zlib.gzipSync;
    zlib.gzipSync = (...args) => {
      const compressed = nativeGzip(...args);
      compressed[9] = Number(process.argv[1]);
      return compressed;
    };
    syncBuiltinESMExports();
    const { buildPdfBundle } = await import("./scripts/pdf-bundle.mjs");
    const hash = bytes => createHash("sha256").update(bytes).digest("hex");
    const results = [];
    for (const [entry, name] of [
      ["src/mcp-apps/work-result/pdf-preview.mjs", "WebCodexPdf"],
      ["src/mcp-apps/work-result/pdf-document.mjs", "WebCodexPdfDocument"],
    ]) {
      const { script, assetCount } = await buildPdfBundle(entry, name);
      const compressed = [...script.matchAll(/"(H4sI[A-Za-z0-9+/=]+)"/g)]
        .map(match => Buffer.from(match[1], "base64"));
      assert.equal(compressed.length, assetCount + 1, "offline assets plus Worker");
      results.push({
        name, assetCount, scriptHash: hash(script),
        gzipOS: [...new Set(compressed.map(bytes => bytes[9]))],
        contentHashes: compressed.map(bytes => hash(zlib.gunzipSync(bytes))),
      });
    }
    process.stdout.write(JSON.stringify(results));
  `, String(osCode)], { cwd: frontendRoot, timeout: 30_000, maxBuffer: 64 * 1024 });
  return JSON.parse(stdout);
}

test("PDF bundles remain byte-identical across native gzip OS headers and preserve decompression", async () => {
  const unix = await buildWithNativeGzipOS(3);
  const windows = await buildWithNativeGzipOS(10);
  assert.deepEqual(windows, unix);
  for (const bundle of unix) assert.deepEqual(bundle.gzipOS, [255]);
});
