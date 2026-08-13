#!/usr/bin/env node
"use strict";

const crypto = require("crypto");
const childProcess = require("child_process");
const fs = require("fs");
const http = require("http");
const https = require("https");
const os = require("os");
const path = require("path");
const { URL, fileURLToPath, pathToFileURL } = require("url");
const zlib = require("zlib");

const ROOT = __dirname;
const VENDOR_DIR = path.join(ROOT, "vendor");
const VENDOR_BIN = path.join(VENDOR_DIR, "bin");
const RELEASE_MANIFEST = path.join(ROOT, "manifest.json");
const EXAMPLE_MANIFEST = path.join(ROOT, "manifest.example.json");
const DEFAULT_MANIFEST = fs.existsSync(RELEASE_MANIFEST) ? RELEASE_MANIFEST : EXAMPLE_MANIFEST;
const PACKAGE_VERSION = require("./package.json").version;
const RUNTIME_BINARIES = Object.freeze(["webcodex", "webcodex-server", "webcodex-runner"]);
const PLATFORM_KEYS = Object.freeze({
  linux: Object.freeze({ x64: "linux-x64", arm64: "linux-arm64" }),
  darwin: Object.freeze({ x64: "darwin-x64", arm64: "darwin-arm64" }),
  win32: Object.freeze({ x64: "win32-x64", arm64: "win32-arm64" })
});
const SUPPORTED_PLATFORM_KEYS = Object.freeze(
  Object.values(PLATFORM_KEYS).flatMap((architectures) => Object.values(architectures))
);

const MAX_MANIFEST_BYTES = 1024 * 1024;
const MAX_ARTIFACT_BYTES = 128 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES = 256 * 1024 * 1024;
const MAX_TAR_ENTRY_BYTES = 96 * 1024 * 1024;
const MAX_REDIRECTS = 5;

const DEFAULT_MANIFEST_DOWNLOAD = Object.freeze({
  firstByteTimeoutMs: 15_000,
  inactivityTimeoutMs: 15_000,
  totalTimeoutMs: 30_000
});
const DEFAULT_ARTIFACT_DOWNLOAD = Object.freeze({
  firstByteTimeoutMs: 30_000,
  inactivityTimeoutMs: 30_000,
  totalTimeoutMs: 120_000
});

function platformKey(platform = process.platform, arch = process.arch) {
  const key = PLATFORM_KEYS[platform] && PLATFORM_KEYS[platform][arch];
  if (!key) {
    throw new Error(`Unsupported platform/architecture: ${platform}-${arch}`);
  }
  return key;
}

function exeName(name, platform = process.platform) {
  return platform === "win32" ? `${name}.exe` : name;
}

function runtimeBinaryFiles(platform = process.platform) {
  return RUNTIME_BINARIES.map((name) => exeName(name, platform));
}

function positiveLimit(value, fallback, label) {
  const resolved = value === undefined ? fallback : value;
  if (!Number.isSafeInteger(resolved) || resolved <= 0) {
    throw new Error(`${label} must be a positive safe integer`);
  }
  return resolved;
}

function resolveLimits(options = {}) {
  const overrides = options.limits || {};
  return {
    maxManifestBytes: positiveLimit(overrides.maxManifestBytes, MAX_MANIFEST_BYTES, "maxManifestBytes"),
    maxArtifactBytes: positiveLimit(overrides.maxArtifactBytes, MAX_ARTIFACT_BYTES, "maxArtifactBytes"),
    maxUncompressedBytes: positiveLimit(overrides.maxUncompressedBytes, MAX_UNCOMPRESSED_BYTES, "maxUncompressedBytes"),
    maxTarEntryBytes: positiveLimit(overrides.maxTarEntryBytes, MAX_TAR_ENTRY_BYTES, "maxTarEntryBytes")
  };
}

