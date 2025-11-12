#!/usr/bin/env bash
set -euo pipefail

echo "🔨 Building kcore ISO (this will take 10-30 minutes)..."
nix build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' \
  --extra-experimental-features 'nix-command flakes'

echo "✅ ISO built successfully!"
echo "📦 ISO location: result/iso/*.iso"
ls -lh result/iso/

