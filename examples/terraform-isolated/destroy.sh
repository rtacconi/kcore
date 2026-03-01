#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${PROJECT_DIR}/../.." && pwd)"
TFRC_PATH="${PROJECT_DIR}/.terraformrc.local"
LAST_TEST_ID_FILE="${PROJECT_DIR}/.last_test_id"

if [ ! -f "${TFRC_PATH}" ]; then
  echo "Error: ${TFRC_PATH} not found. Run apply.sh first in this directory." >&2
  exit 1
fi

if command -v tofu >/dev/null 2>&1; then
  TF_CMD=(tofu)
elif command -v nix >/dev/null 2>&1; then
  TF_CMD=("${REPO_ROOT}/nix_shell" tofu)
else
  echo "Error: OpenTofu (tofu) is not installed and nix is not available for fallback." >&2
  exit 1
fi

export TOFU_CLI_CONFIG_FILE="${TFRC_PATH}"
export TF_CLI_CONFIG_FILE="${TFRC_PATH}"

TEST_ID="${1:-}"
if [ -z "${TEST_ID}" ] && [ -f "${LAST_TEST_ID_FILE}" ]; then
  TEST_ID="$(cat "${LAST_TEST_ID_FILE}")"
fi
if [ -z "${TEST_ID}" ]; then
  echo "Error: test_id is required (pass as first arg or run apply.sh first)." >&2
  exit 1
fi

echo "Destroying OpenTofu-managed test VM..."
"${TF_CMD[@]}" -chdir="${PROJECT_DIR}" destroy -auto-approve -var "test_id=${TEST_ID}"