function resolveDownloadOptions(defaults, overrides, maxBytes, label) {
  const values = { ...defaults, ...(overrides || {}) };
  return {
    firstByteTimeoutMs: positiveLimit(values.firstByteTimeoutMs, defaults.firstByteTimeoutMs, `${label} first-byte timeout`),
    inactivityTimeoutMs: positiveLimit(values.inactivityTimeoutMs, defaults.inactivityTimeoutMs, `${label} inactivity timeout`),
    totalTimeoutMs: positiveLimit(values.totalTimeoutMs, defaults.totalTimeoutMs, `${label} total timeout`),
    maxBytes,
    label
  };
}

function sha256File(file) {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(file));
  return hash.digest("hex");
}

function verifySha256(file, expected) {
  if (!/^[a-f0-9]{64}$/.test(expected || "")) {
    throw new Error("Artifact checksum is missing or invalid");
  }
  const actual = sha256File(file);
  if (actual !== expected) {
    throw new Error(`Artifact checksum mismatch: expected ${expected}, got ${actual}`);
  }
  return actual;
}

function readJsonFileBounded(file, maxBytes = MAX_MANIFEST_BYTES) {
  const target = file instanceof URL ? fileURLToPath(file) : file;
  const stat = fs.statSync(target);
  if (!stat.isFile()) {
    throw new Error("Release manifest is not a regular file");
  }
  if (stat.size > maxBytes) {
    throw new Error(`Release manifest exceeds the ${maxBytes}-byte size limit`);
  }
  return JSON.parse(fs.readFileSync(target, "utf8"));
}

function copyFileUrlTo(url, dest, options) {
  const source = new URL(url);
  if (source.protocol !== "file:") {
    throw new Error(`Unsupported download protocol: ${source.protocol}`);
  }
  const sourcePath = fileURLToPath(source);
  const stat = fs.statSync(sourcePath);
  if (!stat.isFile()) {
    throw new Error(`${options.label} source is not a regular file`);
  }
  if (stat.size > options.maxBytes) {
    throw new Error(`${options.label} exceeds the ${options.maxBytes}-byte download limit`);
  }
  try {
    fs.copyFileSync(sourcePath, dest, fs.constants.COPYFILE_EXCL);
  } catch (err) {
    fs.rmSync(dest, { force: true });
    throw err;
  }
  return Promise.resolve();
}

function resolveRedirectUrl(currentUrl, location) {
  const current = currentUrl instanceof URL ? currentUrl : new URL(currentUrl);
  let next;
  try {
    next = new URL(location, current);
  } catch (_err) {
    throw new Error("Download redirect location is invalid");
  }
  if (next.protocol !== "http:" && next.protocol !== "https:") {
    throw new Error("Download redirect uses an unsupported protocol");
  }
  if (current.protocol === "https:" && next.protocol !== "https:") {
    throw new Error("Download redirect refused an HTTPS downgrade");
  }
  return next;
}

