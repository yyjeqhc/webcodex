"use strict";

const keys = [
  "npm_config_https_proxy",
  "npm_config_proxy",
  "npm_config_noproxy",
  "npm_config_no_proxy",
  "npm_config_cafile",
  "npm_config_ca",
  "npm_config_strict_ssl"
];

const values = {};
for (const key of keys) {
  const value = process.env[key];
  if (typeof value === "string" && value.length > 0) values[key] = value;
}

process.stdout.write(JSON.stringify(values));
