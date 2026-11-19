#!/usr/bin/env bash
set -euo pipefail
# Lock preparation only, not an acceptance test. Work in a disposable copy so the
# shared gate continues to enforce an unchanged checked-out Cargo.lock.
probe="$(mktemp -d)"
git archive HEAD | tar -x -C "$probe"
cd "$probe"
cargo metadata --format-version 1 > /dev/null
mkdir -p "$GITHUB_WORKSPACE/.diagnostic-output"
cp Cargo.lock "$GITHUB_WORKSPACE/.diagnostic-output/Cargo.lock"
