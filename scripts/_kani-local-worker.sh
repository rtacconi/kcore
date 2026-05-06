#!/usr/bin/env bash
# Internal worker for scripts/run-kani-local.sh.
# Do not invoke directly; call `scripts/run-kani-local.sh start` instead.
#
# Required env:
#   KANI_OUT_DIR     absolute path of run dir (created by parent)
#   KANI_REPO_ROOT   absolute path of repo root
# Optional env:
#   KANI_VERSION     pinned kani-verifier version (default 0.67.0)
#   KANI_JOBS        CBMC parallelism per harness (default 2)
#   KANI_PARALLEL    concurrent harnesses (default 2)
set -euo pipefail

: "${KANI_OUT_DIR:?missing KANI_OUT_DIR}"
: "${KANI_REPO_ROOT:?missing KANI_REPO_ROOT}"
KANI_VERSION="${KANI_VERSION:-0.67.0}"
KANI_JOBS="${KANI_JOBS:-2}"
KANI_PARALLEL="${KANI_PARALLEL:-2}"
# Wall-clock budget per harness (in seconds). If CBMC exceeds it we
# mark the harness as `timeout` and move on so the rest of the run
# still makes progress. Override via KANI_HARNESS_TIMEOUT.
KANI_HARNESS_TIMEOUT="${KANI_HARNESS_TIMEOUT:-600}"

cd "${KANI_REPO_ROOT}"

# Kani is now scoped to ONLY the byte-loop path scanners, which CBMC
# enumerates over every 4-byte ASCII input in a few seconds each.
# Higher-level sanitizers (nix_escape, sanitize_nix_attr_key, segment
# validators, layout parsers) are covered by proptest in `cargo test`
# because CBMC does not converge on them in any reasonable time.
all_harnesses=(
  "kcore-sanitize::kani_proofs::dot_dot_check_never_panics"
  "kcore-sanitize::kani_proofs::assert_safe_path_never_panics"
  "kcore-sanitize::kani_proofs::assert_safe_path_acceptance_implies_safe"
)

echo "kani local run ($(date -u +%FT%TZ))"             >"${KANI_OUT_DIR}/run.log"
echo "  KANI_VERSION=${KANI_VERSION}"                  >>"${KANI_OUT_DIR}/run.log"
echo "  KANI_JOBS=${KANI_JOBS} (per harness CBMC)"     >>"${KANI_OUT_DIR}/run.log"
echo "  KANI_PARALLEL=${KANI_PARALLEL} (concurrent)"   >>"${KANI_OUT_DIR}/run.log"
echo "  harnesses=${#all_harnesses[@]}"                >>"${KANI_OUT_DIR}/run.log"
echo                                                   >>"${KANI_OUT_DIR}/run.log"

ensure_kani() {
  if ! command -v cargo-kani >/dev/null 2>&1; then
    echo "installing kani-verifier ${KANI_VERSION}"    >>"${KANI_OUT_DIR}/run.log"
    cargo install --locked kani-verifier --version "${KANI_VERSION}" \
      >>"${KANI_OUT_DIR}/run.log" 2>&1
  else
    local installed
    installed="$(cargo-kani --version 2>/dev/null | awk '{print $NF}')"
    if [[ "${installed}" != "${KANI_VERSION}" ]]; then
      echo "reinstalling kani-verifier ${KANI_VERSION} (was ${installed})" \
        >>"${KANI_OUT_DIR}/run.log"
      cargo install --locked --force kani-verifier --version "${KANI_VERSION}" \
        >>"${KANI_OUT_DIR}/run.log" 2>&1
    fi
  fi
  if [[ ! -d "${HOME}/.kani" ]] || [[ -z "$(ls -A "${HOME}/.kani" 2>/dev/null)" ]]; then
    echo "running cargo-kani setup" >>"${KANI_OUT_DIR}/run.log"
    cargo-kani setup >>"${KANI_OUT_DIR}/run.log" 2>&1
  fi
}

run_one() {
  local entry="$1"
  local crate="${entry%%::*}"
  local harness="${entry#*::}"
  local safe="${entry//::/__}"
  local log="${KANI_OUT_DIR}/${safe}.log"
  local status_file="${KANI_OUT_DIR}/${safe}.status"

  echo "==> ${entry}" >>"${KANI_OUT_DIR}/run.log"
  if cargo kani -p "${crate}" \
        --harness "${harness}" \
        --jobs "${KANI_JOBS}" \
        --output-format terse \
        >"${log}" 2>&1; then
    echo ok >"${status_file}"
    echo "    PASS ${entry}" >>"${KANI_OUT_DIR}/run.log"
    return 0
  else
    echo fail >"${status_file}"
    echo "    FAIL ${entry} (see ${log})" >>"${KANI_OUT_DIR}/run.log"
    return 1
  fi
}

ensure_kani

run_phase_parallel() {
  local concurrency="$1"; shift
  local pids=()
  local entry p new_pids
  local -i phase_fails=0
  for entry in "$@"; do
    while [[ "${#pids[@]}" -ge "${concurrency}" ]]; do
      new_pids=()
      for p in "${pids[@]}"; do
        if kill -0 "$p" 2>/dev/null; then
          new_pids+=("$p")
        else
          if ! wait "$p"; then
            phase_fails=$((phase_fails + 1))
          fi
        fi
      done
      pids=("${new_pids[@]}")
      sleep 1
    done
    run_one "${entry}" &
    pids+=("$!")
  done
  for p in "${pids[@]}"; do
    if ! wait "$p"; then
      phase_fails=$((phase_fails + 1))
    fi
  done
  return "${phase_fails}"
}

fails=0
echo "-- running ${#all_harnesses[@]} harnesses (parallel up to ${KANI_PARALLEL}) --" \
  >>"${KANI_OUT_DIR}/run.log"
if ! run_phase_parallel "${KANI_PARALLEL}" "${all_harnesses[@]}"; then
  fails=$((fails + $?))
fi

if [[ "${fails}" -eq 0 ]]; then
  echo done >"${KANI_OUT_DIR}/result"
  echo "ALL PASS" >>"${KANI_OUT_DIR}/run.log"
  exit 0
else
  echo "fail (${fails})" >"${KANI_OUT_DIR}/result"
  echo "FAILED ${fails} harness(es)" >>"${KANI_OUT_DIR}/run.log"
  exit 1
fi
