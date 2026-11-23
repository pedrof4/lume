#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build
compiler=${LUMEC:-${CARGO_TARGET_DIR:-target}/release/lumec}
"$compiler" build bootstrap/compiler.lume -O2 -o build/stage1.wasm
node tools/host.cjs build/stage1.wasm bootstrap/compiler.lume build/stage2.wasm
node tools/host.cjs build/stage2.wasm bootstrap/compiler.lume build/stage3.wasm
cmp build/stage2.wasm build/stage3.wasm
sha256sum build/stage2.wasm build/stage3.wasm
