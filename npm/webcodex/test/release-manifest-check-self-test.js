"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const packageJson = require("../package.json");
const install = require("../install");
const {
  EXPECTED_BINARIES,
  SUPPORTED_PLATFORMS,
  expectedArtifactUrl,
  loadManifest,
  validateReleaseManifest
} = require("./release-manifest-check");

function validFixture(platforms = SUPPORTED_PLATFORMS) {
  return {
    version: packageJson.version,
    binaries: EXPECTED_BINARIES.slice(),
    artifacts: Object.fromEntries(
      platforms.map((platform, index) => [platform, {
        url: expectedArtifactUrl(packageJson.version, platform),
        sha256: String(index + 1).repeat(64)
      }])
    )
  };
}

function assertInvalid(manifest, pattern) {
  assert.throws(() => validateReleaseManifest(manifest), pattern);
}

function main() {
  const currentPath = path.join(__dirname, "..", "manifest.json");
  if (fs.existsSync(currentPath)) {
    const current = loadManifest(currentPath);
    install.validateManifest(current);
    assert.strictEqual(validateReleaseManifest(current), true);
  }

  const linuxOnly = validFixture(["linux-x64"]);
  assert.strictEqual(validateReleaseManifest(linuxOnly), true);
  assert.strictEqual(validateReleaseManifest(validFixture()), true);

  // win32-x64 is a producible release platform: its URL shape and a
  // placeholder-free checksum must validate like any other platform. The
  // published manifest only gains a real win32-x64 entry once a Windows-host
  // built artifact and its checksum exist.
  assert.strictEqual(
    expectedArtifactUrl(packageJson.version, "win32-x64"),
    `https://github.com/yyjeqhc/webcodex/releases/download/v${packageJson.version}/webcodex-v${packageJson.version}-win32-x64.tar.gz`
  );
  assert.strictEqual(validateReleaseManifest(validFixture(["win32-x64"])), true);

  assertInvalid({ version: packageJson.version, binaries: EXPECTED_BINARIES, artifacts: {} }, /at least one artifact/);

  const unknown = validFixture(["linux-x64"]);
  unknown.artifacts["freebsd-x64"] = {
    url: expectedArtifactUrl(packageJson.version, "freebsd-x64"),
    sha256: "a".repeat(64)
  };
  assertInvalid(unknown, /unknown platform freebsd-x64/);

  const missingChecksum = validFixture(["linux-x64"]);
  delete missingChecksum.artifacts["linux-x64"].sha256;
  assertInvalid(missingChecksum, /64 lowercase hexadecimal/);

  const placeholder = validFixture(["linux-x64"]);
  placeholder.artifacts["linux-x64"].sha256 = "REPLACE_WITH_RELEASE_ARTIFACT_SHA256";
  assertInvalid(placeholder, /placeholder|64 lowercase hexadecimal/);

  const zeroChecksum = validFixture(["linux-x64"]);
  zeroChecksum.artifacts["linux-x64"].sha256 = "0".repeat(64);
  assertInvalid(zeroChecksum, /must not be all zeroes/);

  const wrongVersion = validFixture(["linux-x64"]);
  wrongVersion.version = "9.9.9";
  assertInvalid(wrongVersion, /version must match package version/);

  const wrongUrlVersion = validFixture(["linux-x64"]);
  wrongUrlVersion.artifacts["linux-x64"].url = expectedArtifactUrl("9.9.9", "linux-x64");
  assertInvalid(wrongUrlVersion, /URL must match version and platform/);

  const wrongUrlPlatform = validFixture(["linux-x64"]);
  wrongUrlPlatform.artifacts["linux-x64"].url = expectedArtifactUrl(packageJson.version, "linux-arm64");
  assertInvalid(wrongUrlPlatform, /URL must match version and platform/);

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "webcodex-manifest-fixture-"));
  try {
    const fixturePath = path.join(tmp, "manifest.json");
    fs.writeFileSync(fixturePath, JSON.stringify(linuxOnly));
    assert.strictEqual(validateReleaseManifest(loadManifest(fixturePath)), true);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }

  console.log("release manifest supported/release platform self-test passed");
}

main();
