# GitHub Automation Guide

This guide covers the GitHub automation setup for the kcore-systems organization, including GitHub Pages deployment and automated repository management.

## Overview

The automation system includes:
- **GitHub Pages Deployment**: Automatic deployment of the website to GitHub Pages
- **Automated Commits**: Scheduled or manual commits for maintenance tasks
- **Repository Management**: Scripts to create and manage repositories
- **Website Sync**: Automatic syncing of website files to a separate repository

## Quick Start

### 1. Set Up GitHub Token

Create a GitHub Personal Access Token with the following scopes:
- `repo` (full control of private repositories)
- `workflow` (update GitHub Action workflows)

```bash
export GITHUB_TOKEN=your_token_here
```

Get a token from: https://github.com/settings/tokens

### 2. Create Website Repository

Run the setup script to create the website repository and enable GitHub Pages:

```bash
./scripts/setup-github-pages.sh
```

This will:
- Create a new repository `website` in the `kcore-systems` organization
- Copy files from `kcore-site/` to the repository
- Enable GitHub Pages
- Set up automatic deployment

### 3. Configure GitHub Pages

After the repository is created:
1. Go to: https://github.com/kcore-systems/website/settings/pages
2. Verify that GitHub Pages is enabled
3. Optionally configure a custom domain (e.g., `kcore.systems`)

## GitHub Actions Workflows

### Deploy to GitHub Pages

**File**: `.github/workflows/deploy-pages.yml`

Automatically deploys the website to GitHub Pages when changes are pushed to the `main` branch in the `kcore-site/` directory.

**Triggers**:
- Push to `main` branch with changes in `kcore-site/`
- Manual workflow dispatch

**Features**:
- Automatic deployment to GitHub Pages
- No build step required (static site)
- Deploys from `kcore-site/` directory

### Automated Commit

**File**: `.github/workflows/auto-commit.yml`

Automates commits for scheduled maintenance tasks.

**Triggers**:
- Daily at 2 AM UTC (via cron)
- Manual workflow dispatch

**Usage**:
```bash
# Trigger manually via GitHub UI or API
gh workflow run auto-commit.yml -f message="chore: update dependencies"
```

### Website Sync

**File**: `.github/workflows/website-sync.yml`

Syncs website files to a separate repository for GitHub Pages deployment.

**Triggers**:
- Push to `main` branch with changes in `kcore-site/`
- Manual workflow dispatch

**Note**: Requires a separate `website` repository to exist.

## Automation Scripts

### GitHub Automation Script

**File**: `scripts/github-automation.sh`

A comprehensive script for managing GitHub repositories and operations.

#### Commands

**Create Repository**:
```bash
./scripts/github-automation.sh create-repo my-repo "Repository description" false
```

**Setup Repository with GitHub Pages**:
```bash
./scripts/github-automation.sh setup-repo website "kcore.systems website" ./kcore-site main
```

**Enable GitHub Pages**:
```bash
./scripts/github-automation.sh enable-pages website main
```

**Automate Commit**:
```bash
./scripts/github-automation.sh auto-commit "chore: automated update" main
```

**List Repositories**:
```bash
./scripts/github-automation.sh list-repos
```

### Setup GitHub Pages Script

**File**: `scripts/setup-github-pages.sh`

Quick setup script for initial GitHub Pages configuration.

```bash
export GITHUB_TOKEN=your_token
./scripts/setup-github-pages.sh
```

## Repository Structure

### Main Repository
- Contains the full kcore project
- Website files in `kcore-site/` directory
- GitHub Actions workflows in `.github/workflows/`

### Website Repository (Separate)
- Dedicated repository for GitHub Pages
- Automatically synced from main repository
- Deployed at: `https://kcore-systems.github.io/website`
- Or custom domain: `https://kcore.systems`

## Custom Domain Setup

To use a custom domain (e.g., `kcore.systems`):

1. **Configure DNS**:
   - Add a CNAME record pointing to `kcore-systems.github.io`
   - Or add A records for GitHub Pages IPs

2. **Configure GitHub Pages**:
   - Go to repository settings → Pages
   - Enter your custom domain
   - Enable "Enforce HTTPS"

3. **Update Website Files** (if needed):
   - Update any hardcoded URLs in the website files

## Manual Deployment

If you need to manually deploy:

```bash
# Clone the website repository
git clone https://github.com/kcore-systems/website.git
cd website

# Copy files from main repository
cp -r ../kcore-site/* .

# Commit and push
git add -A
git commit -m "chore: update website"
git push origin main
```

GitHub Pages will automatically deploy the changes.

## Troubleshooting

### GitHub Pages Not Deploying

1. Check repository settings:
   - Go to Settings → Pages
   - Verify source branch is set correctly
   - Check if custom domain is configured

2. Check GitHub Actions:
   - Go to Actions tab
   - Look for failed workflow runs
   - Check workflow logs

3. Verify file structure:
   - Ensure `index.html` exists in the root
   - Check file paths are correct

### Authentication Issues

If you get authentication errors:
- Verify `GITHUB_TOKEN` is set correctly
- Check token has required scopes
- Regenerate token if needed

### Repository Creation Fails

- Verify organization permissions
- Check if repository name already exists
- Ensure token has `repo` scope

## Best Practices

1. **Use GitHub Actions**: Let workflows handle deployments automatically
2. **Separate Repository**: Use a dedicated repository for GitHub Pages
3. **Custom Domain**: Use a custom domain for production
4. **HTTPS**: Always enable HTTPS enforcement
5. **Version Control**: Keep website files in version control
6. **Automated Testing**: Add tests for website changes (optional)

## Additional Resources

- [GitHub Pages Documentation](https://docs.github.com/en/pages)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [GitHub API Documentation](https://docs.github.com/en/rest)

## Support

For issues or questions:
- Check GitHub Actions logs
- Review workflow files
- Consult GitHub documentation
- Open an issue in the repository

