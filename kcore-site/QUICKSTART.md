# Quick Start Guide

Get the kcore.ai marketing site running locally in 30 seconds.

## Start Local Server

### Option 1: Using the serve script (easiest)
```bash
cd kcore-site
./serve.sh
```

### Option 2: Python
```bash
cd kcore-site
python3 -m http.server 8000
```

### Option 3: Node.js
```bash
cd kcore-site
npx http-server -p 8000
```

### Option 4: PHP
```bash
cd kcore-site
php -S localhost:8000
```

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

