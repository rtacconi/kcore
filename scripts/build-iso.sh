#!/usr/bin/env bash
set -euo pipefail

IMAGE=nixos/nix:2.18.1
WORKDIR=/workspace

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is required. Please install Docker Desktop." >&2
  exit 1
fi

echo "Launching Linux container to build NixOS ISO..."
docker run --rm \
  --pull=always \
  -v "$(pwd)":${WORKDIR} \
  -w ${WORKDIR} \
  -e NIX_CONFIG="experimental-features = nix-command flakes" \
  --security-opt seccomp=unconfined \
  --privileged \
  ${IMAGE} \
  bash -lc "set -e; nix --version && nix build --extra-experimental-features 'nix-command flakes' .#nixosConfigurations.kvm-node-iso.config.system.build.isoImage && mkdir -p ${WORKDIR}/out && ISO=\$(readlink -f result/iso/*.iso) && echo 'Copying ISO:' \$ISO && cp -v \$ISO ${WORKDIR}/out/ && ls -lh ${WORKDIR}/out"

echo "Done. ISO copied to ./out"
