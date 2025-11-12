# Quick Start Guide

Get the kcore.ai marketing site running locally in 30 seconds.

## Start Local Server

This project uses devbox for environment management.

### Option 1: Using devbox script (easiest)
```bash
# From the project root
devbox run serve-site
```

### Option 2: Using devbox shell
```bash
devbox shell
cd kcore-site
./serve.sh
```

### Option 3: Direct Python (in devbox shell)
```bash
devbox shell
cd kcore-site
python3 -m http.server 8000
```

> **Note**: Python 3 is provided by devbox. Make sure you're in a devbox shell or use `devbox run`.

## View the Site

Open your browser and go to:
```
http://localhost:8000
```

## File Structure

```
kcore-site/
├── index.html          # Main landing page
├── features.html       # Detailed features page
├── docs.html          # Documentation page
├── css/
│   └── styles.css     # All styles (dark theme)
├── js/
│   └── main.js        # Interactive functionality
├── images/            # Image assets (currently empty)
├── README.md          # Detailed documentation
├── DEPLOYMENT.md      # Deployment guide
└── serve.sh           # Development server script
```

## Making Changes

1. Edit HTML files directly in the `kcore-site` directory
2. Modify styles in `css/styles.css`
3. Update functionality in `js/main.js`
4. Refresh your browser to see changes

## Deploy to Production

See [DEPLOYMENT.md](DEPLOYMENT.md) for detailed deployment instructions for:
- Vercel (recommended)
- Netlify
- GitHub Pages
- Cloudflare Pages
- Self-hosted options

## Quick Deploy

```bash
# Vercel
vercel --prod

# Netlify
netlify deploy --prod
```

## Key Features

✅ Modern dark theme  
✅ Fully responsive design  
✅ No build process required  
✅ No dependencies  
✅ SEO optimized  
✅ Fast loading times  
✅ Accessibility focused  

## Need Help?

- Check [README.md](README.md) for detailed information
- See [DEPLOYMENT.md](DEPLOYMENT.md) for hosting options
- Review the HTML/CSS/JS files for implementation details

---

**Ready to customize?** Start editing `index.html` and see changes instantly!

