# GitHub Pages Setup Guide

## Quick Setup (Manual - Recommended)

Since the GitHub token authentication failed, please follow these manual steps:

### Step 1: Enable GitHub Pages

1. Navigate to: **https://github.com/rtacconi/kcore/settings/pages**

2. Under **"Source"** section:
   - Select: **"Deploy from a branch"**
   - Branch: **`main`** (or `master` if that's your default branch)
   - Folder: **`/kcore-tech`**
   - Click **"Save"**

### Step 2: Verify Setup

After saving, GitHub Pages will:
- Build and deploy your site
- Show a green checkmark when deployment is complete
- Provide the site URL: `https://rtacconi.github.io/kcore/`

### Step 3: Custom Domain (kcorehypervisor.com)

The `CNAME` file is already in place. After GitHub Pages is enabled:

1. **Wait for initial deployment** (2-5 minutes)

2. **Configure DNS** at your domain registrar:
   - **Type**: `CNAME`
   - **Name**: `@` (or root domain)
   - **Value**: `rtacconi.github.io`
   - **TTL**: 3600 (or default)

   OR use A records:
   - **Type**: `A`
   - **Name**: `@`
   - **Value**: `185.199.108.153`, `185.199.109.153`, `185.199.110.153`, `185.199.111.153`

3. **Verify in GitHub**:
   - Go back to Settings → Pages
   - Under "Custom domain", enter: `kcorehypervisor.com`
   - Check "Enforce HTTPS" (after DNS propagates)

### Step 4: Test

- GitHub Pages URL: https://rtacconi.github.io/kcore/
- Custom domain: https://kcorehypervisor.com (after DNS propagates, 5-60 minutes)

---

## Alternative: Using GitHub CLI

If you have GitHub CLI installed and authenticated:

```bash
gh api repos/rtacconi/kcore/pages \
  -X PUT \
  -f source[branch]=main \
  -f source[path]=/kcore-tech
```

---

## Troubleshooting

### Token Issues
If you need to use a token, ensure it has:
- `repo` scope (full repository access)
- Or `public_repo` scope for public repositories

Generate a new token at: https://github.com/settings/tokens

### Deployment Issues
- Check Actions tab for build errors
- Ensure all files are committed and pushed
- Verify the `/kcore-tech` folder exists in the repository

### Custom Domain Issues
- DNS propagation can take up to 48 hours (usually 5-60 minutes)
- Verify DNS with: `dig kcorehypervisor.com` or `nslookup kcorehypervisor.com`
- Ensure CNAME file is in the `/kcore-tech` directory
