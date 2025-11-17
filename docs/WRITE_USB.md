# Writing ISO to USB Stick

## Quick Method: Use the Script

A helper script is provided to make this easier:

```bash
./write-usb.sh
```

The script will:
1. Find the ISO file automatically
2. List available USB devices
3. Show device information
4. Ask for confirmation before writing
5. Safely write the ISO using `dd`

## Manual Method

### On macOS:

1. **Find your USB device:**
   ```bash
   diskutil list
   ```
   Look for your USB stick (usually `/dev/disk2` or similar)

2. **Unmount the USB stick:**
   ```bash
   diskutil unmountDisk /dev/diskX
   ```
   (Replace `diskX` with your device)

3. **Write the ISO:**
   ```bash
   sudo dd if=result-iso/iso/kcore-*.iso of=/dev/rdiskX bs=1m
   ```
   (Use `rdiskX` instead of `diskX` for faster writes)

4. **Eject when done:**
   ```bash
   diskutil eject /dev/diskX
   ```

### On Linux:

1. **Find your USB device:**
   ```bash
   lsblk
   ```
   Look for your USB stick (usually `/dev/sdb` or similar)

2. **Unmount any mounted partitions:**
   ```bash
   sudo umount /dev/sdb1  # if mounted
   ```

3. **Write the ISO:**
   ```bash
   sudo dd if=result-iso/iso/kcore-*.iso of=/dev/sdX bs=4M status=progress oflag=sync
   ```
   (Replace `sdX` with your device)

## Safety Notes

⚠️ **WARNING**: The `dd` command will **ERASE ALL DATA** on the target device. Make sure you've selected the correct USB stick!

- Double-check the device name before running `dd`
- The script includes safety checks and confirmation prompts
- On macOS, the script uses `rdisk` (raw disk) for faster writes
- Always verify the device size matches your USB stick

## Troubleshooting

- **"Permission denied"**: Use `sudo` for `dd` command
- **"Device busy"**: Make sure all partitions are unmounted
- **Wrong device**: Use `diskutil list` (macOS) or `lsblk` (Linux) to verify device names
- **Slow write**: This is normal for large ISO files. Be patient!

