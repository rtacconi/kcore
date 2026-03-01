# kcorehypervisor.com Website

Static website for kcorehypervisor.com, hosted on GitHub Pages.

## Setup

This website is configured to be served from the `kcore-tech` directory on GitHub Pages.

### Automatic Setup (with GitHub Token)

```bash
./setup-github-pages.sh YOUR_GITHUB_TOKEN
```

### Manual GitHub Pages Configuration

1. Go to repository settings: https://github.com/rtacconi/kcore/settings/pages
2. Source: Deploy from a branch
3. Branch: `main` (or `master`)
4. Folder: `/kcore-tech`
5. Save

The website will be available at:
- https://rtacconi.github.io/kcore/ (GitHub Pages default)
- https://kcorehypervisor.com (after DNS is configured - see Custom Domain Setup below)

### Custom Domain Setup

To use the custom domain `kcorehypervisor.com`:

1. Add a `CNAME` file in the `kcore-tech` directory with content: `kcorehypervisor.com`
2. Configure DNS records:
   - Type: `CNAME`
   - Name: `@` (or root domain)
   - Value: `rtacconi.github.io`
   - Or use A records pointing to GitHub Pages IPs

## Local Development

Simply open `index.html` in a browser or use a local server:

```bash
cd kcore-tech
python3 -m http.server 8000
# or
npx serve .
```

Then visit http://localhost:8000
