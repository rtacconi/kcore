# DNS Configuration for kcorehypervisor.com

This guide explains how to configure DNS records to point `kcorehypervisor.com` to GitHub Pages.

## Prerequisites

1. ✅ GitHub Pages enabled (Settings → Pages)
2. ✅ `CNAME` file in `/kcore-tech` directory (already done)
3. ✅ Access to your domain registrar's DNS management panel

## Step-by-Step DNS Configuration

### Option 1: CNAME Record (Recommended - Easier)

This is the simplest method and automatically updates if GitHub changes their IP addresses.

#### Steps:

1. **Log in to your domain registrar** (where you purchased kcorehypervisor.com)
   - Examples: Namecheap, GoDaddy, Cloudflare, Google Domains, etc.

2. **Navigate to DNS Management**
   - Look for "DNS Settings", "DNS Management", "Name Servers", or "DNS Records"

3. **Add a CNAME Record:**
   ```
   Type:    CNAME
   Name:    @ (or leave blank, or use "www" for www.kcorehypervisor.com)
   Value:   rtacconi.github.io
   TTL:     3600 (or Auto/Default)
   ```

4. **For www subdomain (optional):**
   ```
   Type:    CNAME
   Name:    www
   Value:   rtacconi.github.io
   TTL:     3600
   ```

5. **Save the changes**

6. **Wait for DNS propagation** (5-60 minutes, up to 48 hours)

---

### Option 2: A Records (Alternative)

If your registrar doesn't support CNAME for the root domain (@), use A records pointing to GitHub Pages IP addresses.

#### Steps:

1. **Add FOUR A Records** (all pointing to different GitHub Pages IPs):

   ```
   Type:    A
   Name:    @ (or leave blank)
   Value:   185.199.108.153
   TTL:     3600

   Type:    A
   Name:    @ (or leave blank)
   Value:   185.199.109.153
   TTL:     3600

   Type:    A
   Name:    @ (or leave blank)
   Value:   185.199.110.153
   TTL:     3600

   Type:    A
   Name:    @ (or leave blank)
   Value:   185.199.111.153
   TTL:     3600
   ```

2. **Save all four records**

3. **Wait for DNS propagation**

---

## Configure Custom Domain in GitHub

After DNS records are added:

1. **Go to GitHub**: https://github.com/rtacconi/kcore/settings/pages

2. **Under "Custom domain"**:
   - Enter: `kcorehypervisor.com`
   - Check: ✅ **"Enforce HTTPS"** (after DNS propagates)

3. **Click "Save"**

4. **GitHub will verify the domain** (may take a few minutes)

---

## Verify DNS Configuration

### Check DNS Records

Use these commands to verify your DNS is configured correctly:

```bash
# Check CNAME record
dig kcorehypervisor.com CNAME +short
# Should return: rtacconi.github.io

# Check A records (if using A records)
dig kcorehypervisor.com A +short
# Should return: 185.199.108.153 (and/or other GitHub IPs)

# Full DNS lookup
nslookup kcorehypervisor.com
# or
dig kcorehypervisor.com
```

### Online DNS Checkers

- https://dnschecker.org/#CNAME/kcorehypervisor.com
- https://www.whatsmydns.net/#CNAME/kcorehypervisor.com
- https://mxtoolbox.com/SuperTool.aspx?action=cname%3akcorehypervisor.com

---

## Common DNS Record Formats by Registrar

### Namecheap
```
Type:     CNAME Record
Host:     @
Value:    rtacconi.github.io
TTL:      Automatic
```

### GoDaddy
```
Type:     CNAME
Name:     @
Value:    rtacconi.github.io
TTL:      1 Hour
```

### Cloudflare
```
Type:     CNAME
Name:     @
Target:   rtacconi.github.io
Proxy:    DNS only (gray cloud) - Important!
TTL:      Auto
```

**Note for Cloudflare**: Make sure the proxy is set to "DNS only" (gray cloud), not "Proxied" (orange cloud), as GitHub Pages doesn't work with Cloudflare's proxy.

### Google Domains
```
Type:     CNAME
Name:     @
Value:    rtacconi.github.io
TTL:      3600
```

### AWS Route 53
```
Type:     CNAME
Name:     (leave blank or @)
Value:    rtacconi.github.io
TTL:      300
```

---

## Troubleshooting

### Issue: "Domain not verified" in GitHub

**Solution:**
- Wait 5-60 minutes for DNS propagation
- Ensure CNAME/A records are correct
- Check that `CNAME` file exists in `/kcore-tech` directory
- Verify DNS with `dig` or online tools

### Issue: Site shows GitHub 404 page

**Solution:**
- Verify GitHub Pages is enabled
- Check that source is set to branch `main`, folder `/kcore-tech`
- Ensure `CNAME` file contains exactly: `kcorehypervisor.com`
- Wait for GitHub Pages deployment (check Actions tab)

### Issue: HTTPS not available

**Solution:**
- Wait for DNS to fully propagate
- In GitHub Pages settings, check "Enforce HTTPS"
- It may take up to 24 hours for SSL certificate to be issued

### Issue: DNS propagation taking too long

**Solution:**
- Lower TTL values (if possible) before making changes
- Clear DNS cache: `sudo dscacheutil -flushcache` (macOS) or `ipconfig /flushdns` (Windows)
- Use different DNS servers (8.8.8.8, 1.1.1.1) to test

---

## Expected Timeline

- **DNS Propagation**: 5-60 minutes (can take up to 48 hours)
- **GitHub Verification**: 1-10 minutes after DNS propagates
- **SSL Certificate**: Up to 24 hours after domain is verified
- **Total**: Usually 1-2 hours, worst case 48-72 hours

---

## Final Checklist

- [ ] DNS CNAME or A records configured at registrar
- [ ] DNS records verified with `dig` or online tools
- [ ] GitHub Pages enabled (branch: `main`, folder: `/kcore-tech`)
- [ ] Custom domain added in GitHub Pages settings
- [ ] `CNAME` file exists in repository (`/kcore-tech/CNAME`)
- [ ] "Enforce HTTPS" enabled in GitHub (after verification)
- [ ] Site accessible at https://kcorehypervisor.com

---

## Need Help?

If you're still having issues:

1. **Check GitHub Actions** for deployment status
2. **Verify DNS** using the tools above
3. **Check GitHub Pages settings** for error messages
4. **Review GitHub documentation**: https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site
