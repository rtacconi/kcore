#!/bin/bash

# GitHub Automation Script for kcore-systems organization
# This script automates repository creation, commits, and GitHub Pages setup

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
ORG="kcore-systems"
GITHUB_API="https://api.github.com"

# Check if GitHub token is set
if [ -z "$GITHUB_TOKEN" ]; then
    echo -e "${RED}Error: GITHUB_TOKEN environment variable is not set${NC}"
    echo "Please set it with: export GITHUB_TOKEN=your_token_here"
    echo "Get a token from: https://github.com/settings/tokens"
    exit 1
fi

# Function to create a new repository
create_repo() {
    local repo_name=$1
    local description=$2
    local private=${3:-false}
    
    echo -e "${YELLOW}Creating repository: $repo_name${NC}"
    
    response=$(curl -s -X POST \
        -H "Authorization: token $GITHUB_TOKEN" \
        -H "Accept: application/vnd.github.v3+json" \
        "$GITHUB_API/orgs/$ORG/repos" \
        -d "{
            \"name\": \"$repo_name\",
            \"description\": \"$description\",
            \"private\": $private,
            \"auto_init\": false,
            \"has_issues\": true,
            \"has_projects\": true,
            \"has_wiki\": false
        }")
    
    if echo "$response" | grep -q '"message"'; then
        echo -e "${RED}Error creating repository:${NC}"
        echo "$response" | jq -r '.message' 2>/dev/null || echo "$response"
        return 1
    else
        echo -e "${GREEN}Repository created successfully: https://github.com/$ORG/$repo_name${NC}"
        echo "$response" | jq -r '.clone_url' 2>/dev/null
        return 0
    fi
}

# Function to enable GitHub Pages
enable_pages() {
    local repo_name=$1
    local branch=${2:-main}
    
    echo -e "${YELLOW}Enabling GitHub Pages for: $repo_name${NC}"
    
    response=$(curl -s -X POST \
        -H "Authorization: token $GITHUB_TOKEN" \
        -H "Accept: application/vnd.github.v3+json" \
        "$GITHUB_API/repos/$ORG/$repo_name/pages" \
        -d "{
            \"source\": {
                \"branch\": \"$branch\",
                \"path\": \"/\"
            }
        }")
    
    if echo "$response" | grep -q '"message"'; then
        echo -e "${YELLOW}Warning:${NC}"
        echo "$response" | jq -r '.message' 2>/dev/null || echo "$response"
    else
        echo -e "${GREEN}GitHub Pages enabled for: $repo_name${NC}"
        echo "Site will be available at: https://$ORG.github.io/$repo_name"
    fi
}

# Function to create and push to repository
setup_repo() {
    local repo_name=$1
    local description=$2
    local source_dir=$3
    local branch=${4:-main}
    
    echo -e "${YELLOW}Setting up repository: $repo_name${NC}"
    
    # Create repository
    clone_url=$(create_repo "$repo_name" "$description")
    if [ $? -ne 0 ]; then
        return 1
    fi
    
    # Clone the repository
    if [ -d "/tmp/$repo_name" ]; then
        rm -rf "/tmp/$repo_name"
    fi
    
    git clone "$clone_url" "/tmp/$repo_name"
    cd "/tmp/$repo_name"
    
    # Copy source files
    if [ -n "$source_dir" ] && [ -d "$source_dir" ]; then
        cp -r "$source_dir"/* .
        git add -A
        git commit -m "Initial commit: $description" || true
        git push -u origin "$branch"
    fi
    
    # Enable GitHub Pages
    enable_pages "$repo_name" "$branch"
    
    echo -e "${GREEN}Repository setup complete!${NC}"
    echo "Repository: https://github.com/$ORG/$repo_name"
    echo "Pages: https://$ORG.github.io/$repo_name"
    
    cd - > /dev/null
    rm -rf "/tmp/$repo_name"
}

# Function to automate commits to existing repository
auto_commit() {
    local repo_name=$1
    local message=${2:-"chore: automated update"}
    local branch=${3:-main}
    
    echo -e "${YELLOW}Automating commit to: $repo_name${NC}"
    
    # Check if repository exists locally
    if [ ! -d ".git" ]; then
        echo -e "${RED}Error: Not in a git repository${NC}"
        return 1
    fi
    
    # Check for changes
    if [ -z "$(git status --porcelain)" ]; then
        echo -e "${YELLOW}No changes to commit${NC}"
        return 0
    fi
    
    # Add all changes
    git add -A
    
    # Commit
    git commit -m "$message" || {
        echo -e "${YELLOW}No changes to commit${NC}"
        return 0
    }
    
    # Push
    git push origin "$branch"
    
    echo -e "${GREEN}Commit and push completed${NC}"
}

# Function to list all repositories in the organization
list_repos() {
    echo -e "${YELLOW}Listing repositories in $ORG:${NC}"
    
    curl -s -X GET \
        -H "Authorization: token $GITHUB_TOKEN" \
        -H "Accept: application/vnd.github.v3+json" \
        "$GITHUB_API/orgs/$ORG/repos?per_page=100" | \
        jq -r '.[] | "\(.name) - \(.description // "No description")"' 2>/dev/null || {
        echo -e "${RED}Error listing repositories${NC}"
        return 1
    }
}

# Main menu
show_help() {
    cat << EOF
GitHub Automation Script for kcore-systems

Usage: $0 [command] [options]

Commands:
  create-repo <name> <description> [private]
    Create a new repository in the kcore-systems organization
    Example: $0 create-repo my-repo "My repository description" false

  setup-repo <name> <description> <source-dir> [branch]
    Create repository, copy files, and enable GitHub Pages
    Example: $0 setup-repo website "kcore.systems website" ./kcore-site main

  enable-pages <name> [branch]
    Enable GitHub Pages for an existing repository
    Example: $0 enable-pages website main

  auto-commit [message] [branch]
    Automate commit and push in current repository
    Example: $0 auto-commit "chore: update content" main

  list-repos
    List all repositories in the organization

Environment Variables:
  GITHUB_TOKEN    - GitHub personal access token (required)

Examples:
  # Create a new repository
  export GITHUB_TOKEN=your_token
  $0 create-repo my-project "Project description"

  # Setup website repository with GitHub Pages
  $0 setup-repo website "kcore.systems website" ./kcore-site

  # Enable GitHub Pages for existing repo
  $0 enable-pages website

  # Automate commit in current repo
  $0 auto-commit "chore: automated update"
EOF
}

# Parse command line arguments
case "${1:-help}" in
    create-repo)
        if [ -z "$2" ] || [ -z "$3" ]; then
            echo -e "${RED}Error: Repository name and description required${NC}"
            exit 1
        fi
        create_repo "$2" "$3" "${4:-false}"
        ;;
    
    setup-repo)
        if [ -z "$2" ] || [ -z "$3" ] || [ -z "$4" ]; then
            echo -e "${RED}Error: Repository name, description, and source directory required${NC}"
            exit 1
        fi
        setup_repo "$2" "$3" "$4" "${5:-main}"
        ;;
    
    enable-pages)
        if [ -z "$2" ]; then
            echo -e "${RED}Error: Repository name required${NC}"
            exit 1
        fi
        enable_pages "$2" "${3:-main}"
        ;;
    
    auto-commit)
        auto_commit "." "${2:-chore: automated update}" "${3:-main}"
        ;;
    
    list-repos)
        list_repos
        ;;
    
    help|--help|-h)
        show_help
        ;;
    
    *)
        echo -e "${RED}Unknown command: $1${NC}"
        show_help
        exit 1
        ;;
esac

