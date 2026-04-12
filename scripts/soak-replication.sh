#!/usr/bin/env bash
set -euo pipefail

ITERATIONS="${ITERATIONS:-5}"
PER_STEP_TIMEOUT="${PER_STEP_TIMEOUT:-180s}"

echo "==> replication soak harness"
echo "iterations=${ITERATIONS} timeout_per_step=${PER_STEP_TIMEOUT}"

if ! command -v timeout >/dev/null 2>&1; then
  echo "error: timeout command is required for bounded soak runs" >&2
  exit 1
fi

run_step() {
  local name="$1"
  shift
  echo "---- ${name}"
  if ! timeout "${PER_STEP_TIMEOUT}" "$@"; then
    echo "error: step failed or timed out: ${name}" >&2
    exit 1
  fi
}

for i in $(seq 1 "${ITERATIONS}"); do
  echo "==> iteration ${i}/${ITERATIONS}"
  run_step "controller_replication_tests" cargo test -p kcore-controller replication::tests::
  run_step "controller_admin_replication_tests" cargo test -p kcore-controller grpc::admin::tests::
  run_step "controller_heartbeat_replication_outbox" cargo test -p kcore-controller heartbeat_appends_replication_outbox_when_configured --
  run_step "replication_trace_drift" bash ./scripts/test-replication-trace.sh
done

echo "replication soak harness passed (${ITERATIONS} iterations)"
