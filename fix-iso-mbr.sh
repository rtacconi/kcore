#!/bin/bash
# Post-process the ISO to add proper MBR boot code for Legacy BIOS
set -euo pipefail

ISO="$1"

if [ ! -f "$ISO" ]; then
    echo "Error: ISO file not found: $ISO"
    exit 1
fi

echo "=== Fixing ISO MBR for Legacy BIOS boot ==="
echo "ISO: $ISO"
echo ""

# Check if isohybrid is available
if ! command -v isohybrid &> /dev/null; then
    echo "Error: isohybrid not found"
    echo "Install syslinux package:"
    echo "  macOS: nix profile install nixpkgs#syslinux"
    echo "  Linux: sudo apt-get install syslinux-utils"
    exit 1
fi

# Make a backup
BACKUP="${ISO}.backup"
if [ ! -f "$BACKUP" ]; then
    echo "Creating backup: $BACKUP"
    cp "$ISO" "$BACKUP"
fi

# Run isohybrid to add MBR boot code
# -u : Generate UUID (helps with booting)
# -h 64 : Set head count to 64 (more compatible)
# -s 32 : Set sector count to 32 (more compatible)
echo "Running isohybrid..."
isohybrid -u -h 64 -s 32 "$ISO"

echo ""
echo "✓ ISO MBR fixed!"
echo ""
echo "Verifying MBR signature..."
# Check MBR signature (should be 55aa at offset 510)
MBR_SIG=$(dd if="$ISO" bs=1 skip=510 count=2 2>/dev/null | xxd -p)
if [ "$MBR_SIG" == "55aa" ]; then
    echo "✓ MBR signature: $MBR_SIG (valid)"
else
    echo "⚠ MBR signature: $MBR_SIG (expected: 55aa)"
    exit 1
fi

echo ""
echo "ISO is now hybrid-bootable for both Legacy BIOS and UEFI!"



