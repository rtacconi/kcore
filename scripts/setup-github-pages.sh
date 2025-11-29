#!/bin/bash

# Quick setup script for GitHub Pages
# This script helps set up the website repository and GitHub Pages

set -e

ORG="kcore-systems"
REPO="website"
SOURCE_DIR="kcore-site"

echo "🚀 Setting up GitHub Pages for kcore.systems"
echo ""

# Check if GitHub token is set
if [ -z "$GITHUB_TOKEN" ]; then
    echo "❌ Error: GITHUB_TOKEN environment variable is not set"
    echo ""
    echo "Please set it with:"
    echo "  export GITHUB_TOKEN=your_token_here"
    echo ""
    echo "Get a token from: https://github.com/settings/tokens"
    echo "Required scopes: repo, workflow"
    exit 1
fi

# Check if source directory exists
if [ ! -d "$SOURCE_DIR" ]; then
    echo "❌ Error: Source directory '$SOURCE_DIR' not found"
    exit 1
fi

echo "📦 Step 1: Creating repository '$REPO' in organization '$ORG'..."
echo ""

# Create repository using the automation script
if [ -f "scripts/github-automation.sh" ]; then
    chmod +x scripts/github-automation.sh
    ./scripts/github-automation.sh setup-repo "$REPO" "kcore.systems website" "$SOURCE_DIR" main
else
    echo "⚠️  Automation script not found. Creating repository manually..."
    
    # Create repository via API
    response=$(curl -s -X POST \
        -H "Authorization: token $GITHUB_TOKEN" \
        -H "Accept: application/vnd.github.v3+json" \
        "https://api.github.com/orgs/$ORG/repos" \
        -d "{
            \"name\": \"$REPO\",
            \"description\": \"kcore.systems website\",
            \"private\": false,
            \"has_issues\": true,
            \"has_projects\": false,
            \"has_wiki\": false
        }")
    
    if echo "$response" | grep -q '"message"'; then
        echo "⚠️  Repository might already exist. Continuing..."
    else
        echo "✅ Repository created"
    fi
    
    # Clone and setup
    if [ ! -d "/tmp/$REPO" ]; then
        git clone "https://github.com/$ORG/$REPO.git" "/tmp/$REPO" 2>/dev/null || {
            echo "⚠️  Could not clone. Repository might not exist yet."
            echo "Please create it manually at: https://github.com/organizations/$ORG/repositories/new"
            exit 1
        }
    fi
    
    cd "/tmp/$REPO"
    cp -r "../../$SOURCE_DIR"/* .
    git add -A
    git commit -m "Initial commit: kcore.systems website" || true
    git push origin main || git push -u origin main
    
    # Enable GitHub Pages
    curl -s -X POST \
        -H "Authorization: token $GITHUB_TOKEN" \
        -H "Accept: application/vnd.github.v3+json" \
        "https://api.github.com/repos/$ORG/$REPO/pages" \
        -d '{
            "source": {
                "branch": "main",
                "path": "/"
            }
        }' > /dev/null
    
    echo "✅ GitHub Pages enabled"
    cd - > /dev/null
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "📝 Next steps:"
echo "1. Go to: https://github.com/$ORG/$REPO/settings/pages"
echo "2. Verify that GitHub Pages is enabled and using the 'main' branch"
echo "3. Your site will be available at: https://$ORG.github.io/$REPO"
echo "4. Or configure a custom domain: https://$ORG.github.io/$REPO/settings/pages"
echo ""
echo "🔧 To update the website:"
echo "  - Make changes in the '$SOURCE_DIR' directory"
echo "  - Push to main branch (GitHub Actions will deploy automatically)"
echo "  - Or use: ./scripts/github-automation.sh auto-commit 'your message'"

