#!/bin/bash
# Script to verify USB stick is bootable

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

if [ $# -lt 1 ]; then
    echo "Usage: $0 <USB_DEVICE>"
    echo "Example: $0 /dev/disk4"
    exit 1
fi

USB_DEVICE="$1"

echo -e "${YELLOW}=== Verifying USB Bootability: $USB_DEVICE ===${NC}"
echo ""

# Check if device exists
if ! diskutil list "$USB_DEVICE" &>/dev/null; then
    echo -e "${RED}✗ Error: Device $USB_DEVICE not found${NC}"
    exit 1
fi

echo "1. Checking partition table:"
diskutil list "$USB_DEVICE"
echo ""

# Check for boot signatures
echo "2. Checking boot signatures:"
USB_RAW="/dev/r$(basename $USB_DEVICE)"

# Check MBR boot signature (bytes 510-511 should be 0x55 0xAA)
MBR_SIG=$(sudo hexdump -n 512 -s 510 -v -e '/1 "%02x"' "$USB_RAW" 2>/dev/null || echo "0000")
if [ "$MBR_SIG" = "55aa" ]; then
    echo -e "${GREEN}✓ MBR boot signature found (0x55 0xAA)${NC}"
else
    echo -e "${YELLOW}⚠ MBR boot signature: $MBR_SIG (expected: 55aa)${NC}"
fi

# Check for GPT signature
GPT_SIG=$(sudo hexdump -n 8 -s 512 -v -e '/1 "%02x"' "$USB_RAW" 2>/dev/null || echo "0000000000000000")
if [ "$GPT_SIG" = "4546492050415254" ]; then
    echo -e "${GREEN}✓ GPT signature found (EFI PART)${NC}"
else
    echo -e "${YELLOW}⚠ GPT signature: $GPT_SIG (may use MBR or hybrid)${NC}"
fi

echo ""

# Try to mount and check for EFI files
echo "3. Checking for EFI boot files:"
if diskutil mountDisk "$USB_DEVICE" &>/dev/null; then
    # Look for EFI partition
    MOUNTED_VOLUMES=$(diskutil list "$USB_DEVICE" | grep -E "disk4s[0-9]" | awk '{print $NF}' | head -3)
    
    EFI_FOUND=false
    for vol in $MOUNTED_VOLUMES; do
        MOUNT_POINT=$(diskutil info "$vol" 2>/dev/null | grep "Mount Point" | cut -d: -f2 | xargs)
        if [ -n "$MOUNT_POINT" ] && [ -d "$MOUNT_POINT/EFI" ]; then
            echo -e "${GREEN}✓ Found EFI directory at: $MOUNT_POINT/EFI${NC}"
            if [ -d "$MOUNT_POINT/EFI/BOOT" ]; then
                echo -e "${GREEN}✓ Found EFI/BOOT directory${NC}"
                if [ -f "$MOUNT_POINT/EFI/BOOT/BOOTx64.EFI" ] || [ -f "$MOUNT_POINT/EFI/BOOT/bootx64.efi" ]; then
                    echo -e "${GREEN}✓ Found BOOTx64.EFI boot file${NC}"
                    EFI_FOUND=true
                else
                    echo -e "${YELLOW}⚠ EFI/BOOT directory exists but BOOTx64.EFI not found${NC}"
                    ls -la "$MOUNT_POINT/EFI/BOOT/" 2>/dev/null | head -5 || echo "  (directory empty or inaccessible)"
                fi
            fi
        fi
    done
    
    # Also check ISO9660 filesystem (NixOS ISOs are hybrid)
    ISO9660_VOL=$(diskutil list "$USB_DEVICE" | grep -i "cd9660\|ISO" | head -1)
    if [ -n "$ISO9660_VOL" ]; then
        echo -e "${GREEN}✓ ISO9660 filesystem detected (hybrid ISO)${NC}"
        EFI_FOUND=true
    fi
    
    diskutil unmountDisk "$USB_DEVICE" &>/dev/null || true
else
    echo -e "${YELLOW}⚠ Could not mount USB for file system check${NC}"
    echo "  (This may be normal for some bootable USB formats)"
fi

echo ""

# Final assessment
echo "4. Bootability Assessment:"
if [ "$MBR_SIG" = "55aa" ] || [ "$GPT_SIG" = "4546492050415254" ] || [ -n "$ISO9660_VOL" ]; then
    echo -e "${GREEN}✓ USB appears to be bootable${NC}"
    echo ""
    echo "Boot instructions:"
    echo "  1. Insert USB before powering on"
    echo "  2. Press F12 during boot to access boot menu"
    echo "  3. Select USB device"
    echo ""
    echo "If USB doesn't appear:"
    echo "  • Disable Secure Boot in BIOS"
    echo "  • Enable Legacy/CSM mode"
    echo "  • Try USB 2.0 ports"
else
    echo -e "${RED}✗ USB may not be bootable - boot signatures not found${NC}"
    echo "  Consider rewriting with: sudo dd if=kcore.iso of=/dev/rdisk4 bs=4m conv=sync"
fi



