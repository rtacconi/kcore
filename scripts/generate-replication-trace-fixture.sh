#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_PATH="${1:-${ROOT_DIR}/specs/tla/traces/replication-generated.json}"
TEST_NAME="replication::tests::export_replication_trace_fixture"

run_with_cargo() {
  KCORE_REPLICATION_TRACE_OUT="${OUTPUT_PATH}" \
    cargo test -p kcore-controller "${TEST_NAME}" -- --exact
}

run_with_nix() {
  KCORE_REPLICATION_TRACE_OUT="${OUTPUT_PATH}" \
    nix develop -c cargo test -p kcore-controller "${TEST_NAME}" -- --exact
}

if command -v cargo >/dev/null 2>&1; then
  run_with_cargo
else
  run_with_nix
fi

echo "generated replication trace: ${OUTPUT_PATH}"
