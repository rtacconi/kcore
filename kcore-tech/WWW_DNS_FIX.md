# Fix for www.kcorehypervisor.com DNS Error

## Problem
GitHub Pages is showing an error:
```
www.kcorehypervisor.com is improperly configured
Domain's DNS record could not be retrieved. (InvalidDNSError)
```

## Solution

GitHub Pages automatically tries to verify both the root domain and the www subdomain. You need to add a DNS record for `www.kcorehypervisor.com`.

### Step 1: Add www CNAME Record

Go to your domain registrar's DNS management and add:

```
Type:    CNAME
Name:    www
Value:   rtacconi.github.io
TTL:     3600 (or Auto/Default)
```

### Step 2: Verify DNS

After adding the record, verify it's working:

```bash
dig www.kcorehypervisor.com CNAME +short
# Should return: rtacconi.github.io
```

Or use online tools:
- https://dnschecker.org/#CNAME/www.kcorehypervisor.com
- https://www.whatsmydns.net/#CNAME/www.kcorehypervisor.com

### Step 3: Wait for Propagation

- DNS propagation: 5-60 minutes (can take up to 48 hours)
- GitHub will automatically detect the DNS record once it propagates
- The error should disappear once GitHub can verify the www subdomain

### Step 4: Test Both Domains

After DNS propagates, both should work:
- https://kcorehypervisor.com ✅
- https://www.kcorehypervisor.com ✅

## Why This Happens

GitHub Pages automatically checks both:
- Root domain: `kcorehypervisor.com` (already configured ✅)
- WWW subdomain: `www.kcorehypervisor.com` (needs DNS record ❌)

Even if you only want to use the root domain, GitHub requires the www subdomain to have a valid DNS record pointing to GitHub Pages.

## Alternative: Disable www Verification

If you don't want to use www, you can:
1. Add the DNS record anyway (recommended - it's free and takes 2 minutes)
2. Or contact GitHub support to disable www verification (not recommended)

## Quick Fix Summary

**Just add this one DNS record:**
- Type: CNAME
- Name: www  
- Value: rtacconi.github.io
- Save

That's it! The error will resolve once DNS propagates.
