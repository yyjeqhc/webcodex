"use strict";

const path = require("path");
const { createRequire } = require("module");

const keyMap = [
  ["npm_config_https_proxy", "https-proxy"],
  ["npm_config_proxy", "proxy"],
  ["npm_config_noproxy", "noproxy"],
  ["npm_config_no_proxy", "noproxy"],
  ["npm_config_cafile", "cafile"],
  ["npm_config_ca", "ca"],
  ["npm_config_strict_ssl", "strict-ssl"]
];

function inheritedValues() {
  const values = {};
  for (const [envKey] of keyMap) {
    const value = process.env[envKey];
    if (typeof value === "string" && value.length > 0) values[envKey] = value;
  }
  return values;
}

async function configuredValues(npmCli) {
  const requireFromNpm = createRequire(npmCli);
  const Config = requireFromNpm("@npmcli/config");
  const definitions = requireFromNpm("@npmcli/config/lib/definitions");
  const config = new Config({
    npmPath: path.dirname(path.dirname(npmCli)),
    definitions: definitions.definitions,
    flatten: definitions.flatten,
    shorthands: definitions.shorthands,
    argv: [],
    env: process.env,
    cwd: process.cwd()
  });
  await config.load();
  const values = {};
  for (const [envKey, configKey] of keyMap) {
    const value = config.get(configKey);
    if (value === undefined || value === null || value === "") continue;
    values[envKey] = Array.isArray(value) ? value.join("\n") : String(value);
  }
  return values;
}

async function main() {
  const npmCli = process.argv[2];
  const values = npmCli ? await configuredValues(path.resolve(npmCli)) : inheritedValues();
  process.stdout.write(JSON.stringify(values));
}

main().catch(() => {
  process.exitCode = 1;
});
