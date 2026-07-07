"use strict";

const assert = require("assert");
const packageJson = require("../package.json");
const manifest = require("../manifest.json");

assert.strictEqual(manifest.version, packageJson.version);

for (const [platform, artifact] of Object.entries(manifest.artifacts || {})) {
  assert.match(
    artifact.url,
    new RegExp(`v${packageJson.version}/webcodex-v${packageJson.version}-${platform}\\.tar\\.gz$`)
  );
  assert.match(artifact.sha256, /^[a-f0-9]{64}$/);
}

console.log(`release manifest is publish-ready for ${packageJson.version}`);
