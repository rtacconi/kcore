#!/usr/bin/env bash
set -euo pipefail

echo "🗑️  Deleting VM from node..."
echo "Usage: Requires NODE_IP and VM_ID environment variables"
echo "Example: NODE_IP=192.168.40.146 VM_ID=<uuid> make delete-vm"

if [ -z "${NODE_IP:-}" ]; then
  echo "❌ Error: NODE_IP not set"
  exit 1
fi

if [ -z "${VM_ID:-}" ]; then
  echo "❌ Error: VM_ID not set"
  exit 1
fi

grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  -import-path ./proto -proto node.proto \
  -d "{\"id\": \"$VM_ID\"}" \
  "$NODE_IP:9091" kcore.node.NodeCompute/DeleteVm

