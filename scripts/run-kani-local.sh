#!/usr/bin/env bash
# Runs all Kani harnesses for the workspace locally, in parallel,
# detached from the calling shell (and from Cursor itself), via
# `systemd-run --user`. The worker lives under
#   user@<uid>.service / app.slice / kcore-kani-local.service
# so it survives Cursor crashes / restarts.
#
# Usage:
#   scripts/run-kani-local.sh start [--force]   # launch detached run
#   scripts/run-kani-local.sh status            # one-line status of latest run
#   scripts/run-kani-local.sh tail [N]          # tail latest run.log
#   scripts/run-kani-local.sh wait              # block until run finishes
#   scripts/run-kani-local.sh kill              # stop the running unit
#   scripts/run-kani-local.sh logs [N]          # systemd journal for the unit
#
# Environment overrides:
#   KANI_JOBS=N         CBMC parallelism inside one cargo-kani call (default 2)
#   KANI_PARALLEL=N     # of harnesses to run concurrently            (default 2)
#   KANI_VERSION=X.Y.Z  pinned kani-verifier version                  (default 0.67.0)
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
WORKER="${REPO_ROOT}/scripts/_kani-local-worker.sh"
LOG_ROOT="${REPO_ROOT}/target/kani-results"
LATEST_LINK="${LOG_ROOT}/latest"
UNIT="kcore-kani-local.service"

# `systemd-run` may not be on PATH inside the Cursor sandbox shell, so try
# the absolute NixOS path first, then fall back.
SYSTEMD_RUN="$(command -v systemd-run 2>/dev/null || true)"
if [[ -z "${SYSTEMD_RUN}" && -x /run/current-system/sw/bin/systemd-run ]]; then
  SYSTEMD_RUN=/run/current-system/sw/bin/systemd-run
fi
SYSTEMCTL="$(command -v systemctl 2>/dev/null || true)"
if [[ -z "${SYSTEMCTL}" && -x /run/current-system/sw/bin/systemctl ]]; then
  SYSTEMCTL=/run/current-system/sw/bin/systemctl
fi
JOURNALCTL="$(command -v journalctl 2>/dev/null || true)"
if [[ -z "${JOURNALCTL}" && -x /run/current-system/sw/bin/journalctl ]]; then
  JOURNALCTL=/run/current-system/sw/bin/journalctl
fi

KANI_VERSION="${KANI_VERSION:-0.67.0}"
KANI_JOBS="${KANI_JOBS:-2}"
# KANI_PARALLEL only governs the LIGHT harness phase; heavy harnesses are
# forced sequential in the worker because each can peak 25-30 GB.
KANI_PARALLEL="${KANI_PARALLEL:-2}"
# Cgroup memory ceiling for the whole unit. With sequential heavy
# harnesses, ~30 GB is normal; 50 GB leaves headroom for safety while
# still tripping before the host's own OOM killer fires.
KANI_MEMORY_MAX="${KANI_MEMORY_MAX:-50G}"

# Total number of harnesses (kept in sync with worker).
TOTAL_HARNESSES=3

mkdir -p "${LOG_ROOT}"

cmd="${1:-help}"
shift || true

latest_dir() {
  if [[ -L "${LATEST_LINK}" ]]; then
    readlink -f "${LATEST_LINK}"
  fi
}

unit_active() {
  [[ -n "${SYSTEMCTL}" ]] && "${SYSTEMCTL}" --user is-active --quiet "${UNIT}"
}