function fetchToFile(url, dest, options = {}, redirects = 0, deadlineAt = null) {
  const parsed = new URL(url);
  const resolved = {
    firstByteTimeoutMs: positiveLimit(options.firstByteTimeoutMs, 15_000, "first-byte timeout"),
    inactivityTimeoutMs: positiveLimit(options.inactivityTimeoutMs, 15_000, "inactivity timeout"),
    totalTimeoutMs: positiveLimit(options.totalTimeoutMs, 30_000, "total timeout"),
    maxBytes: positiveLimit(options.maxBytes, MAX_ARTIFACT_BYTES, "download byte limit"),
    label: options.label || "Download"
  };

  if (parsed.protocol === "file:") {
    return copyFileUrlTo(parsed, dest, resolved);
  }
  if (!['http:', 'https:'].includes(parsed.protocol)) {
    return Promise.reject(new Error(`Unsupported download protocol: ${parsed.protocol}`));
  }
  const sharedDeadline = deadlineAt || Date.now() + resolved.totalTimeoutMs;
  const remainingTotal = sharedDeadline - Date.now();
  if (remainingTotal <= 0) {
    return Promise.reject(new Error(`${resolved.label} download exceeded its total timeout`));
  }

  return new Promise((resolve, reject) => {
    const client = parsed.protocol === "https:" ? https : http;
    let response = null;
    let file = null;
    let settled = false;
    let received = 0;
    let inactivityTimer = null;

    const clearTimers = () => {
      clearTimeout(firstByteTimer);
      clearTimeout(totalTimer);
      clearTimeout(inactivityTimer);
    };
    const removePartial = () => {
      try {
        fs.rmSync(dest, { force: true });
      } catch (_err) {
        // Keep the original bounded download error.
      }
    };
    const fail = (message) => {
      if (settled) return;
      settled = true;
      clearTimers();
      if (response) response.destroy();
      if (file) file.destroy();
      request.destroy();
      removePartial();
      reject(message instanceof Error ? message : new Error(message));
    };
    const succeed = () => {
      if (settled) return;
      settled = true;
      clearTimers();
      resolve();
    };
    const resetInactivityTimer = () => {
      clearTimeout(inactivityTimer);
      inactivityTimer = setTimeout(
        () => fail(`${resolved.label} download stalled before completion`),
        resolved.inactivityTimeoutMs
      );
    };

    const totalTimer = setTimeout(
      () => fail(`${resolved.label} download exceeded its total timeout`),
      remainingTotal
    );
    const firstByteTimer = resolved.firstByteTimeoutMs < remainingTotal
      ? setTimeout(
        () => fail(`${resolved.label} download timed out waiting for a response`),
        resolved.firstByteTimeoutMs
      )
      : null;

    const request = client.get(parsed, (res) => {
      response = res;
      clearTimeout(firstByteTimer);
      const status = res.statusCode || 0;

      if ([301, 302, 303, 307, 308].includes(status)) {
        const location = res.headers.location;
        if (!location) {
          fail(`${resolved.label} redirect did not include a location`);
          return;
        }
        if (redirects >= MAX_REDIRECTS) {
          fail(`${resolved.label} exceeded the redirect limit`);
          return;
        }

        let nextUrl;
        try {
          nextUrl = resolveRedirectUrl(parsed, location);
        } catch (err) {
          fail(err);
          return;
        }

        settled = true;
        clearTimers();
        res.destroy();
        request.destroy();
        fetchToFile(nextUrl, dest, resolved, redirects + 1, sharedDeadline).then(resolve, reject);
        return;
      }

      if (status !== 200) {
        res.resume();
        fail(`${resolved.label} download failed with HTTP ${status}`);
        return;
      }

      const contentLength = res.headers["content-length"];
      if (contentLength !== undefined) {
        const declared = Number(contentLength);
        if (Number.isFinite(declared) && declared > resolved.maxBytes) {
          fail(`${resolved.label} exceeds the ${resolved.maxBytes}-byte download limit`);
          return;
        }
      }

      try {
        file = fs.createWriteStream(dest, { flags: "wx", mode: 0o600 });
      } catch (err) {
        fail(err);
        return;
      }
      resetInactivityTimer();

      res.on("data", (chunk) => {
        if (settled) return;
        received += chunk.length;
        if (!Number.isSafeInteger(received) || received > resolved.maxBytes) {
          fail(`${resolved.label} exceeds the ${resolved.maxBytes}-byte download limit`);
          return;
        }
        resetInactivityTimer();
        if (!file.write(chunk)) res.pause();
      });
      file.on("drain", () => {
        if (!settled) res.resume();
      });
      res.on("end", () => {
        if (!settled) file.end();
      });
      res.on("aborted", () => fail(`${resolved.label} download ended before completion`));
      res.on("error", () => fail(`${resolved.label} download failed while reading the response`));
      file.on("error", () => fail(`${resolved.label} download failed while writing the temporary file`));
      file.on("finish", () => file.close(succeed));
    });
    request.on("error", () => fail(`${resolved.label} download request failed`));
  });
}

function parseTarOctal(field, label) {
  const value = field.toString("ascii").replace(/\0.*$/, "").trim();
  if (value === "") return 0;
  if (!/^[0-7]+$/.test(value)) {
    throw new Error(`Invalid tar ${label}`);
  }
  const parsed = Number.parseInt(value, 8);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error(`Invalid tar ${label}`);
  }
  return parsed;
}

