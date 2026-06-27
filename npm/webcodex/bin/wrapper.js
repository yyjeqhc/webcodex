"use strict";

const fs = require("fs");
const path = require("path");
const childProcess = require("child_process");

function exeName(name) {
  return process.platform === "win32" ? `${name}.exe` : name;
}

function packageRoot() {
  return path.resolve(__dirname, "..");
}

function nativePath(name) {
  if (process.env.WEBCODEX_BINARY_DIR) {
    return path.join(process.env.WEBCODEX_BINARY_DIR, exeName(name));
  }
  return path.join(packageRoot(), "vendor", "bin", exeName(name));
}

function runNative(name) {
  const target = nativePath(name);
  if (!fs.existsSync(target)) {
    console.error(
      `WebCodex native binary not found: ${target}\nRun npm install again or set WEBCODEX_BINARY_DIR=/path/to/local/bin`
    );
    process.exit(127);
  }
  const child = childProcess.spawn(target, process.argv.slice(2), {
    stdio: "inherit",
    windowsHide: false
  });
  child.on("exit", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code === null ? 1 : code);
  });
  child.on("error", (err) => {
    console.error(`Failed to execute ${target}: ${err.message}`);
    process.exit(127);
  });
}

module.exports = { exeName, nativePath, runNative };
