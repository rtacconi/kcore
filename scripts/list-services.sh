#!/usr/bin/env bash
set -euo pipefail

echo "📋 Listing gRPC services on node..."
echo "Usage: Requires NODE_IP environment variable"

if [ -z "${NODE_IP:-}" ]; then
  echo "❌ Error: NODE_IP not set"
  exit 1
fi

grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  "$NODE_IP:9091" list

