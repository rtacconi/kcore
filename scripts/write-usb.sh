#!/usr/bin/env bash
set -euo pipefail

echo "💾 Writing ISO to USB drive..."
echo "Usage: Requires USB_DEVICE environment variable"
echo "Example: USB_DEVICE=/dev/disk4 make write-usb (macOS)"
echo "Example: USB_DEVICE=/dev/sdb make write-usb (Linux)"

if [ -z "${USB_DEVICE:-}" ]; then
  echo "❌ Error: USB_DEVICE not set"
  exit 1
fi

if ! ls result/iso/*.iso 1> /dev/null 2>&1; then
  echo "❌ ISO not found. Run: make build-iso"
  exit 1
fi

ISO_FILE=$(ls result/iso/*.iso | head -1)
echo "Writing $ISO_FILE to $USB_DEVICE..."
echo "⚠️  This will ERASE all data on the USB drive!"
read -p "Type YES to continue: " confirm

if [ "$confirm" != "YES" ]; then
  echo "Cancelled."
  exit 0
fi

if [[ "$OSTYPE" == "darwin"* ]]; then
  # macOS
  sudo dd if="$ISO_FILE" of="$USB_DEVICE" bs=4m status=progress
else
  # Linux
  sudo dd if="$ISO_FILE" of="$USB_DEVICE" bs=4M status=progress oflag=sync
fi

echo "✅ USB drive written successfully!"

