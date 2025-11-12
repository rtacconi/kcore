# kcore.ai - Marketing Website

A modern, dark-themed marketing website for kcore - the AI-powered virtualization platform.

## Overview

This is a static website showcasing kcore's features and capabilities:

- **Atomic Updates & Rollback**: Immutable infrastructure with instant rollback
- **Declarative Infrastructure**: Full Terraform integration
- **AI-Powered Agent**: MCP server for natural language infrastructure management
- **Built-in Automation**: Intelligent orchestration and automation
- **Modern Architecture**: gRPC APIs, multi-node orchestration, flexible storage

## Tech Stack

- **HTML5**: Semantic, accessible markup
- **Modern CSS**: Custom properties, Grid, Flexbox, animations
- **Vanilla JavaScript**: No framework dependencies, lightweight and fast
- **Dark Theme**: Inspired by modern tech platforms (Heroku, Railway, Vercel)

## Features

### Design
- Fully responsive design
- Dark mode optimized
- Smooth animations and transitions
- Interactive code examples
- Tab-based content navigation

### Functionality
- Smooth scrolling navigation
- Copy-to-clipboard for install commands
- Keyboard navigation support
- Intersection Observer for scroll animations
- Parallax effects
- Performance optimized

## File Structure

```
kcore-site/
├── index.html          # Main landing page
├── css/
│   └── styles.css      # All styles (dark theme)
├── js/
│   └── main.js         # Interactive functionality
└── images/             # Image assets (when needed)
```

## Development

### Local Development

This project uses [devbox](https://www.jetpack.io/devbox/) for environment management.

#### Quickest Way (Recommended)

From the project root:

```bash
devbox run serve-site
```

#### Alternative Methods

```bash
# Using devbox shell
devbox shell
cd kcore-site
./serve.sh

# Or manually with Python
devbox shell
cd kcore-site
python3 -m http.server 8000
```

Then visit `http://localhost:8000` in your browser.

See [DEVBOX_SETUP.md](DEVBOX_SETUP.md) for more details.

### Customization

**Colors**: Edit CSS custom properties in `styles.css`:
```css
:root {
    --accent-primary: #6366f1;
    --accent-secondary: #8b5cf6;
    /* ... more variables */
}
```

**Content**: Edit `index.html` directly. All content is in semantic HTML.

**Functionality**: Modify `main.js` for interactive behaviors.

## Browser Support

- Chrome/Edge (latest)
- Firefox (latest)
- Safari (latest)
- Mobile browsers (iOS Safari, Chrome Mobile)

## Performance

- No external dependencies
- Minimal JavaScript
- CSS animations hardware-accelerated
- Lazy loading ready
- Optimized for Core Web Vitals

## Deployment

### Static Hosting

Deploy to any static hosting service:

- **Vercel**: `vercel --prod`
- **Netlify**: Drag and drop or connect to git
- **GitHub Pages**: Push to gh-pages branch
- **Cloudflare Pages**: Connect repository
- **AWS S3 + CloudFront**: Upload files
- **Nginx/Apache**: Copy files to web root

### Example Nginx Config

```nginx
server {
    listen 80;
    server_name kcore.ai www.kcore.ai;
    root /var/www/kcore-site;
    index index.html;

    location / {
        try_files $uri $uri/ =404;
    }

    # Cache static assets
    location ~* \.(css|js|jpg|jpeg|png|gif|ico|svg)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }
}
```

## SEO Optimization

- Semantic HTML5 markup
- Meta descriptions included
- Proper heading hierarchy
- Alt text ready for images
- Structured data ready
- Mobile-friendly
- Fast loading times

## Future Enhancements

- [ ] Add blog section
- [ ] Interactive demo/playground
- [ ] Documentation pages
- [ ] API reference
- [ ] Case studies/testimonials
- [ ] Video content
- [ ] Multi-language support
- [ ] Dark/light theme toggle

## License

Part of the kcore project.

## Contact

For questions about kcore, visit the main repository or documentation.

