#!/bin/bash

# Simple development server script
# Choose the first available option

PORT=8000

echo "🚀 Starting kcore.ai development server on port $PORT..."
echo ""

# Check if we're in a devbox shell
if [ -n "$DEVBOX_SHELL_ENABLED" ]; then
    echo "✓ Running in devbox shell"
fi

# Try Python 3
if command -v python3 &> /dev/null; then
    echo "Using Python 3..."
    python3 -m http.server $PORT
    exit 0
fi

# Try Python 2
if command -v python &> /dev/null; then
    echo "Using Python 2..."
    python -m SimpleHTTPServer $PORT
    exit 0
fi

# Try PHP
if command -v php &> /dev/null; then
    echo "Using PHP..."
    php -S localhost:$PORT
    exit 0
fi

# Try Node.js with npx http-server
if command -v npx &> /dev/null; then
    echo "Using Node.js http-server..."
    npx http-server -p $PORT
    exit 0
fi

echo "❌ Error: No suitable server found."
echo ""
echo "This project uses devbox for environment management."
echo ""
echo "Please run one of the following:"
echo "  1. devbox run serve-site            (from project root)"
echo "  2. devbox shell && cd kcore-site && ./serve.sh"
echo ""
echo "Or install one of these manually:"
echo "  - Python 3 (recommended)"
echo "  - PHP"
echo "  - Node.js"
exit 1

