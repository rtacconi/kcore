# Devbox Setup for kcore-site

## Using the Development Server with Devbox

Since this project uses devbox for environment management, Python 3 has been added to the devbox configuration.

### Option 1: Use devbox script (Recommended)

From the project root:

```bash
devbox run serve-site
```

This will start the server at http://localhost:8000

### Option 2: Manual start within devbox shell

```bash
# Enter devbox shell (if not already in it)
devbox shell

# Navigate to kcore-site
cd kcore-site

# Run the serve script
./serve.sh
```

### Option 3: Direct Python command

```bash
devbox shell  # if not already in shell
cd kcore-site
python3 -m http.server 8000
```

## What was added to devbox.json

1. **Package**: `python3` was added to the packages list
2. **Script**: `serve-site` script was added for convenience

## Troubleshooting

### "python3 not found" error

This means you're not in the devbox shell. Run:

```bash
devbox shell
```

Then try again.

### Port already in use

If port 8000 is already in use, you can specify a different port:

```bash
cd kcore-site
python3 -m http.server 3000  # Use port 3000 instead
```

## Viewing the Site

Once the server is running, open your browser to:
- http://localhost:8000 (default)
- Or the port you specified

Press `Ctrl+C` to stop the server.

## Quick Commands Summary

```bash
# From project root - easiest way
devbox run serve-site

# Or with devbox shell
devbox shell
cd kcore-site && ./serve.sh

# Or direct command
devbox run python3 -m http.server 8000 -d kcore-site
```

