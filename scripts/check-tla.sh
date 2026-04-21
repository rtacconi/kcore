#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPEC_DIR="${ROOT_DIR}/specs/tla"

if [[ -n "${TLC_CMD:-}" ]]; then
  TLC_BIN="${TLC_CMD}"
elif command -v tlc >/dev/null 2>&1; then
  TLC_BIN="tlc"
elif [[ -n "${TLA2TOOLS_JAR:-}" ]]; then
  if [[ "${TLA2TOOLS_JAR}" = /* ]]; then
    TLC_JAR="${TLA2TOOLS_JAR}"
  else
    TLC_JAR="${ROOT_DIR}/${TLA2TOOLS_JAR}"
  fi
  TLC_BIN="java -cp \"${TLC_JAR}\" tlc2.TLC"
else
  echo "TLC not found."
  echo "Install a 'tlc' command or set TLA2TOOLS_JAR=/path/to/tla2tools.jar"
  exit 1
fi

run_tlc() {
  local module="$1"
  local cfg="$2"
  echo "==> TLC ${module}"
  # shellcheck disable=SC2086
  eval ${TLC_BIN} -deadlock -workers 1 -cleanup -config "${cfg}" "${module}"
}

pushd "${SPEC_DIR}" >/dev/null
run_tlc "ControllerNodeReconcile.tla" "ControllerNodeReconcile.cfg"
run_tlc "ControllerReplication.tla" "ControllerReplication.cfg"
run_tlc "CrossDcReplication.tla" "CrossDcReplication.cfg"
run_tlc "DiskLayoutReconcile.tla" "DiskLayoutReconcile.cfg"
popd >/dev/null

echo "TLA+ model checks passed."
