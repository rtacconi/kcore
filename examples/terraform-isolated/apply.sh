#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${PROJECT_DIR}/../.." && pwd)"

TEST_ID="${1:-$(date +%Y%m%d%H%M%S)}"
PROVIDER_DIR="${PROJECT_DIR}/.providers"
PROVIDER_BIN="${PROVIDER_DIR}/terraform-provider-kcore"
TFRC_PATH="${PROJECT_DIR}/.terraformrc.local"
LAST_TEST_ID_FILE="${PROJECT_DIR}/.last_test_id"

mkdir -p "${PROVIDER_DIR}"

if command -v tofu >/dev/null 2>&1; then
  TF_CMD=(tofu)
elif command -v nix >/dev/null 2>&1; then
  TF_CMD=("${REPO_ROOT}/nix_shell" tofu)
else
  echo "Error: OpenTofu (tofu) is not installed and nix is not available for fallback." >&2
  exit 1
fi

echo "Building local OpenTofu provider binary..."
"${REPO_ROOT}/nix_shell" bash -lc \
  "cd '${REPO_ROOT}/terraform-provider-kcore' && go build -o '${PROVIDER_BIN}' ."

cat > "${TFRC_PATH}" <<EOF
provider_installation {
  dev_overrides {
    "registry.opentofu.org/kcore/kcore" = "${PROVIDER_DIR}"
  }
  direct {}
}
EOF

export TOFU_CLI_CONFIG_FILE="${TFRC_PATH}"
export TF_CLI_CONFIG_FILE="${TFRC_PATH}"

echo "Applying OpenTofu for test_id=${TEST_ID} (dev override mode)..."
"${TF_CMD[@]}" -chdir="${PROJECT_DIR}" apply -auto-approve -var "test_id=${TEST_ID}"
printf "%s\n" "${TEST_ID}" > "${LAST_TEST_ID_FILE}"

echo
echo "Done. To destroy this test VM:"
echo "  ${PROJECT_DIR}/destroy.sh ${TEST_ID}"
