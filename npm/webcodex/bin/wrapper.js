"use strict";

const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");

const NPM_NETWORK_KEYS = [
  "npm_config_https_proxy",
  "npm_config_proxy",
  "npm_config_noproxy",
  "npm_config_no_proxy",
  "npm_config_cafile",
  "npm_config_ca",
  "npm_config_strict_ssl"
];
const NPM_NETWORK_CAPTURE_BYTES = 4 * 1024 * 1024;
const NPM_NETWORK_QUERY_TIMEOUT_MS = 5000;

function exeName(name, platform = process.platform) {
  return platform === "win32" ? `${name}.exe` : name;
}

function packageRoot() {
  return path.resolve(__dirname, "..");
}

function nativePath(options = {}) {
  const platform = options.platform || process.platform;
  const pathApi = platform === "win32" ? path.win32 : path.posix;
  const root = options.packageRoot || packageRoot();
  return pathApi.join(root, "vendor", "bin", exeName("webcodex", platform));
}

function normalizedNpmConfigValue(value) {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  if (!trimmed || /^(?:null|undefined|\[\])$/i.test(trimmed)) return undefined;
  return value;
}

function npmProgram(options = {}) {
  if (options.npmProgram) return options.npmProgram;
  const platform = options.platform || process.platform;
  return platform === "win32" ? "npm.cmd" : "npm";
}

function captureProtectedNpmEnvironment(environment, options = {}) {
  const root = options.packageRoot || packageRoot();
  const helper = options.networkHelper || path.join(root, "bin", "npm-network-env.js");
  if (!fs.existsSync(helper)) return {};
  const result = childProcess.spawnSync(
    npmProgram(options),
    ["exec", "--yes=false", "--", process.execPath, helper],
    {
      cwd: root,
      env: environment,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
      timeout: NPM_NETWORK_QUERY_TIMEOUT_MS,
      maxBuffer: NPM_NETWORK_CAPTURE_BYTES,
      windowsHide: true,
      shell: false
    }
  );
  if (result.error || result.signal || result.status !== 0 || typeof result.stdout !== "string") return {};
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (_err) {
    return {};
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
  const captured = {};
  let totalBytes = 0;
  for (const key of NPM_NETWORK_KEYS) {
    const value = normalizedNpmConfigValue(parsed[key]);
    if (value === undefined) continue;
    totalBytes += Buffer.byteLength(key) + Buffer.byteLength(value);
    if (totalBytes > NPM_NETWORK_CAPTURE_BYTES) return {};
    captured[key] = value;
  }
  return captured;
}

function queryNpmConfigValue(key, environment, options = {}) {
  const result = childProcess.spawnSync(npmProgram(options), ["config", "get", key], {
    cwd: options.packageRoot || packageRoot(),
    env: environment,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
    timeout: NPM_NETWORK_QUERY_TIMEOUT_MS,
    maxBuffer: NPM_NETWORK_CAPTURE_BYTES,
    windowsHide: true,
    shell: false
  });
  if (result.error || result.signal || result.status !== 0) return undefined;
  return normalizedNpmConfigValue(result.stdout);
}

function rehydrateNpmNetworkEnvironment(environment = process.env, options = {}) {
  const hydrated = { ...environment };
  const captured = captureProtectedNpmEnvironment(environment, options);
  for (const key of NPM_NETWORK_KEYS) {
    if (normalizedNpmConfigValue(hydrated[key]) === undefined && captured[key] !== undefined) {
      hydrated[key] = captured[key];
    }
  }
  for (const [key, configKey] of [["npm_config_ca", "ca"], ["npm_config_strict_ssl", "strict-ssl"]]) {
    if (normalizedNpmConfigValue(hydrated[key]) !== undefined) continue;
    const value = queryNpmConfigValue(configKey, environment, options);
    if (value !== undefined) hydrated[key] = value;
  }
  return hydrated;
}

function needsNpmNetworkContext(argv, needsBootstrap) {
  return needsBootstrap || argv.length === 0 || argv[0] === "share";
}

function bootstrapNative(options = {}) {
  const root = options.packageRoot || packageRoot();
  const installScript = options.installScript || path.join(root, "install.js");
  if (!fs.existsSync(installScript)) {
    console.error("WebCodex installation is incomplete: install.js is missing. Reinstall the npm package.");
    return false;
  }
  const result = childProcess.spawnSync(process.execPath, [installScript], {
    cwd: root,
    env: options.environment || process.env,
    stdio: "inherit",
    windowsHide: false,
    shell: false
  });
  if (result.error) {
    console.error(`Failed to bootstrap the native WebCodex binaries: ${result.error.message}`);
    return false;
  }
  if (result.signal) {
    console.error(`WebCodex native bootstrap was terminated by ${result.signal}.`);
    return false;
  }
  return result.status === 0;
}

function runNative(options = {}) {
  const customTarget = Boolean(options.target);
  const target = options.target || nativePath(options);
  const argv = options.argv || process.argv.slice(2);
  const needsBootstrap = !fs.existsSync(target) && !customTarget;
  const networkEnvironment = needsNpmNetworkContext(argv, needsBootstrap)
    ? rehydrateNpmNetworkEnvironment(process.env, options)
    : process.env;
  if (needsBootstrap) {
    bootstrapNative({ ...options, environment: networkEnvironment });
    // A concurrent first launch may have completed the same atomic install even
    // if this process's bootstrap attempt lost the final rename race.
    if (!fs.existsSync(target)) {
      console.error("WebCodex installation is incomplete: the native webcodex binary is still missing after bootstrap.");
      process.exitCode = 127;
      return null;
    }
  }
  if (!fs.existsSync(target)) {
    console.error("WebCodex installation is incomplete: the native webcodex binary is missing. Reinstall the npm package.");
    process.exitCode = 127;
    return null;
  }

  const child = childProcess.spawn(target, argv, {
    env: { ...networkEnvironment, WEBCODEX_NPM_WRAPPER: "1" },
    stdio: "inherit",
    windowsHide: false,
    shell: false
  });
  let forwardedSignal = null;
  const forward = (signal) => {
    forwardedSignal = signal;
    if (!child.killed) child.kill(signal);
  };
  const signals = process.platform === "win32" ? ["SIGINT", "SIGTERM"] : ["SIGINT", "SIGTERM", "SIGHUP"];
  for (const signal of signals) process.once(signal, forward);

  const cleanup = () => {
    for (const signal of signals) process.removeListener(signal, forward);
  };
  child.once("error", (err) => {
    cleanup();
    console.error(`Failed to execute the native webcodex binary: ${err.message}`);
    process.exitCode = 127;
  });
  child.once("exit", (code, signal) => {
    cleanup();
    if (signal || forwardedSignal) {
      const exitSignal = signal || forwardedSignal;
      process.kill(process.pid, exitSignal);
      return;
    }
    process.exitCode = code === null ? 1 : code;
  });
  return child;
}

module.exports = {
  bootstrapNative,
  captureProtectedNpmEnvironment,
  exeName,
  nativePath,
  needsNpmNetworkContext,
  queryNpmConfigValue,
  rehydrateNpmNetworkEnvironment,
  runNative
};
