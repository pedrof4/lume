#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
./tools/bootstrap.sh
node tools/suite.cjs
node tools/benchmark.cjs
