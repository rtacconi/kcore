#!/usr/bin/env bash
set -euo pipefail

echo "📦 Deploying node-agent and certs to node..."
echo "Usage: Requires NODE_IP environment variable"
echo "Example: NODE_IP=192.168.40.146 make deploy-node"

if [ -z "${NODE_IP:-}" ]; then
  echo "❌ Error: NODE_IP not set"
  exit 1
fi

echo "Creating directories on node..."
ssh "root@$NODE_IP" 'mkdir -p /opt/kcore/bin /etc/kcore'

echo "Copying node-agent binary..."
if [ -f ./result/bin/kcore-node-agent ]; then
  scp ./result/bin/kcore-node-agent "root@$NODE_IP:/opt/kcore/bin/"
else
  echo "❌ node-agent binary not found. Run: make build-node-agent"
  exit 1
fi

echo "Copying certificates..."
scp certs/*.crt certs/*.key "root@$NODE_IP:/etc/kcore/"

echo "Copying config..."
scp examples/node-agent.yaml "root@$NODE_IP:/etc/kcore/"

echo "✅ Deployment complete!"
echo "Restart node-agent: ssh root@$NODE_IP systemctl restart kcore-node-agent"

