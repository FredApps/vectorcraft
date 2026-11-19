#!/usr/bin/env bash
set -euo pipefail
# Bounded actual-workspace CAD check, not full application/wasm/platform validation.
cargo fmt --all -- --check
cargo clippy -p vectorcraft-cad --all-targets --locked -- -D warnings
cargo test -p vectorcraft-cad --locked
