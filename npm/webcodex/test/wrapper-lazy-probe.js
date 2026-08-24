"use strict";

const { runNative } = require("../bin/wrapper");

runNative({ packageRoot: process.argv[2], argv: process.argv.slice(3) });
