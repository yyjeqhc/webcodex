"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");
const packageJson = require("../package.json");
const install = require("../install");

const EXPECTED_BINARIES = Object.freeze(["webcodex", "webcodex-server", "webcodex-runner"]);
const SUPPORTED_PLATFORMS = Object.freeze(install.SUPPORTED_PLATFORM_KEYS.slice());
const SHA256_RE = /^[a-f0-9]{64}$/;

function expectedArtifactUrl(version, platform) {
  return `https://github.com/yyjeqhc/webcodex/releases/download/v${version}/webcodex-v${version}-${platform}.tar.gz`;
}

function validateReleaseManifest(manifest, version = packageJson.version) {
  assert.ok(install.isPlainObject(manifest), "release manifest must be a plain object");
  assert.strictEqual(manifest.version, version, "release manifest version must match package version");
  assert.deepStrictEqual(manifest.binaries, EXPECTED_BINARIES, "release manifest binaries must be canonical");
  assert.ok(install.isPlainObject(manifest.artifacts), "release manifest artifacts must be a plain object");

  const platforms = Object.keys(manifest.artifacts);
  assert.ok(platforms.length > 0, "release manifest must contain at least one artifact");
  for (const platform of platforms) {
    assert.ok(SUPPORTED_PLATFORMS.includes(platform), `release manifest contains unknown platform ${platform}`);
  }

  for (const platform of platforms) {
    const artifact = manifest.artifacts[platform];
    assert.ok(install.isPlainObject(artifact), `release manifest artifact ${platform} must be a plain object`);
    assert.strictEqual(artifact.url, expectedArtifactUrl(version, platform), `release manifest URL must match version and platform for ${platform}`);
    assert.notStrictEqual(artifact.sha256, "REPLACE_WITH_RELEASE_ARTIFACT_SHA256", `release manifest checksum for ${platform} must not be a placeholder`);
    assert.match(artifact.sha256 || "", SHA256_RE, `release manifest checksum for ${platform} must be 64 lowercase hexadecimal characters`);
    assert.notStrictEqual(artifact.sha256, "0".repeat(64), `release manifest checksum for ${platform} must not be all zeroes`);
  }
  return true;
}

function loadManifest(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function main() {
  const args = process.argv.slice(2);
  const quiet = args.includes("--quiet");
  const requested = args.find((arg) => arg !== "--quiet") || process.env.WEBCODEX_RELEASE_MANIFEST;
  const manifestPath = requested
    ? path.resolve(requested)
    : path.join(__dirname, "..", "manifest.json");
  if (!fs.existsSync(manifestPath)) {
    throw new Error(
      `publish-ready release manifest not found at ${manifestPath}; ` +
      "generate it on OE from the native release artifacts and stage the npm package before publishing"
    );
  }
  validateReleaseManifest(loadManifest(manifestPath));
  if (!quiet) console.log(`release manifest is publish-ready for ${packageJson.version}`);
}

if (require.main === module) main();

module.exports = {
  EXPECTED_BINARIES,
  SUPPORTED_PLATFORMS,
  expectedArtifactUrl,
  loadManifest,
  validateReleaseManifest
};
