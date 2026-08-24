"use strict";

const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");

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

function bootstrapNative(options = {}) {
  const root = options.packageRoot || packageRoot();
  const installScript = options.installScript || path.join(root, "install.js");
  if (!fs.existsSync(installScript)) {
    console.error("WebCodex installation is incomplete: install.js is missing. Reinstall the npm package.");
    return false;
  }
  const result = childProcess.spawnSync(process.execPath, [installScript], {
    cwd: root,
    env: process.env,
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
  if (!fs.existsSync(target) && !customTarget) {
    bootstrapNative(options);
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

module.exports = { bootstrapNative, exeName, nativePath, runNative };
