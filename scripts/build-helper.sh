#!/usr/bin/env bash
set -euo pipefail
[[ "$(uname -s)" == Linux ]] || exit 0
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Independent crate: build the shipped resource before compiling Tauri.
cargo build --manifest-path "$ROOT/src-tauri/helper/Cargo.toml" --locked --release
