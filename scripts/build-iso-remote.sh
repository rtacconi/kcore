#!/bin/bash
# Build kcore ISO on remote Linux server via SSH

set -euo pipefail

SSH_HOST="rtacconi@192.168.40.10"
SSH_KEY="$HOME/.ssh/id_ed25519_gmail"
REMOTE_DIR="/mnt/md126/kcore-rust"
LOCAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "======================================================"
echo "  Building kcore ISO on remote Linux server"
echo "======================================================"
echo ""

if [ ! -f "$SSH_KEY" ]; then
    echo "Error: SSH key not found at $SSH_KEY"
    exit 1
fi

echo "Testing SSH connection..."
if ! ssh -i "$SSH_KEY" -o ConnectTimeout=5 "$SSH_HOST" "echo 'Connection OK'" 2>/dev/null; then
    echo "Error: Cannot connect to $SSH_HOST"
    exit 1
fi

echo "Checking Nix on remote..."
if ! ssh -i "$SSH_KEY" "$SSH_HOST" "command -v nix >/dev/null 2>&1 || test -f ~/.nix-profile/etc/profile.d/nix.sh"; then
    echo "Error: Nix not installed on remote server."
    echo ""
    echo "Install it:"
    echo "  ssh -i $SSH_KEY $SSH_HOST"
    echo "  curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install"
    exit 1
fi

ssh -i "$SSH_KEY" "$SSH_HOST" "bash -c '
    mkdir -p ~/.config/nix
    grep -q experimental-features ~/.config/nix/nix.conf 2>/dev/null || \
        echo \"experimental-features = nix-command flakes\" >> ~/.config/nix/nix.conf
'"

echo "Setting up remote build directory..."
ssh -i "$SSH_KEY" "$SSH_HOST" "mkdir -p $REMOTE_DIR && rm -rf $REMOTE_DIR/* $REMOTE_DIR/.* 2>/dev/null || true"

echo "Transferring files..."
rsync -avz --progress \
    -e "ssh -i $SSH_KEY" \
    --exclude '.git' \
    --exclude 'result*' \
    --exclude '*.iso' \
    --exclude 'target/' \
    --exclude '*.db' \
    --exclude '*.log' \
    "$LOCAL_DIR/" "$SSH_HOST:$REMOTE_DIR/"

echo ""
echo "Building ISO on remote server (this may take 30-60 minutes on first build)..."
echo ""

ssh -i "$SSH_KEY" "$SSH_HOST" "bash -c '
    mkdir -p /mnt/md126/tmp
    cd $REMOTE_DIR
    export TMPDIR=/mnt/md126/tmp
    export TEMP=/mnt/md126/tmp
    export TMP=/mnt/md126/tmp
    export NIX_CONFIG=\"experimental-features = nix-command flakes\"
    source ~/.nix-profile/etc/profile.d/nix.sh 2>/dev/null || true
    nix --extra-experimental-features nix-command \
        --extra-experimental-features flakes \
        --option sandbox false \
        build .#nixosConfigurations.kcore-iso.config.system.build.isoImage \
        -o result-iso 2>&1
'" | tee /tmp/kcore-iso-build.log

BUILD_EXIT=$?

if [ $BUILD_EXIT -eq 0 ]; then
    echo ""
    echo "======================================================"
    echo "  ISO build completed"
    echo "======================================================"
    echo ""

    ISO_PATH=$(ssh -i "$SSH_KEY" "$SSH_HOST" "ls $REMOTE_DIR/result-iso/iso/*.iso 2>/dev/null | head -1")

    if [ -z "$ISO_PATH" ]; then
        echo "Warning: Could not find ISO on remote server"
        ssh -i "$SSH_KEY" "$SSH_HOST" "ls -lh $REMOTE_DIR/result-iso/ 2>/dev/null || echo 'Directory not found'"
    else
        echo "Found: $ISO_PATH"
        ssh -i "$SSH_KEY" "$SSH_HOST" "ls -lh '$ISO_PATH'"
        echo ""
        echo "Downloading ISO..."
        scp -i "$SSH_KEY" "$SSH_HOST:$ISO_PATH" "$LOCAL_DIR/kcore.iso"
        echo ""
        echo "ISO downloaded: $LOCAL_DIR/kcore.iso"
        ls -lh "$LOCAL_DIR/kcore.iso"
    fi
else
    echo ""
    echo "Build failed. Check: tail -100 /tmp/kcore-iso-build.log"
    exit 1
fi
