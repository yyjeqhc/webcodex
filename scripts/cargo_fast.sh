#!/usr/bin/env bash
set -euo pipefail

# Opt-in accelerator for local Cargo work. Keep canonical Cargo commands valid
# everywhere: missing accelerators always fall back to the normal toolchain.
cargo_bin="${CARGO:-cargo}"

# mold is Linux/ELF-only here. `mold -run` intercepts linker execution without
# adding project-wide rustflags, so macOS and Windows linker selection is never
# changed and Linux hosts without mold simply use Cargo's normal linker.
if [[ "$(uname -s)" == "Linux" ]] && command -v mold >/dev/null 2>&1; then
    exec mold -run "$cargo_bin" "$@"
fi

exec "$cargo_bin" "$@"
