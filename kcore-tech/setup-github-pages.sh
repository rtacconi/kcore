#!/usr/bin/env bash
# Setup script for GitHub Pages
# Usage: ./setup-github-pages.sh [GITHUB_TOKEN]

set -euo pipefail

GITHUB_TOKEN="${1:-}"
REPO="rtacconi/kcore"
BRANCH="main"
PATH_DIR="/kcore-tech"

if [ -z "$GITHUB_TOKEN" ]; then
    echo "Error: GitHub token required"
    echo "Usage: ./setup-github-pages.sh GITHUB_TOKEN"
    echo ""
    echo "Or set up manually:"
    echo "1. Go to https://github.com/$REPO/settings/pages"
    echo "2. Source: Deploy from a branch"
    echo "3. Branch: $BRANCH"
    echo "4. Folder: $PATH_DIR"
    echo "5. Save"
    exit 1
fi

echo "Configuring GitHub Pages for $REPO..."

# Enable GitHub Pages
RESPONSE=$(curl -s -w "\n%{http_code}" -X PUT \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    -H "Content-Type: application/json" \
    -d "{\"source\":{\"branch\":\"$BRANCH\",\"path\":\"$PATH_DIR\"}}" \
    "https://api.github.com/repos/$REPO/pages")

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ] || [ "$HTTP_CODE" -eq 201 ]; then
    echo "✅ GitHub Pages configured successfully!"
    echo ""
    echo "Your site will be available at:"
    echo "  - https://rtacconi.github.io/kcore/"
    echo "  - https://kcorehypervisor.com (after DNS is configured)"
    echo ""
    echo "Note: It may take a few minutes for the site to be live."
else
    echo "❌ Failed to configure GitHub Pages"
    echo "HTTP Status: $HTTP_CODE"
    echo "Response: $BODY"
    echo ""
    echo "Please set up manually:"
    echo "1. Go to https://github.com/$REPO/settings/pages"
    echo "2. Source: Deploy from a branch"
    echo "3. Branch: $BRANCH"
    echo "4. Folder: $PATH_DIR"
    echo "5. Save"
    exit 1
fi
