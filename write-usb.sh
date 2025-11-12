#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 2 ]; then
  echo "Usage: $0 <path/to.iso> </dev/device>"
  echo "Examples:"
  echo "  macOS: $0 kcode.iso /dev/disk3   (will use /dev/rdisk3)"
  echo "  Linux: $0 kcode.iso /dev/sdX"
  exit 1
fi

ISO="$1"
DEV="$2"
OS="$(uname -s)"

if [ ! -f "$ISO" ]; then
  echo "Error: ISO file not found: $ISO"
  exit 1
fi

echo "ISO: $ISO"
echo "Device: $DEV"
echo "OS: $OS"
echo ""
echo "⚠️  WARNING: This will completely erase $DEV!"
echo ""
read -rp ">> THIS WILL OVERWRITE ${DEV}. Continue? (yes/NO) " ans
[ "${ans:-}" = "yes" ] || { echo "Aborted."; exit 1; }

case "$OS" in
  Darwin)
    # macOS: prefer raw device for speed (BSD dd - no status=progress support)
    RAWDEV="${DEV/disk/rdisk}"
    echo "Unmounting disk (keeping it connected)..."
    
    # Unmount all volumes on the disk (but don't eject)
    sudo diskutil unmountDisk force "$DEV" || true
    sleep 1
    
    echo "Writing ISO to $RAWDEV (this may take 3-5 minutes)..."
    echo "Note: No progress display available with BSD dd. Press Ctrl+T to see status..."
    echo ""
    
    # Use numeric block size for compatibility (1MB = 1048576 bytes)
    sudo dd if="$ISO" of="$RAWDEV" bs=1048576
    
    sync
    echo ""
    echo "✅ Done. macOS may pop up a 'disk not readable' dialog; that's expected for a bootable ISO."
    echo "   Click 'Ignore' or 'Eject'."
    ;;
  Linux)
    # Linux (GNU dd supports status=progress)
    echo "Unmounting any mounted partitions..."
    sudo umount "${DEV}"* 2>/dev/null || true
    echo "Writing ISO to $DEV (this may take several minutes)..."
    sudo dd if="$ISO" of="$DEV" bs=4M conv=fsync oflag=direct status=progress
    sync
    echo ""
    echo "✅ Done."
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

echo ""
echo "🔌 Safely eject the drive before removing it."
