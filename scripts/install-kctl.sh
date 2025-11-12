#!/usr/bin/env bash
set -euo pipefail

echo "📦 Installing kctl..."

# Check if kctl binary exists
if [ ! -f ./bin/kctl ]; then
  echo "❌ Error: kctl binary not found. Run: make kctl"
  exit 1
fi

# Determine installation method
if [ "${1:-}" = "--local" ]; then
  # Install to user's local bin
  INSTALL_DIR="$HOME/.local/bin"
  mkdir -p "$INSTALL_DIR"
  cp ./bin/kctl "$INSTALL_DIR/kctl"
  chmod +x "$INSTALL_DIR/kctl"
  
  echo "✅ kctl installed to $INSTALL_DIR/kctl"
  echo ""
  echo "Add to your PATH by adding this to ~/.zshrc or ~/.bashrc:"
  echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
  echo ""
  echo "Then run: source ~/.zshrc"
  
elif [ "${1:-}" = "--system" ]; then
  # Install to system bin (requires sudo)
  INSTALL_DIR="/usr/local/bin"
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  sudo cp ./bin/kctl "$INSTALL_DIR/kctl"
  sudo chmod +x "$INSTALL_DIR/kctl"
  
  echo "✅ kctl installed to $INSTALL_DIR/kctl"
  echo "Available globally as 'kctl'"
  
else
  # Show usage
  echo "Usage: $0 [--local|--system]"
  echo ""
  echo "Options:"
  echo "  --local   Install to ~/.local/bin (no sudo required)"
  echo "  --system  Install to /usr/local/bin (requires sudo)"
  echo ""
  echo "Or run directly:"
  echo "  ./bin/kctl [command]"
  exit 1
fi

