import assert from "node:assert/strict";
import test from "node:test";

import { inspectNpmPackage, parseNpmPackageSource } from "./dist/package-inspect.js";

test("package inspect parser accepts explicit npm sources and rejects non-npm sources", () => {
  assert.deepEqual(parseNpmPackageSource("npm:pi-mcp-adapter@1.2.3"), {
    source: "npm:pi-mcp-adapter@1.2.3",
    name: "pi-mcp-adapter",
    selector: "1.2.3",
  });
  assert.deepEqual(parseNpmPackageSource("npm:@scope/pkg@beta"), {
    source: "npm:@scope/pkg@beta",
    name: "@scope/pkg",
    selector: "beta",
  });
  assert.deepEqual(parseNpmPackageSource("npm:@scope/pkg"), {
    source: "npm:@scope/pkg",
    name: "@scope/pkg",
    selector: "latest",
  });
  assert.throws(() => parseNpmPackageSource("git:github.com/example/repo"), /only explicit npm:/u);
  assert.throws(() => parseNpmPackageSource("npm:https://evil.example/pkg"), /name is invalid/u);
});

test("package inspect returns bounded registry metadata without executing lifecycle scripts", async () => {
  const calls = [];
  const mockFetch = async (url, options) => {
    calls.push({ url, options });
    return new Response(JSON.stringify({
      name: "pi-mcp-adapter",
      version: "1.2.3",
      description: "Fixture package",
      license: "MIT",
      homepage: "https://example.invalid",
      repository: { url: "git+https://github.com/example/pi-mcp-adapter.git" },
      engines: { node: ">=22" },
      dependencies: { zeta: "2", alpha: "1" },
      peerDependencies: { peer: "^1" },
      optionalDependencies: { opt: "~3" },
      scripts: {
        test: "node test.js",
        preinstall: "node preinstall.js",
        postinstall: "node postinstall.js",
      },
      pi: { extensions: ["./extension.ts"], skills: ["./skills"] },
      dist: { tarball: "https://registry.npmjs.org/pi-mcp-adapter/-/pi-mcp-adapter-1.2.3.tgz" },
    }), { status: 200, headers: { "content-type": "application/json" } });
  };

  const result = await inspectNpmPackage("npm:pi-mcp-adapter@1.2.3", mockFetch);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, "https://registry.npmjs.org/pi-mcp-adapter/1.2.3");
  assert.equal(calls[0].options.method, "GET");
  assert.equal(calls[0].options.redirect, "error");
  assert.equal(result.resolvedVersion, "1.2.3");
  assert.equal(result.hasLifecycleScripts, true);
  assert.deepEqual(result.lifecycleScripts.map((entry) => entry.name), ["preinstall", "postinstall"]);
  assert.deepEqual(result.dependencies.map((entry) => entry.name), ["alpha", "zeta"]);
  assert.equal(result.distTarballHost, "registry.npmjs.org");
  assert.equal(result.sourceReviewComplete, false);
  assert.match(result.piManifestJson, /extensions/u);
  assert.match(result.notes.join(" "), /no tarball was downloaded/u);
});

test("package inspect bounds registry responses and keeps truncated Pi manifest preview valid JSON", async () => {
  const hugePi = { extensions: ["x".repeat(20 * 1024)] };
  const result = await inspectNpmPackage(
    "npm:bounded@1.0.0",
    async () => new Response(JSON.stringify({ name: "bounded", version: "1.0.0", pi: hugePi }), { status: 200 }),
  );
  assert.equal(result.piManifestTruncated, true);
  assert.doesNotThrow(() => JSON.parse(result.piManifestJson));
  assert.equal(JSON.parse(result.piManifestJson).truncated, true);

  const oversized = "x".repeat(1024 * 1024 + 1);
  await assert.rejects(
    inspectNpmPackage(
      "npm:oversized@1.0.0",
      async () => new Response(oversized, { status: 200 }),
    ),
    /metadata response exceeds/u,
  );
});

test("package inspect rejects registry errors and mismatched manifests", async () => {
  await assert.rejects(
    inspectNpmPackage("npm:missing", async () => new Response("no", { status: 404 })),
    /HTTP 404/u,
  );
  await assert.rejects(
    inspectNpmPackage(
      "npm:expected@1.0.0",
      async () => new Response(JSON.stringify({ name: "different", version: "1.0.0" }), { status: 200 }),
    ),
    /did not match/u,
  );
});
