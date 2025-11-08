#!/bin/bash
# Build kcode ISO inside a Podman Linux container - NO SECCOMP VERSION
# This version tries to work around macOS Podman seccomp issues

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Building kcode ISO (no-seccomp workaround)             ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if Podman is available
if ! command -v podman &> /dev/null; then
    echo -e "${RED}Error: podman not found.${NC}"
    exit 1
fi

# Check if Podman machine is running (macOS)
if [[ "$OSTYPE" == "darwin"* ]]; then
    if ! podman machine list | grep -q "running"; then
        echo -e "${YELLOW}Starting Podman machine...${NC}"
        podman machine start || exit 1
    fi
fi

CONTAINER_IMAGE="nixos/nix:2.24.1"

echo -e "${YELLOW}📦 Using container: $CONTAINER_IMAGE${NC}"
echo ""

# Try using --security-opt seccomp=/dev/null or completely bypass seccomp
# Also try setting kernel.unprivileged_userns_clone=1
podman run --rm \
    --platform linux/amd64 \
    --privileged \
    --security-opt seccomp=/dev/null \
    --security-opt apparmor=unconfined \
    --cap-add ALL \
    --tmpfs /tmp \
    --tmpfs /run \
    -v "$SCRIPT_DIR:/work:Z" \
    -w /work \
    -e NIXPKGS_ALLOW_UNFREE=1 \
    -e NIX_CONF_DIR=/tmp/nix-conf \
    "$CONTAINER_IMAGE" \
    sh -c "
        set -e
        echo '📋 Nix version:'
        nix --version
        echo ''
        echo '🔧 Disabling Nix sandbox completely...'
        mkdir -p /tmp/nix-conf
        cat > /tmp/nix-conf/nix.conf << 'EOF'
sandbox = false
build-use-sandbox = false
build-sandbox-paths = 
sandbox-fallback = false
EOF
        export NIX_CONF_DIR=/tmp/nix-conf
        echo 'Nix config:'
        cat /tmp/nix-conf/nix.conf
        echo ''
        echo '🔧 Building ISO...'
        echo ''
        nix --extra-experimental-features nix-command \
            --extra-experimental-features flakes \
            --option sandbox false \
            --option build-use-sandbox false \
            --option sandbox-fallback false \
            build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' \
            -o result-iso
        echo ''
        echo '✅ Build complete!'
        find result-iso -name '*.iso' -type f -exec ls -lh {} \; 2>/dev/null || echo 'Check result-iso/'
    "

