#!/usr/bin/env bash
# Build both Soroban contracts to wasm and the TypeScript CLI. Runs the test suites first.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== cargo test (contracts run inside the Soroban host)"
cargo test

echo "== stellar contract build (wasm32v1-none, release profile)"
stellar contract build
ls -l target/wasm32v1-none/release/biller_registry.wasm target/wasm32v1-none/release/campaign.wasm

echo "== app: npm install + npm test (offline)"
cd "$ROOT/app"
if [ ! -d node_modules ]; then npm install --no-audit --no-fund; fi
npm test
echo "== done"
