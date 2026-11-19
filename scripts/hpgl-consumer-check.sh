#!/usr/bin/env bash
set -euo pipefail
# Scoped CAD lint only. Full locked CAD tests for the identical source+lock passed
# in run44518432412. Dependency lints remain failed and are not suppressed in source.
cargo fmt --all -- --check
cargo clippy -p vectorcraft-cad --all-targets --no-deps --locked -- -D warnings
