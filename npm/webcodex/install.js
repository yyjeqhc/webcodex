#!/usr/bin/env node
"use strict";

const crypto = require("crypto");
const fs = require("fs");
const http = require("http");
const https = require("https");
const os = require("os");
const path = require("path");
const { URL } = require("url");
const zlib = require("zlib");

const ROOT = __dirname;
const VENDOR_BIN = path.join(ROOT, "vendor", "bin");
const DEFAULT_MANIFEST = path.join(ROOT, "manifest.example.json");

function platformKey(platform = process.platform, arch = process.arch) {
  const supported = new Set([
    "linux-x64",
    "linux-arm64",
    "darwin-x64",
    "darwin-arm64",
    "win32-x64"
  ]);
  const key = `${platform}-${arch}`;
  if (!supported.has(key)) {
    throw new Error(`Unsupported platform/arch: ${key}`);
  }
  return key;
}

function sha256File(file) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(file));
  return hash.digest("hex");
}

function verifySha256(file, expected) {
  const actual = sha256File(file);
  if (actual !== expected) {
    throw new Error(`Checksum mismatch for ${path.basename(file)}: expected ${expected}, got ${actual}`);
  }
  return actual;
}

function readJsonFile(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function fetchToFile(urlString, dest) {
  const url = new URL(urlString);
  if (url.protocol === "file:") {
    fs.copyFileSync(url, dest);
    return Promise.resolve();
  }
  const client = url.protocol === "https:" ? https : url.protocol === "http:" ? http : null;
  if (!client) {
    return Promise.reject(new Error(`Unsupported artifact URL protocol: ${url.protocol}`));
  }
  return new Promise((resolve, reject) => {
    const req = client.get(url, (res) => {
      if (res.statusCode < 200 || res.statusCode >= 300) {
        reject(new Error(`Download failed with HTTP ${res.statusCode}`));
        res.resume();
        return;
      }
      const out = fs.createWriteStream(dest, { mode: 0o600 });
      res.pipe(out);
      out.on("finish", () => out.close(resolve));
      out.on("error", reject);
    });
    req.on("error", reject);
  });
}

function copyLocalBinaryDir(srcDir) {
  fs.mkdirSync(VENDOR_BIN, { recursive: true });
  for (const name of ["webcodex", "webcodex-agent", "webcodex-cli"]) {
    const exe = process.platform === "win32" ? `${name}.exe` : name;
    const src = path.join(srcDir, exe);
    const dest = path.join(VENDOR_BIN, exe);
    if (!fs.existsSync(src)) {
      throw new Error(`WEBCODEX_BINARY_DIR missing ${exe}`);
    }
    fs.copyFileSync(src, dest);
    if (process.platform !== "win32") {
      fs.chmodSync(dest, 0o755);
    }
  }
}

function extractTarGz(archive, destDir) {
  const tmpTar = path.join(os.tmpdir(), `webcodex-${Date.now()}-${process.pid}.tar`);
  fs.writeFileSync(tmpTar, zlib.gunzipSync(fs.readFileSync(archive)));
  const data = fs.readFileSync(tmpTar);
  fs.unlinkSync(tmpTar);
  let offset = 0;
  fs.mkdirSync(destDir, { recursive: true });
  while (offset + 512 <= data.length) {
    const header = data.subarray(offset, offset + 512);
    offset += 512;
    if (header.every((b) => b === 0)) break;
    const name = header.subarray(0, 100).toString("utf8").replace(/\0.*$/, "");
    const sizeOctal = header.subarray(124, 136).toString("utf8").replace(/\0.*$/, "").trim();
    const type = header[156];
    const size = parseInt(sizeOctal || "0", 8);
    const content = data.subarray(offset, offset + size);
    offset += Math.ceil(size / 512) * 512;
    if (!name || type === 53) continue;
    const base = path.basename(name);
    if (!["webcodex", "webcodex-agent", "webcodex-cli", "webcodex.exe", "webcodex-agent.exe", "webcodex-cli.exe"].includes(base)) {
      continue;
    }
    const out = path.join(destDir, base);
    fs.writeFileSync(out, content, { mode: 0o755 });
  }
}

async function installFromManifest(manifestPathOrUrl) {
  let manifestPath = manifestPathOrUrl || process.env.WEBCODEX_MANIFEST || DEFAULT_MANIFEST;
  let cleanupManifest = null;
  if (/^https?:/.test(manifestPath)) {
    cleanupManifest = path.join(os.tmpdir(), `webcodex-manifest-${Date.now()}.json`);
    await fetchToFile(manifestPath, cleanupManifest);
    manifestPath = cleanupManifest;
  } else if (/^file:/.test(manifestPath)) {
    manifestPath = new URL(manifestPath);
  }
  const manifest = typeof manifestPath === "string" ? readJsonFile(manifestPath) : JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  if (cleanupManifest) fs.unlinkSync(cleanupManifest);
  const key = platformKey();
  const artifact = manifest.artifacts && manifest.artifacts[key];
  if (!artifact) {
    throw new Error(`No WebCodex artifact for ${key} in manifest`);
  }
  if (!artifact.url || !artifact.sha256) {
    throw new Error(`Manifest artifact ${key} must include url and sha256`);
  }
  const tmp = path.join(os.tmpdir(), `webcodex-${key}-${Date.now()}`);
  await fetchToFile(artifact.url, tmp);
  verifySha256(tmp, artifact.sha256);
  fs.rmSync(VENDOR_BIN, { recursive: true, force: true });
  fs.mkdirSync(VENDOR_BIN, { recursive: true });
  if (artifact.url.endsWith(".tar.gz") || artifact.url.endsWith(".tgz")) {
    extractTarGz(tmp, VENDOR_BIN);
  } else {
    throw new Error("Only .tar.gz/.tgz artifacts are supported by the MVP installer");
  }
  fs.unlinkSync(tmp);
}

async function main() {
  if (process.env.WEBCODEX_SKIP_DOWNLOAD === "1") {
    console.log("WEBCODEX_SKIP_DOWNLOAD=1 set; skipping native binary download.");
    return;
  }
  if (process.env.WEBCODEX_BINARY_DIR) {
    copyLocalBinaryDir(process.env.WEBCODEX_BINARY_DIR);
    return;
  }
  await installFromManifest();
}

if (require.main === module) {
  main().catch((err) => {
    console.error(`WebCodex install failed: ${err.message}`);
    process.exit(1);
  });
}

module.exports = { platformKey, sha256File, verifySha256, installFromManifest, copyLocalBinaryDir };
