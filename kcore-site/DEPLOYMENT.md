# kcore.systems Deployment Guide

This guide covers deploying the kcore.systems marketing website to various hosting platforms.

## Quick Deploy Options

### Option 1: Vercel (Recommended)

Vercel offers free static hosting with automatic SSL and global CDN.

```bash
# Install Vercel CLI
npm i -g vercel

# Deploy from the kcore-site directory
cd kcore-site
vercel

# For production deployment
vercel --prod
```

Or use the Vercel web interface:
1. Visit [vercel.com](https://vercel.com)
2. Import your Git repository
3. Set build settings:
   - Framework Preset: Other
   - Root Directory: `kcore-site`
   - Build Command: (leave empty)
   - Output Directory: `.`

### Option 2: Netlify

```bash
# Install Netlify CLI
npm install -g netlify-cli

# Deploy
cd kcore-site
netlify deploy

# Production deployment
netlify deploy --prod
```

Or drag-and-drop deployment:
1. Visit [netlify.com](https://netlify.com)
2. Drag the `kcore-site` folder into the deploy area
3. Done!

### Option 3: GitHub Pages

1. Create a new repository or use an existing one
2. Push the `kcore-site` contents to the `gh-pages` branch:

```bash
cd kcore-site
git init
git add .
git commit -m "Initial kcore.systems website"
git branch -M gh-pages
git remote add origin https://github.com/yourusername/kcore-site.git
git push -u origin gh-pages
```

3. Enable GitHub Pages in repository settings
4. Select `gh-pages` branch as source

### Option 4: Cloudflare Pages

1. Visit [pages.cloudflare.com](https://pages.cloudflare.com)
2. Connect your Git repository
3. Configure build settings:
   - Build command: (leave empty)
   - Build output directory: `/kcore-site`
4. Deploy

## Self-Hosted Options

### Nginx

```nginx
server {
    listen 80;
    listen [::]:80;
    server_name kcore.systems www.kcore.systems;

    # Redirect to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name kcore.systems www.kcore.systems;

    # SSL Configuration
    ssl_certificate /etc/letsencrypt/live/kcore.systems/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/kcore.systems/privkey.pem;

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;

    root /var/www/kcore-site;
    index index.html;

    location / {
        try_files $uri $uri/ =404;
    }

    # Cache static assets
    location ~* \.(css|js|jpg|jpeg|png|gif|ico|svg|woff|woff2|ttf|eot)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }

    # Security: Prevent access to hidden files
    location ~ /\. {
        deny all;
    }
}
```

### Apache

```apache
<VirtualHost *:80>
    ServerName kcore.systems
    ServerAlias www.kcore.systems
    
    DocumentRoot /var/www/kcore-site
    
    <Directory /var/www/kcore-site>
        Options -Indexes +FollowSymLinks
        AllowOverride All
        Require all granted
    </Directory>
    
    # Redirect to HTTPS
    RewriteEngine On
    RewriteCond %{HTTPS} off
    RewriteRule ^(.*)$ https://%{HTTP_HOST}%{REQUEST_URI} [L,R=301]
</VirtualHost>

<VirtualHost *:443>
    ServerName kcore.systems
    ServerAlias www.kcore.systems
    
    DocumentRoot /var/www/kcore-site
    
    SSLEngine on
    SSLCertificateFile /etc/letsencrypt/live/kcore.systems/fullchain.pem
    SSLCertificateKeyFile /etc/letsencrypt/live/kcore.systems/privkey.pem
    
    <Directory /var/www/kcore-site>
        Options -Indexes +FollowSymLinks
        AllowOverride All
        Require all granted
    </Directory>
    
    # Cache control
    <FilesMatch "\.(css|js|jpg|jpeg|png|gif|ico|svg)$">
        Header set Cache-Control "max-age=31536000, public, immutable"
    </FilesMatch>
</VirtualHost>
```

### Docker

Create a `Dockerfile`:

```dockerfile
FROM nginx:alpine

# Copy website files
COPY . /usr/share/nginx/html

# Copy custom nginx config (optional)
# COPY nginx.conf /etc/nginx/nginx.conf

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
```

Build and run:

```bash
cd kcore-site
docker build -t kcore-site .
docker run -d -p 80:80 kcore-site
```

### AWS S3 + CloudFront

```bash
# Install AWS CLI
pip install awscli

# Configure AWS credentials
aws configure

# Create S3 bucket
aws s3 mb s3://kcore.systems

# Enable static website hosting
aws s3 website s3://kcore.systems \
    --index-document index.html \
    --error-document index.html

# Upload files
cd kcore-site
aws s3 sync . s3://kcore.systems \
    --acl public-read \
    --cache-control max-age=31536000,public

# Create CloudFront distribution (optional, for CDN)
aws cloudfront create-distribution \
    --origin-domain-name kcore.systems.s3.amazonaws.com
```

## Domain Configuration

### DNS Settings

Point your domain to the hosting service:

**For Vercel/Netlify/Cloudflare:**
- Add CNAME record: `www` → `[your-project].vercel.app`
- Add A/AAAA records for apex domain (provided by host)

**For Self-hosted:**
- Add A record: `@` → `[your-server-ip]`
- Add A record: `www` → `[your-server-ip]`

### SSL/TLS Certificate

**Automated (Recommended):**
- Hosting platforms provide automatic SSL
- For self-hosted, use Let's Encrypt:

```bash
# Install Certbot
sudo apt install certbot python3-certbot-nginx

# Get certificate
sudo certbot --nginx -d kcore.systems -d www.kcore.systems

# Auto-renewal is configured automatically
```

## Performance Optimization

### Enable Compression

**Nginx:**
```nginx
gzip on;
gzip_vary on;
gzip_types text/plain text/css text/xml text/javascript application/javascript application/xml+rss application/json;
```

**Apache:**
```apache
<IfModule mod_deflate.c>
    AddOutputFilterByType DEFLATE text/html text/plain text/xml text/css text/javascript application/javascript application/json
</IfModule>
```

### HTTP/2

Enable HTTP/2 for better performance:
- Nginx: Use `listen 443 ssl http2;`
- Apache: Enable `mod_http2` module
- Most hosting platforms enable it automatically

### CDN

Use a CDN for global performance:
- Cloudflare (free tier available)
- AWS CloudFront
- Fastly
- Most hosting platforms include CDN

## Analytics

Add analytics by inserting before `</body>` in HTML files:

### Google Analytics
```html
<script async src="https://www.googletagmanager.com/gtag/js?id=GA_MEASUREMENT_ID"></script>
<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){dataLayer.push(arguments);}
  gtag('js', new Date());
  gtag('config', 'GA_MEASUREMENT_ID');
</script>
```

### Plausible (Privacy-friendly)
```html
<script defer data-domain="kcore.systems" src="https://plausible.io/js/script.js"></script>
```

## Monitoring

Monitor your site's uptime and performance:
- [UptimeRobot](https://uptimerobot.com) - Free uptime monitoring
- [PageSpeed Insights](https://pagespeed.web.dev) - Performance testing
- [Lighthouse](https://developers.google.com/web/tools/lighthouse) - Audit tool

## Maintenance

### Regular Updates
- Keep content fresh and accurate
- Update dependencies if you add any
- Monitor and fix broken links
- Review and respond to user feedback

### Backups
- Git repository serves as version control
- Hosting platforms usually provide automatic backups
- For self-hosted, set up regular backups:

```bash
# Backup script
#!/bin/bash
tar -czf kcore-site-backup-$(date +%Y%m%d).tar.gz /var/www/kcore-site
```

## Troubleshooting

### Site not loading
1. Check DNS propagation: `dig kcore.systems`
2. Verify hosting service status
3. Check server logs: `tail -f /var/log/nginx/error.log`

### Styles not applying
1. Clear browser cache (Ctrl+Shift+R)
2. Check file paths in HTML
3. Verify CSS file is being served: Check Network tab in DevTools

### SSL issues
1. Verify certificate is valid: `openssl s_client -connect kcore.systems:443`
2. Check certificate expiration
3. Ensure auto-renewal is working

## Security Checklist

- [ ] HTTPS enabled with valid certificate
- [ ] Security headers configured
- [ ] Hidden files (.git, .env) not accessible
- [ ] Directory listing disabled
- [ ] Regular security updates applied
- [ ] DDoS protection enabled (via Cloudflare or similar)
- [ ] Rate limiting configured

## Contact

For issues or questions about deployment, refer to the main kcore repository or documentation.