case "${cmd}" in
  start)
    force=0
    if [[ "${1:-}" == "--force" ]]; then
      force=1
      shift || true
    fi

    if [[ -z "${SYSTEMD_RUN}" || -z "${SYSTEMCTL}" ]]; then
      echo "error: systemd-run / systemctl not found; this script requires systemd --user." >&2
      exit 2
    fi

    if unit_active; then
      if [[ "${force}" -eq 1 ]]; then
        "${SYSTEMCTL}" --user stop "${UNIT}" 2>/dev/null || true
        "${SYSTEMCTL}" --user reset-failed "${UNIT}" 2>/dev/null || true
        sleep 1
      else
        echo "kani-local already running as ${UNIT}"
        echo "  status : scripts/run-kani-local.sh status"
        echo "  force  : scripts/run-kani-local.sh start --force"
        exit 0
      fi
    fi
    # Clear any leftover failed state from a prior run.
    "${SYSTEMCTL}" --user reset-failed "${UNIT}" 2>/dev/null || true

    ts="$(date -u +%Y%m%dT%H%M%SZ)"
    out_dir="${LOG_ROOT}/${ts}"
    mkdir -p "${out_dir}"
    ln -sfn "${out_dir}" "${LATEST_LINK}"
    echo running >"${out_dir}/result"
    echo "${UNIT}" >"${out_dir}/unit"

    "${SYSTEMD_RUN}" --user \
      --unit="${UNIT}" \
      --description="kcore Kani proofs (local, parallel)" \
      --working-directory="${REPO_ROOT}" \
      --property="MemoryMax=${KANI_MEMORY_MAX}" \
      --property="OOMPolicy=continue" \
      --setenv=KANI_OUT_DIR="${out_dir}" \
      --setenv=KANI_REPO_ROOT="${REPO_ROOT}" \
      --setenv=KANI_VERSION="${KANI_VERSION}" \
      --setenv=KANI_JOBS="${KANI_JOBS}" \
      --setenv=KANI_PARALLEL="${KANI_PARALLEL}" \
      --setenv=HOME="${HOME}" \
      --setenv=PATH="${PATH}" \
      -- nix develop -c nix shell nixpkgs#rustup -c bash "${WORKER}"

    main_pid="$("${SYSTEMCTL}" --user show -p MainPID --value "${UNIT}" 2>/dev/null || echo 0)"
    echo "${main_pid}" >"${out_dir}/pid"

    echo "kani-local started"
    echo "  unit    : ${UNIT}"
    echo "  pid     : ${main_pid}"
    echo "  log dir : ${out_dir}"
    echo "  status  : make kani-local-status"
    echo "  tail    : make kani-local-tail"
    ;;

  status)
    dir="$(latest_dir)"
    if [[ -z "${dir}" ]]; then
      echo "no runs yet"
      exit 0
    fi
    state="unknown"
    if [[ -f "${dir}/result" ]]; then
      state="$(cat "${dir}/result")"
    fi
    pass=0
    fail=0
    if compgen -G "${dir}/*.status" >/dev/null; then
      pass=$( (grep -l '^ok$'   "${dir}"/*.status 2>/dev/null || true) | wc -l | tr -d ' ' )
      fail=$( (grep -l '^fail$' "${dir}"/*.status 2>/dev/null || true) | wc -l | tr -d ' ' )
    fi
    pending=$((TOTAL_HARNESSES - pass - fail))

    unit_state="-"
    if [[ -n "${SYSTEMCTL}" ]]; then
      unit_state="$("${SYSTEMCTL}" --user is-active "${UNIT}" 2>/dev/null || true)"
      if [[ -z "${unit_state}" ]]; then unit_state="inactive"; fi
    fi
    main_pid="$( ([[ -n "${SYSTEMCTL}" ]] && "${SYSTEMCTL}" --user show -p MainPID --value "${UNIT}" 2>/dev/null) || echo 0)"

    echo "dir=${dir}"
    echo "state=${state} pass=${pass} fail=${fail} pending=${pending} unit=${unit_state} main_pid=${main_pid}"
    ;;

  tail)
    n="${1:-80}"
    dir="$(latest_dir)"
    if [[ -z "${dir}" ]]; then
      echo "no runs yet"
      exit 0
    fi
    if [[ -f "${dir}/run.log" ]]; then
      tail -n "${n}" "${dir}/run.log"
    else
      echo "no run.log yet in ${dir}"
    fi
    ;;

  logs)
    n="${1:-200}"
    if [[ -n "${JOURNALCTL}" ]]; then
      "${JOURNALCTL}" --user -u "${UNIT}" -n "${n}" --no-pager
    else
      echo "journalctl not available" >&2
      exit 2
    fi
    ;;

  wait)
    if [[ -z "${SYSTEMCTL}" ]]; then
      echo "systemctl not available" >&2
      exit 2
    fi
    while "${SYSTEMCTL}" --user is-active --quiet "${UNIT}"; do
      sleep 5
    done
    dir="$(latest_dir)"
    if [[ -n "${dir}" && -f "${dir}/result" ]]; then
      cat "${dir}/result"
    fi
    ;;

  kill)
    if [[ -n "${SYSTEMCTL}" ]]; then
      "${SYSTEMCTL}" --user stop "${UNIT}" 2>/dev/null || true
      "${SYSTEMCTL}" --user reset-failed "${UNIT}" 2>/dev/null || true
      echo "stopped ${UNIT}"
    fi
    ;;

  help|*)
    sed -n '1,22p' "$0"
    ;;
esac
