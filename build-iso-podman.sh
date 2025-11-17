#!/bin/bash
# Build kcore ISO inside a Podman Linux container

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
echo -e "${BLUE}║  Building kcore ISO inside Podman Linux container      ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if Podman is available
if ! command -v podman &> /dev/null; then
    echo -e "${RED}Error: podman not found. Please install Podman.${NC}"
    echo "Install with: brew install podman"
    exit 1
fi

# Check if Podman machine is running (macOS)
if [[ "$OSTYPE" == "darwin"* ]]; then
    if ! podman machine list | grep -q "running"; then
        echo -e "${YELLOW}Podman machine not running. Starting it...${NC}"
        podman machine start || {
            echo -e "${RED}Failed to start Podman machine.${NC}"
            echo "Try: podman machine init && podman machine start"
            exit 1
        }
    fi
fi

# Use a NixOS container image that has Nix pre-installed
CONTAINER_IMAGE="nixos/nix:2.24.1"

echo -e "${YELLOW}📦 Checking for container image: $CONTAINER_IMAGE${NC}"
if ! podman image exists "$CONTAINER_IMAGE" 2>/dev/null; then
    echo "Pulling image (this may take a few minutes)..."
    podman pull "$CONTAINER_IMAGE"
else
    echo "Image already available"
fi

echo ""
echo -e "${YELLOW}🔨 Starting build container...${NC}"
echo -e "${BLUE}This will take 30-60 minutes as it builds the ISO with node agent.${NC}"
echo -e "${BLUE}The container runs Linux/amd64, so on macOS it uses emulation (slower).${NC}"
echo ""

# Run the build inside the container
# Use --privileged mode and disable syscall filtering in Nix
# The filter-syscalls=false option prevents Nix from using seccomp
podman run --rm \
    --platform linux/amd64 \
    --privileged \
    --security-opt apparmor=unconfined \
    --cap-add SYS_ADMIN \
    --cap-add SYS_CHROOT \
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
        echo '🔧 Configuring Nix to disable sandbox completely...'
        mkdir -p /tmp/nix-conf
        cat > /tmp/nix-conf/nix.conf << 'EOF'
sandbox = false
build-use-sandbox = false
build-sandbox-paths = 
filter-syscalls = false
EOF
        export NIX_CONF_DIR=/tmp/nix-conf
        echo 'Nix config:'
        cat /tmp/nix-conf/nix.conf
        echo ''
        echo '🔧 Building ISO (this will take a while)...'
        echo ''
        nix --extra-experimental-features nix-command \
            --extra-experimental-features flakes \
            --option sandbox false \
            --option build-use-sandbox false \
            --option filter-syscalls false \
            build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' \
            -o result-iso
        echo ''
        echo '✅ Build complete!'
        echo ''
        echo '📁 ISO files:'
        find result-iso -name '*.iso' -type f -exec ls -lh {} \; || find result-iso -type f | head -5
    "

BUILD_EXIT=$?

if [ $BUILD_EXIT -eq 0 ]; then
    echo ""
    echo -e "${GREEN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║  ✓ ISO build completed successfully!                    ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "📁 ISO location:"
    find result-iso -name "*.iso" -type f 2>/dev/null | head -1 || echo "Check result-iso/ directory"
    echo ""
    echo "Next step: Write ISO to USB stick with:"
    echo "  ./write-usb.sh"
else
    echo ""
    echo -e "${RED}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${RED}║  ✗ Build failed                                         ║${NC}"
    echo -e "${RED}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "Common issues:"
    echo "  - Podman machine not running (macOS): podman machine start"
    echo "  - Out of disk space: Check available space"
    echo "  - Network issues: Check internet connection"
    exit 1
fi

