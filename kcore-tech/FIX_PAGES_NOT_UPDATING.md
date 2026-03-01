# Fix: Website Not Showing Updates

## Problem
The website at https://kcorehypervisor.com is showing the old Jekyll site instead of the new static site from `/kcore-tech` directory.

## Root Cause
GitHub Pages is configured to serve from the root directory (`/`) instead of the `/kcore-tech` directory.

## Solution: Configure GitHub Pages Source

### Step 1: Go to GitHub Pages Settings
1. Navigate to: **https://github.com/rtacconi/kcore/settings/pages**
2. Scroll down to the **"Source"** section

### Step 2: Change Source Directory
1. Under **"Source"**, select: **"Deploy from a branch"**
2. **Branch**: Select `main` (or `master` if that's your default)
3. **Folder**: Select **`/kcore-tech`** (this is the important part!)
4. Click **"Save"**

### Step 3: Wait for Deployment
- GitHub will automatically rebuild and deploy
- This usually takes 1-5 minutes
- You can check the deployment status in the **Actions** tab

### Step 4: Clear Browser Cache
After deployment completes:
- Hard refresh: `Cmd+Shift+R` (Mac) or `Ctrl+Shift+R` (Windows/Linux)
- Or open in incognito/private window
- Or clear browser cache

## Verify It's Working

After configuration, the site should show:
- ✅ Title: "kcore hypervisor" (not just "kcore")
- ✅ Dark mode by default (black background)
- ✅ Theme toggle button in the navbar (sun/moon icon)
- ✅ Modern Railway.com-inspired design
- ✅ Updated styling and layout

## Alternative: Move Files to Root

If you prefer to keep the site in the root directory:

```bash
# Move kcore-tech files to root
cd /Users/riccardotacconi/dev/kcore
mv kcore-tech/* .
mv kcore-tech/.* . 2>/dev/null || true
rmdir kcore-tech
git add .
git commit -m "Move website files to root directory"
git push
```

Then configure GitHub Pages to serve from `/` (root) instead of `/kcore-tech`.

## Current Status

- ✅ Files are correct in `/kcore-tech` directory
- ✅ All changes are committed and pushed
- ❌ GitHub Pages source directory needs to be changed to `/kcore-tech`

Once you change the source directory in GitHub settings, the new site will appear!