function extractTarGz(archive, destDir, options = {}) {
  const limits = resolveLimits(options);
  const platform = options.platform || process.platform;
  const expected = new Set(runtimeBinaryFiles(platform));
  const found = new Set();
  const archiveStat = fs.statSync(archive);
  if (!archiveStat.isFile()) {
    throw new Error("Release artifact is not a regular file");
  }
  if (archiveStat.size > limits.maxArtifactBytes) {
    throw new Error(`Release artifact exceeds the ${limits.maxArtifactBytes}-byte compressed size limit`);
  }

  const compressed = fs.readFileSync(archive);
  let data;
  try {
    data = zlib.gunzipSync(compressed, { maxOutputLength: limits.maxUncompressedBytes + 1 });
  } catch (err) {
    if (err && err.code === "ERR_BUFFER_TOO_LARGE") {
      throw new Error(`Release artifact exceeds the ${limits.maxUncompressedBytes}-byte uncompressed size limit`);
    }
    throw new Error("Release artifact is not a valid bounded gzip archive");
  }
  if (data.length > limits.maxUncompressedBytes) {
    throw new Error(`Release artifact exceeds the ${limits.maxUncompressedBytes}-byte uncompressed size limit`);
  }

  fs.mkdirSync(destDir, { recursive: true });
  let offset = 0;
  let expectedContentBytes = 0;
  while (offset + 512 <= data.length) {
    const header = data.subarray(offset, offset + 512);
    offset += 512;
    if (header.every((byte) => byte === 0)) break;

    const name = header.subarray(0, 100).toString("utf8").replace(/\0.*$/, "");
    const size = parseTarOctal(header.subarray(124, 136), "entry size");
    const type = String.fromCharCode(header[156] || 48);
    if (size > limits.maxTarEntryBytes) {
      throw new Error(`Release artifact tar entry exceeds the ${limits.maxTarEntryBytes}-byte limit`);
    }
    const paddedSize = Math.ceil(size / 512) * 512;
    if (!Number.isSafeInteger(paddedSize) || paddedSize < size) {
      throw new Error("Release artifact contains an invalid tar entry size");
    }
    if (offset > data.length || paddedSize > data.length - offset || size > data.length - offset) {
      throw new Error("Release artifact contains a truncated tar entry");
    }
    const content = data.subarray(offset, offset + size);
    offset += paddedSize;

    if (type === "5") continue;
    if (type !== "0" && type !== "\0") continue;
    const base = path.basename(name);
    if (!expected.has(base)) continue;
    if (found.has(base)) {
      throw new Error(`Release artifact contains duplicate ${base}`);
    }
    expectedContentBytes += size;
    if (!Number.isSafeInteger(expectedContentBytes) || expectedContentBytes > limits.maxUncompressedBytes) {
      throw new Error("Release artifact runtime binary content exceeds the installation limit");
    }
    fs.writeFileSync(path.join(destDir, base), content, { mode: 0o755, flag: "wx" });
    found.add(base);
  }
}

function versionIdentity(binary, name, expectedVersion, options = {}) {
  // Narrow, test-only seam: a caller-provided identity provider lets tests
  // run on platforms without a C toolchain (Windows CI has no compiler for
  // fake PE fixtures). Production never passes one, so the spawn-based
  // validation below is the only path in real installs; the Windows
  // real-binary smoke covers this function against actual build output.
  if (options.versionIdentity) {
    return options.versionIdentity(binary, name, expectedVersion);
  }
  const result = childProcess.spawnSync(binary, ["--version"], {
    encoding: "utf8",
    windowsHide: true,
    shell: false,
    timeout: 5000,
    maxBuffer: 64 * 1024
  });
  if (result.error || result.signal || result.status !== 0) {
    throw new Error(`${name} failed its installation version check`);
  }
  const line = String(result.stdout || "").trim().split(/\r?\n/, 1)[0];
  const prefix = `${name} `;
  if (!line.startsWith(prefix)) {
    throw new Error(`${name} returned an unexpected version string`);
  }
  const identity = line.slice(prefix.length);
  if (identity !== expectedVersion && !identity.startsWith(`${expectedVersion} `)) {
    throw new Error(`${name} version does not match package ${expectedVersion}`);
  }
  return identity;
}

