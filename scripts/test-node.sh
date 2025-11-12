#!/usr/bin/env bash
set -euo pipefail

echo "🔍 Testing node connectivity..."
echo "Usage: Requires NODE_IP environment variable"

if [ -z "${NODE_IP:-}" ]; then
  echo "❌ Error: NODE_IP not set"
  exit 1
fi

echo "Testing TCP connection to port 9091..."
if nc -zv "$NODE_IP" 9091 2>&1; then
  echo "✅ TCP connection successful"
else
  echo "❌ Cannot connect to node"
  exit 1
fi

echo "Testing gRPC service list..."
if grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  "$NODE_IP:9091" list 2>&1; then
  echo "✅ gRPC service responding"
else
  echo "❌ gRPC service not responding"
  exit 1
fi

