#!/bin/bash
# deploy-node-agent.sh - Deploy node agent to a ThinkCentre node

set -e

NODE="${1:-}"
USER="${2:-root}"
BINARY="${3:-bin/kcore-node-agent-linux-amd64}"

if [ -z "$NODE" ]; then
    echo "Usage: $0 <node-ip-or-hostname> [user] [binary-path]"
    echo "Example: $0 192.168.1.100 root bin/kcore-node-agent-linux-amd64"
    exit 1
fi

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY"
    echo "Build it first with: make node-agent"
    exit 1
fi

echo "Deploying node agent to $USER@$NODE..."
echo "Binary: $BINARY"

# Copy binary to temporary location
scp "$BINARY" "$USER@$NODE:/tmp/kcore-node-agent"

# Move to final location and restart service
ssh "$USER@$NODE" << 'EOF'
    set -e
    echo "Installing node agent..."
    sudo mkdir -p /opt/kcode
    sudo mv /tmp/kcore-node-agent /opt/kcode/kcore-node-agent
    sudo chmod +x /opt/kcode/kcore-node-agent
    sudo chown root:libvirt /opt/kcode/kcore-node-agent
    
    echo "Restarting kcode-node-agent service..."
    sudo systemctl daemon-reload
    sudo systemctl restart kcode-node-agent
    
    echo "Checking service status..."
    sudo systemctl status kcode-node-agent --no-pager || true
EOF

echo "Deployment complete!"