function validateBinarySet(dir, expectedVersion = PACKAGE_VERSION, platform = process.platform, options = {}) {
  const identities = [];
  for (const name of RUNTIME_BINARIES) {
    const file = path.join(dir, exeName(name, platform));
    let stat;
    try {
      stat = fs.lstatSync(file);
    } catch (_err) {
      throw new Error(`Release artifact is missing ${exeName(name, platform)}`);
    }
    if (!stat.isFile()) {
      throw new Error(`${exeName(name, platform)} is not a regular file`);
    }
    if (platform !== "win32") {
      fs.chmodSync(file, 0o755);
    }
    identities.push(versionIdentity(file, name, expectedVersion, options));
  }
  if (!identities.every((identity) => identity === identities[0])) {
    throw new Error("Installed WebCodex binaries are not from the same build");
  }
  return identities[0];
}

function defaultWarning(message) {
  console.warn(message);
}

function replaceDirectoryAtomically(stagedDir, finalDir, onWarning = defaultWarning) {
  const parent = path.dirname(finalDir);
  const backup = path.join(parent, `.bin-backup-${process.pid}-${Date.now()}`);
  const hadPrevious = fs.existsSync(finalDir);
  if (hadPrevious) {
    fs.renameSync(finalDir, backup);
  }
  try {
    fs.renameSync(stagedDir, finalDir);
  } catch (err) {
    if (hadPrevious && fs.existsSync(backup) && !fs.existsSync(finalDir)) {
      fs.renameSync(backup, finalDir);
    }
    throw err;
  }
  if (hadPrevious) {
    try {
      fs.rmSync(backup, { recursive: true, force: true });
    } catch (_err) {
      onWarning("WebCodex installation succeeded, but the previous binary backup could not be removed.");
    }
  }
}

function installBinarySet(populate, options = {}) {
  const destinationDir = options.destinationDir || VENDOR_BIN;
  const expectedVersion = options.expectedVersion || PACKAGE_VERSION;
  const platform = options.platform || process.platform;
  const parent = path.dirname(destinationDir);
  fs.mkdirSync(parent, { recursive: true });
  const stagedDir = fs.mkdtempSync(path.join(parent, ".bin-staging-"));
  try {
    populate(stagedDir);
    const identity = validateBinarySet(stagedDir, expectedVersion, platform, options);
    replaceDirectoryAtomically(stagedDir, destinationDir, options.onWarning);
    return identity;
  } finally {
    fs.rmSync(stagedDir, { recursive: true, force: true });
  }
}

function copyLocalBinaryDir(srcDir, options = {}) {
  const platform = options.platform || process.platform;
  return installBinarySet((stagedDir) => {
    for (const file of runtimeBinaryFiles(platform)) {
      const source = path.join(srcDir, file);
      if (!fs.existsSync(source)) {
        throw new Error(`WEBCODEX_BINARY_DIR is missing ${file}`);
      }
      fs.copyFileSync(source, path.join(stagedDir, file), fs.constants.COPYFILE_EXCL);
    }
  }, { ...options, platform });
}

