#!/usr/bin/env bash
# Configure GitHub Pages via API
# Usage: ./configure-pages.sh [GITHUB_TOKEN]

set -euo pipefail

GITHUB_TOKEN="${1:-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}"
REPO="rtacconi/kcore"
BRANCH="main"
PATH_DIR="/kcore-tech"

echo "Configuring GitHub Pages for $REPO..."
echo "Branch: $BRANCH"
echo "Folder: $PATH_DIR"
echo ""

# First, verify token
echo "Verifying GitHub token..."
USER_RESPONSE=$(curl -s -w "\n%{http_code}" \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    "https://api.github.com/user")

HTTP_CODE=$(echo "$USER_RESPONSE" | tail -n1)
USER_BODY=$(echo "$USER_RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -ne 200 ]; then
    echo "❌ Token authentication failed (HTTP $HTTP_CODE)"
    echo "Response: $USER_BODY"
    echo ""
    echo "The token appears to be invalid or expired."
    echo ""
    echo "Please either:"
    echo "1. Generate a new token at: https://github.com/settings/tokens"
    echo "   - Select 'repo' scope (full repository access)"
    echo "   - Run: ./configure-pages.sh YOUR_NEW_TOKEN"
    echo ""
    echo "2. Or set up manually:"
    echo "   - Go to: https://github.com/$REPO/settings/pages"
    echo "   - Source: Deploy from a branch"
    echo "   - Branch: $BRANCH"
    echo "   - Folder: $PATH_DIR"
    echo "   - Save"
    exit 1
fi

USER_LOGIN=$(echo "$USER_BODY" | grep -o '"login":"[^"]*' | cut -d'"' -f4)
echo "✅ Authenticated as: $USER_LOGIN"
echo ""

# Configure Pages
echo "Configuring GitHub Pages..."
PAGES_RESPONSE=$(curl -s -w "\n%{http_code}" -X PUT \
    -H "Authorization: token $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    -H "Content-Type: application/json" \
    -d "{\"source\":{\"branch\":\"$BRANCH\",\"path\":\"$PATH_DIR\"}}" \
    "https://api.github.com/repos/$REPO/pages")

PAGES_HTTP_CODE=$(echo "$PAGES_RESPONSE" | tail -n1)
PAGES_BODY=$(echo "$PAGES_RESPONSE" | sed '$d')

if [ "$PAGES_HTTP_CODE" -eq 200 ] || [ "$PAGES_HTTP_CODE" -eq 201 ]; then
    echo "✅ GitHub Pages configured successfully!"
    echo ""
    echo "Your site will be available at:"
    echo "  - https://rtacconi.github.io/kcore/"
    echo "  - https://kcorehypervisor.com (after DNS is configured)"
    echo ""
    echo "Note: It may take a few minutes for the site to be live."
    echo ""
    echo "Next steps:"
    echo "1. Wait for deployment (check Actions tab)"
    echo "2. Configure DNS for kcorehypervisor.com to point to GitHub Pages"
    echo "3. Add custom domain in Settings → Pages → Custom domain"
else
    echo "❌ Failed to configure GitHub Pages"
    echo "HTTP Status: $PAGES_HTTP_CODE"
    echo "Response: $PAGES_BODY"
    echo ""
    echo "Please set up manually:"
    echo "1. Go to: https://github.com/$REPO/settings/pages"
    echo "2. Source: Deploy from a branch"
    echo "3. Branch: $BRANCH"
    echo "4. Folder: $PATH_DIR"
    echo "5. Save"
    exit 1
fi
