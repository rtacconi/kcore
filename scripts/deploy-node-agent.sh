#!/bin/bash
set -e

NODE_HOST="${1:-root@192.168.40.146}"
BINARY_PATH="${2:-/tmp/kcore-node-agent-fixed}"

echo "🚀 Deploying node agent to $NODE_HOST..."

# Stop existing agent (run in subshell to avoid killing SSH)
echo "📋 Stopping existing agent..."
ssh "$NODE_HOST" '(pkill -f kcore-node-agent || true) &'
sleep 3

# Deploy new binary
echo "📦 Deploying new binary..."
ssh "$NODE_HOST" "cp $BINARY_PATH /usr/local/bin/kcore-node-agent && chmod +x /usr/local/bin/kcore-node-agent"

# Start agent in background with nohup
echo "🔄 Starting agent in background..."
ssh "$NODE_HOST" 'nohup /usr/local/bin/kcore-node-agent >/tmp/node-agent.log 2>&1 </dev/null &'

# Wait a moment for startup
sleep 2

# Verify it's running
echo "✅ Verifying agent is running..."
if ssh "$NODE_HOST" 'pgrep -f kcore-node-agent >/dev/null'; then
    echo "✅ Node agent is running!"
    ssh "$NODE_HOST" 'ps aux | grep "[k]core-node-agent"'
    echo ""
    echo "📋 Last 5 log lines:"
    ssh "$NODE_HOST" 'tail -5 /tmp/node-agent.log'
else
    echo "❌ Node agent failed to start"
    echo "📋 Log output:"
    ssh "$NODE_HOST" 'tail -20 /tmp/node-agent.log'
    exit 1
fi

echo ""
echo "✅ Deployment complete!"
