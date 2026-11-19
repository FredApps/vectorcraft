#!/usr/bin/env bash
set -euo pipefail
# Actual locked CAD tests and selected-package lint; unrelated dependency lint debt
# remains reported by run44518432412, not suppressed or claimed passed here.
status=0
cargo fmt --all -- --check || status=1
cargo clippy -p vectorcraft-cad --all-targets --no-deps --locked -- -D warnings || status=1
cargo test -p vectorcraft-cad --locked || status=1
exit "$status"
