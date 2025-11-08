#!/bin/bash
# Build kcode ISO on remote Ubuntu server via SSH

set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
SSH_HOST="rtacconi@192.168.40.10"
SSH_KEY="$HOME/.ssh/id_ed25519_gmail"
REMOTE_DIR="/mnt/md126/kcore"
NIX_STORE="/mnt/md126/nix"
LOCAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${BLUE}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Building kcode ISO on remote Ubuntu server            ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check SSH key exists
if [ ! -f "$SSH_KEY" ]; then
    echo -e "${RED}Error: SSH key not found at $SSH_KEY${NC}"
    exit 1
fi

# Test SSH connection
echo -e "${YELLOW}🔌 Testing SSH connection...${NC}"
if ! ssh -i "$SSH_KEY" -o ConnectTimeout=5 "$SSH_HOST" "echo 'Connection successful'" 2>/dev/null; then
    echo -e "${RED}Error: Cannot connect to $SSH_HOST${NC}"
    echo "Please check:"
    echo "  - SSH key path: $SSH_KEY"
    echo "  - Host is reachable: ping 192.168.40.10"
    echo "  - SSH access is configured"
    exit 1
fi
echo -e "${GREEN}✅ SSH connection OK${NC}"
echo ""

# Check/install Nix on remote
echo -e "${YELLOW}📦 Checking Nix installation on remote server...${NC}"
if ssh -i "$SSH_KEY" "$SSH_HOST" "command -v nix >/dev/null 2>&1 || test -f ~/.nix-profile/etc/profile.d/nix.sh"; then
    echo -e "${GREEN}✅ Nix already installed${NC}"
    # Source Nix if needed
    ssh -i "$SSH_KEY" "$SSH_HOST" "source ~/.nix-profile/etc/profile.d/nix.sh 2>/dev/null || true"
    
    # Configure Nix to use custom store location (skip if sudo required)
    echo -e "${YELLOW}🔧 Checking Nix store location...${NC}"
    NIX_STORE_STATUS=$(ssh -i "$SSH_KEY" "$SSH_HOST" "bash -c '
        if [ -L /nix ]; then
            echo \"symlink:\$(readlink /nix)\"
        elif [ -d /nix ]; then
            echo \"directory\"
        else
            echo \"missing\"
        fi
    '" 2>/dev/null)
    
    if echo "$NIX_STORE_STATUS" | grep -q "symlink:$NIX_STORE"; then
        echo -e "${GREEN}✅ Nix store already configured at $NIX_STORE${NC}"
    elif echo "$NIX_STORE_STATUS" | grep -q "symlink:"; then
        CURRENT_LINK=$(echo "$NIX_STORE_STATUS" | cut -d: -f2)
        echo -e "${YELLOW}⚠️  /nix points to $CURRENT_LINK (not $NIX_STORE)${NC}"
        echo -e "${YELLOW}   To change it, run on server:${NC}"
        echo -e "${YELLOW}   sudo rm /nix && sudo ln -s $NIX_STORE /nix${NC}"
    else
        echo -e "${YELLOW}⚠️  /nix is a directory (not symlink).${NC}"
        echo -e "${YELLOW}   Note: Nix store will use /nix (not $NIX_STORE)${NC}"
        echo -e "${YELLOW}   Build directory will use $REMOTE_DIR as requested${NC}"
        echo ""
        echo -e "${YELLOW}   To move Nix store to $NIX_STORE later, run on server:${NC}"
        echo -e "${YELLOW}   sudo systemctl stop nix-daemon.socket nix-daemon.service${NC}"
        echo -e "${YELLOW}   sudo mv /nix $NIX_STORE${NC}"
        echo -e "${YELLOW}   sudo ln -s $NIX_STORE /nix${NC}"
        echo -e "${YELLOW}   sudo systemctl start nix-daemon.socket nix-daemon.service${NC}"
        echo ""
        echo -e "${YELLOW}   Proceeding with build anyway...${NC}"
    fi
else
    echo -e "${RED}Nix not found on remote server.${NC}"
    echo ""
    echo -e "${YELLOW}Please install Nix manually first:${NC}"
    echo ""
    echo "  1. SSH to the server:"
    echo "     ssh -i $SSH_KEY $SSH_HOST"
    echo ""
    echo "  2. On the server, run:"
    echo "     sudo mkdir -m 0755 -p /nix"
    echo "     sudo chown \$USER /nix"
    echo "     sh <(curl -L https://nixos.org/nix/install) --daemon"
    echo ""
    echo "  3. After installation, log out and back in, or run:"
    echo "     source ~/.nix-profile/etc/profile.d/nix.sh"
    echo ""
    echo -e "${YELLOW}Then run this script again.${NC}"
    exit 1
fi
echo ""

# Enable Nix flakes and experimental features
echo -e "${YELLOW}🔧 Configuring Nix...${NC}"
ssh -i "$SSH_KEY" "$SSH_HOST" "mkdir -p ~/.config/nix && \
    echo 'experimental-features = nix-command flakes' >> ~/.config/nix/nix.conf 2>/dev/null || true"
echo -e "${GREEN}✅ Nix configured${NC}"
echo ""

# Create remote directory
echo -e "${YELLOW}📁 Setting up remote build directory...${NC}"
ssh -i "$SSH_KEY" "$SSH_HOST" "mkdir -p $REMOTE_DIR && rm -rf $REMOTE_DIR/* $REMOTE_DIR/.* 2>/dev/null || true"
echo -e "${GREEN}✅ Remote directory ready at $REMOTE_DIR${NC}"
echo ""

# Transfer files (using rsync for efficiency)
echo -e "${YELLOW}📤 Transferring files to remote server...${NC}"
rsync -avz --progress \
    -e "ssh -i $SSH_KEY" \
    --exclude '.git' \
    --exclude 'result*' \
    --exclude '*.iso' \
    --exclude '.devbox' \
    --exclude 'devbox.lock' \
    --exclude 'bin/' \
    --exclude '*.db' \
    --exclude '*.log' \
    --exclude 'certs/*.key' \
    --exclude 'certs/*.crt' \
    "$LOCAL_DIR/" "$SSH_HOST:$REMOTE_DIR/"
echo -e "${GREEN}✅ Files transferred${NC}"
echo ""

# Build ISO on remote
echo -e "${YELLOW}🔨 Building ISO on remote server (this will take 30-60 minutes)...${NC}"
echo -e "${BLUE}Build directory: $REMOTE_DIR${NC}"
echo -e "${BLUE}Nix store: $NIX_STORE${NC}"
echo -e "${BLUE}This runs natively on AMD64 Linux, so it will be much faster than macOS emulation.${NC}"
echo ""

ssh -i "$SSH_KEY" "$SSH_HOST" "cd $REMOTE_DIR && \
    export NIXPKGS_ALLOW_UNFREE=1 && \
    source ~/.nix-profile/etc/profile.d/nix.sh 2>/dev/null || true && \
    nix --extra-experimental-features nix-command \
        --extra-experimental-features flakes \
        --option sandbox false \
        build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' \
        -o result-iso 2>&1" | tee /tmp/remote-build.log

BUILD_EXIT=$?

if [ $BUILD_EXIT -eq 0 ]; then
    echo ""
    echo -e "${GREEN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║  ✓ ISO build completed successfully!                    ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    # Find ISO on remote
    echo -e "${YELLOW}📥 Downloading ISO from remote server...${NC}"
    ISO_PATH=$(ssh -i "$SSH_KEY" "$SSH_HOST" "find $REMOTE_DIR/result-iso -name '*.iso' -type f 2>/dev/null | head -1")
    
    if [ -z "$ISO_PATH" ]; then
        # Try alternative location
        ISO_PATH=$(ssh -i "$SSH_KEY" "$SSH_HOST" "ls $REMOTE_DIR/result-iso/iso/*.iso 2>/dev/null | head -1")
    fi
    
    if [ -z "$ISO_PATH" ]; then
        echo -e "${RED}Warning: Could not find ISO file on remote server${NC}"
        echo "Checking result-iso directory:"
        ssh -i "$SSH_KEY" "$SSH_HOST" "ls -lh $REMOTE_DIR/result-iso/ 2>/dev/null || echo 'Directory not found'"
    else
        echo "Found ISO: $ISO_PATH"
        scp -i "$SSH_KEY" "$SSH_HOST:$ISO_PATH" "$LOCAL_DIR/kcode.iso"
        echo -e "${GREEN}✅ ISO downloaded to: $LOCAL_DIR/kcode.iso${NC}"
        echo ""
        echo "Next step: Write ISO to USB stick with:"
        echo "  ./write-usb.sh kcode.iso"
    fi
else
    echo ""
    echo -e "${RED}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${RED}║  ✗ Build failed                                         ║${NC}"
    echo -e "${RED}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "Check the build log:"
    echo "  tail -100 /tmp/remote-build.log"
    echo ""
    echo "Or check remote logs:"
    echo "  ssh -i $SSH_KEY $SSH_HOST 'tail -100 $REMOTE_DIR/build.log'"
    exit 1
fi

