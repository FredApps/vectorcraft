#!/usr/bin/env bash
set -euo pipefail
# Bounded actual-workspace CAD check, not full application/wasm/platform validation.
# Collect independent results; every failure still makes the diagnostic fail.
status=0
cargo fmt --all -- --check || status=1
cargo clippy -p vectorcraft-cad --all-targets --locked -- -D warnings || status=1
cargo test -p vectorcraft-cad --locked || status=1
exit "$status"