function isPlainObject(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function validateManifest(manifest) {
  if (!isPlainObject(manifest) || manifest.version !== PACKAGE_VERSION) {
    throw new Error(`Release manifest version must match package ${PACKAGE_VERSION}`);
  }
  if (!Array.isArray(manifest.binaries) ||
      manifest.binaries.length !== RUNTIME_BINARIES.length ||
      !RUNTIME_BINARIES.every((name, index) => manifest.binaries[index] === name)) {
    throw new Error("Release manifest must declare the three WebCodex runtime binaries in canonical order");
  }
  if (!isPlainObject(manifest.artifacts)) {
    throw new Error("Release manifest artifacts must be a plain object");
  }
}

function makeTempDirectory(parent, prefix) {
  fs.mkdirSync(parent, { recursive: true });
  return fs.mkdtempSync(path.join(parent, prefix));
}

async function loadManifest(manifestPathOrUrl, options = {}) {
  const input = manifestPathOrUrl || process.env.WEBCODEX_MANIFEST || DEFAULT_MANIFEST;
  const limits = resolveLimits(options);
  if (/^https?:/i.test(input)) {
    const tempDir = makeTempDirectory(options.tempDir || os.tmpdir(), "webcodex-manifest-");
    const tmp = path.join(tempDir, "manifest.json");
    try {
      const downloadOptions = resolveDownloadOptions(
        DEFAULT_MANIFEST_DOWNLOAD,
        options.manifestDownload,
        limits.maxManifestBytes,
        "Manifest"
      );
      await fetchToFile(input, tmp, downloadOptions);
      return { manifest: readJsonFileBounded(tmp, limits.maxManifestBytes), baseUrl: new URL(input) };
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  }
  const manifestUrl = /^file:/i.test(input)
    ? new URL(input)
    : pathToFileURL(path.resolve(String(input)));
  return { manifest: readJsonFileBounded(manifestUrl, limits.maxManifestBytes), baseUrl: manifestUrl };
}

async function installFromManifest(manifestPathOrUrl, options = {}) {
  const platform = options.platform || process.platform;
  const arch = options.arch || process.arch;
  const limits = resolveLimits(options);
  const { manifest, baseUrl } = await loadManifest(manifestPathOrUrl, options);
  validateManifest(manifest);
  const key = platformKey(platform, arch);
  const artifact = manifest.artifacts[key];
  if (!artifact) {
    throw new Error(`No WebCodex release artifact is available for ${key}`);
  }
  if (!isPlainObject(artifact) || !artifact.url || !artifact.sha256) {
    throw new Error(`Release manifest entry ${key} must include url and sha256`);
  }
  const artifactUrl = new URL(artifact.url, baseUrl);
  if (!/\.(?:tar\.gz|tgz)$/i.test(artifactUrl.pathname)) {
    throw new Error("Only .tar.gz/.tgz release artifacts are supported");
  }

  const tempDir = makeTempDirectory(options.tempDir || os.tmpdir(), "webcodex-artifact-");
  const tmp = path.join(tempDir, "artifact.tar.gz");
  try {
    const downloadOptions = resolveDownloadOptions(
      DEFAULT_ARTIFACT_DOWNLOAD,
      options.artifactDownload,
      limits.maxArtifactBytes,
      "Artifact"
    );
    await fetchToFile(artifactUrl, tmp, downloadOptions);
    verifySha256(tmp, artifact.sha256);
    return installBinarySet(
      (stagedDir) => extractTarGz(tmp, stagedDir, { ...options, platform, limits }),
      { ...options, platform, expectedVersion: manifest.version }
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
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

module.exports = {
  DEFAULT_ARTIFACT_DOWNLOAD,
  DEFAULT_MANIFEST_DOWNLOAD,
  MAX_ARTIFACT_BYTES,
  MAX_MANIFEST_BYTES,
  MAX_REDIRECTS,
  MAX_TAR_ENTRY_BYTES,
  MAX_UNCOMPRESSED_BYTES,
  PACKAGE_VERSION,
  PLATFORM_KEYS,
  RUNTIME_BINARIES,
  SUPPORTED_PLATFORM_KEYS,
  VENDOR_BIN,
  copyLocalBinaryDir,
  exeName,
  extractTarGz,
  fetchToFile,
  installBinarySet,
  installFromManifest,
  isPlainObject,
  loadManifest,
  platformKey,
  readJsonFileBounded,
  resolveLimits,
  resolveRedirectUrl,
  runtimeBinaryFiles,
  sha256File,
  validateBinarySet,
  validateManifest,
  verifySha256
};
