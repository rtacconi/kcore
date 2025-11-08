#!/bin/bash
# Script to write kcode ISO image to USB stick

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Find ISO file
# If ISO path is provided as argument, use it
if [ $# -ge 1 ]; then
    ISO_FILE="$1"
    if [ ! -f "$ISO_FILE" ]; then
        echo -e "${RED}Error: ISO file not found: $ISO_FILE${NC}"
        exit 1
    fi
else
    # Otherwise, look in result-iso/iso directory
    ISO_DIR="result-iso/iso"
    if [ ! -d "$ISO_DIR" ]; then
        echo -e "${RED}Error: ISO directory not found: $ISO_DIR${NC}"
        echo "Please provide ISO file path as argument:"
        echo "  ./write-usb.sh /path/to/kcode.iso"
        echo ""
        echo "Or build the ISO first with:"
        echo "  nix build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' -o result-iso"
        exit 1
    fi
    
    ISO_FILE=$(find "$ISO_DIR" -name "kcode-*.iso" -o -name "nixos-kcode-*.iso" | head -1)
    if [ -z "$ISO_FILE" ]; then
        echo -e "${RED}Error: No ISO file found in $ISO_DIR${NC}"
        exit 1
    fi
fi

echo -e "${GREEN}Found ISO: $ISO_FILE${NC}"
ISO_SIZE=$(du -h "$ISO_FILE" | cut -f1)
echo -e "ISO size: ${GREEN}$ISO_SIZE${NC}"
echo ""

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS="macos"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    OS="linux"
else
    echo -e "${RED}Error: Unsupported OS: $OSTYPE${NC}"
    exit 1
fi

# Function to list disks on macOS
list_disks_macos() {
    echo -e "${YELLOW}Available disks:${NC}"
    diskutil list | grep -E "^/dev/disk" | while read -r line; do
        DEVICE=$(echo "$line" | awk '{print $1}')
        SIZE=$(diskutil info "$DEVICE" 2>/dev/null | grep "Disk Size" | awk '{print $3, $4}' || echo "unknown")
        MODEL=$(diskutil info "$DEVICE" 2>/dev/null | grep "Device / Media Name" | cut -d: -f2 | xargs || echo "Unknown")
        NAME=$(diskutil info "$DEVICE" 2>/dev/null | grep "Volume Name" | cut -d: -f2 | xargs || echo "")
        # Show model name, or volume name if available
        if [ -n "$NAME" ] && [ "$NAME" != "Not applicable (no file system)" ]; then
            DISPLAY_NAME="$NAME"
        else
            DISPLAY_NAME="$MODEL"
        fi
        echo "  $DEVICE - $SIZE - $DISPLAY_NAME"
    done
}

# Function to list disks on Linux
list_disks_linux() {
    echo -e "${YELLOW}Available disks:${NC}"
    lsblk -o NAME,SIZE,TYPE,MOUNTPOINT,MODEL | grep -E "disk|NAME" | while read -r line; do
        echo "  $line"
    done
}

# Function to get disk info on macOS
get_disk_info_macos() {
    local device=$1
    echo "  Device Node: $(diskutil info "$device" 2>/dev/null | grep "Device Node" | cut -d: -f2 | xargs)"
    echo "  Disk Size: $(diskutil info "$device" 2>/dev/null | grep "Disk Size" | cut -d: -f2 | xargs)"
    echo "  Device Model: $(diskutil info "$device" 2>/dev/null | grep "Device / Media Name" | cut -d: -f2 | xargs)"
    VOLUME_NAME=$(diskutil info "$device" 2>/dev/null | grep "Volume Name" | cut -d: -f2 | xargs)
    if [ -n "$VOLUME_NAME" ] && [ "$VOLUME_NAME" != "Not applicable (no file system)" ]; then
        echo "  Volume Name: $VOLUME_NAME"
    fi
    echo "  Protocol: $(diskutil info "$device" 2>/dev/null | grep "Protocol" | cut -d: -f2 | xargs)"
}

# Function to get disk info on Linux
get_disk_info_linux() {
    local device=$1
    lsblk -o NAME,SIZE,TYPE,MOUNTPOINT,MODEL "$device" 2>/dev/null || echo "Device not found"
}

# List available disks
if [ "$OS" == "macos" ]; then
    list_disks_macos
    echo ""
    echo -e "${YELLOW}Enter the USB device (e.g., /dev/disk2):${NC}"
    read -r USB_DEVICE
    
    # Validate device exists
    if ! diskutil list "$USB_DEVICE" &>/dev/null; then
        echo -e "${RED}Error: Device $USB_DEVICE not found${NC}"
        exit 1
    fi
    
    # Show device info
    echo ""
    echo -e "${YELLOW}Device information:${NC}"
    get_disk_info_macos "$USB_DEVICE"
    
    # Use rdisk for faster writes on macOS
    USB_DEVICE_RAW="/dev/r$(basename $USB_DEVICE)"
    
elif [ "$OS" == "linux" ]; then
    list_disks_linux
    echo ""
    echo -e "${YELLOW}Enter the USB device (e.g., /dev/sdb):${NC}"
    read -r USB_DEVICE
    
    # Validate device exists
    if [ ! -b "$USB_DEVICE" ]; then
        echo -e "${RED}Error: Device $USB_DEVICE not found or not a block device${NC}"
        exit 1
    fi
    
    # Show device info
    echo ""
    echo -e "${YELLOW}Device information:${NC}"
    get_disk_info_linux "$USB_DEVICE"
    
    USB_DEVICE_RAW="$USB_DEVICE"
fi

# Safety confirmation
echo ""
echo -e "${RED}WARNING: This will ERASE all data on $USB_DEVICE${NC}"
echo -e "ISO file: ${GREEN}$ISO_FILE${NC}"
echo -e "Target device: ${RED}$USB_DEVICE${NC}"
echo ""
echo -e "${YELLOW}Type 'yes' to continue:${NC}"
read -r CONFIRM

if [ "$CONFIRM" != "yes" ]; then
    echo "Aborted."
    exit 0
fi

# Unmount on macOS
if [ "$OS" == "macos" ]; then
    echo ""
    echo -e "${YELLOW}Unmounting $USB_DEVICE...${NC}"
    diskutil unmountDisk "$USB_DEVICE" || true
    sleep 2
fi

# Unmount on Linux
if [ "$OS" == "linux" ]; then
    echo ""
    echo -e "${YELLOW}Unmounting partitions on $USB_DEVICE...${NC}"
    for partition in $(lsblk -ln -o NAME "$USB_DEVICE" | grep -v "^$(basename $USB_DEVICE)$"); do
        umount "/dev/$partition" 2>/dev/null || true
    done
    sleep 2
fi

# Write ISO
echo ""
echo -e "${GREEN}Writing ISO to USB stick...${NC}"
echo -e "This may take several minutes. Please wait...${NC}"
echo ""

if [ "$OS" == "macos" ]; then
    # macOS dd doesn't support status=progress, use pv if available or basic dd
    if command -v pv &> /dev/null; then
        sudo pv "$ISO_FILE" | sudo dd of="$USB_DEVICE_RAW" bs=1M
    else
        echo "Writing (this may take a few minutes, no progress shown)..."
        sudo dd if="$ISO_FILE" of="$USB_DEVICE_RAW" bs=1M
    fi
    sync
    diskutil eject "$USB_DEVICE"
elif [ "$OS" == "linux" ]; then
    sudo dd if="$ISO_FILE" of="$USB_DEVICE_RAW" bs=4M status=progress oflag=sync
    sync
fi

echo ""
echo -e "${GREEN}✓ ISO successfully written to USB stick!${NC}"
echo -e "${GREEN}You can now boot from the USB stick on your ThinkCentre.${NC}"

