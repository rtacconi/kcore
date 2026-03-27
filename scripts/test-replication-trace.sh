#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKER="${ROOT_DIR}/scripts/check-replication-trace.py"
GENERATOR="${ROOT_DIR}/scripts/generate-replication-trace-fixture.sh"
TRACE_DIR="${ROOT_DIR}/specs/tla/traces"
GENERATED_TRACE="$(mktemp /tmp/kcore-repl-trace.XXXXXX.json)"
trap 'rm -f "${GENERATED_TRACE}"' EXIT

echo "==> replication trace checker (positive)"
python3 "${CHECKER}" "${TRACE_DIR}/replication-sample.json"
python3 "${CHECKER}" "${TRACE_DIR}/replication-sample-2.json"
bash "${GENERATOR}" "${GENERATED_TRACE}"
python3 "${CHECKER}" "${GENERATED_TRACE}"

echo "==> replication trace checker (negative)"
if python3 "${CHECKER}" "${TRACE_DIR}/replication-invalid-terminal.json"; then
  echo "expected invalid-terminal trace to fail, but it passed"
  exit 1
fi

echo "trace harness passed."
