#!/usr/bin/env bash
set -euo pipefail

echo "🚀 Creating VM on node..."
echo "Usage: Requires NODE_IP environment variable"
echo "Example: NODE_IP=192.168.40.146 make create-vm"

if [ -z "${NODE_IP:-}" ]; then
  echo "❌ Error: NODE_IP not set"
  exit 1
fi

VM_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
echo "Creating VM with ID: $VM_ID"

grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  -import-path ./proto -proto node.proto \
  -d "{\"spec\": {\"id\": \"$VM_ID\", \"name\": \"test-vm\", \"cpu\": 2, \"memory_bytes\": 2147483648}}" \
  "$NODE_IP:9091" kcore.node.NodeCompute/CreateVm

