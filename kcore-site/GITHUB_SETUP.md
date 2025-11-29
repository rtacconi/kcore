# GitHub Pages Setup for kcore.systems

Quick guide to set up and deploy the website to GitHub Pages.

## Prerequisites

1. GitHub Personal Access Token with `repo` and `workflow` scopes
2. Access to the `kcore-systems` organization

## Quick Setup

```bash
# 1. Set your GitHub token
export GITHUB_TOKEN=your_token_here

# 2. Run the setup script
./scripts/setup-github-pages.sh
```

This will:
- Create a `website` repository in the `kcore-systems` organization
- Copy website files to the repository
- Enable GitHub Pages
- Set up automatic deployment

## Manual Setup

### Option 1: Deploy from Main Repository

The website can be deployed directly from the main repository using GitHub Actions:

1. Go to repository Settings → Pages
2. Select "GitHub Actions" as the source
3. The workflow will automatically deploy when you push changes to `kcore-site/`

### Option 2: Separate Repository

Create a dedicated repository for the website:

```bash
# Create repository
./scripts/github-automation.sh setup-repo website "kcore.systems website" ./kcore-site main

# Or manually:
# 1. Create repository at: https://github.com/kcore-systems/website
# 2. Clone and copy files
# 3. Enable GitHub Pages in settings
```

## Custom Domain

To use `kcore.systems` as the domain:

1. **DNS Configuration**:
   - Add CNAME record: `kcore.systems` → `kcore-systems.github.io`
   - Or A records pointing to GitHub Pages IPs

2. **GitHub Configuration**:
   - Go to repository Settings → Pages
   - Enter `kcore.systems` in the custom domain field
   - Enable "Enforce HTTPS"

## Updating the Website

### Automatic (Recommended)

Just push changes to the `kcore-site/` directory:

```bash
# Make your changes
cd kcore-site
# ... edit files ...

# Commit and push
cd ..
git add kcore-site/
git commit -m "docs: update website"
git push origin main
```

GitHub Actions will automatically deploy the changes.

### Manual Update

If using a separate repository:

```bash
cd website  # or wherever the website repo is
cp -r ../kcore-site/* .
git add -A
git commit -m "chore: update website"
git push origin main
```

## Verification

After deployment, check:
- GitHub Actions: https://github.com/kcore-systems/kcore/actions
- Website: https://kcore-systems.github.io/website (or your custom domain)
- Pages Settings: Repository Settings → Pages

## Troubleshooting

**Pages not deploying?**
- Check GitHub Actions logs
- Verify Pages is enabled in settings
- Check file paths are correct

**Custom domain not working?**
- Verify DNS records
- Check domain is configured in GitHub Pages settings
- Wait for DNS propagation (can take up to 24 hours)

For more details, see: [docs/GITHUB_AUTOMATION.md](../docs/GITHUB_AUTOMATION.md)

