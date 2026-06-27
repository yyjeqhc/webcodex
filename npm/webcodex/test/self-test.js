"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const install = require("../install");
const wrapper = require("../bin/wrapper");

assert.strictEqual(install.platformKey("linux", "x64"), "linux-x64");
assert.throws(() => install.platformKey("sunos", "x64"), /Unsupported/);

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "webcodex-npm-test-"));
const artifact = path.join(tmp, "artifact");
fs.writeFileSync(artifact, "hello");
const hash = install.sha256File(artifact);
assert.strictEqual(install.verifySha256(artifact, hash), hash);
assert.throws(() => install.verifySha256(artifact, "0".repeat(64)), /Checksum mismatch/);

process.env.WEBCODEX_BINARY_DIR = tmp;
assert.strictEqual(wrapper.nativePath("webcodex"), path.join(tmp, wrapper.exeName("webcodex")));
delete process.env.WEBCODEX_BINARY_DIR;

fs.rmSync(tmp, { recursive: true, force: true });
console.log("npm wrapper self-test passed");
