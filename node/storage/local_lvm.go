package storage

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
)

// LocalLVMDriver implements volume storage using LVM logical volumes
type LocalLVMDriver struct {
	volumeGroup string
}

func NewLocalLVMDriver(volumeGroup string) (*LocalLVMDriver, error) {
	// Verify volume group exists
	cmd := exec.Command("vgs", volumeGroup, "--noheadings", "--options", "vg_name")
	output, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("volume group %s not found: %w", volumeGroup, err)
	}

	if strings.TrimSpace(string(output)) != volumeGroup {
		return nil, fmt.Errorf("volume group %s not found", volumeGroup)
	}

	return &LocalLVMDriver{volumeGroup: volumeGroup}, nil
}

func (d *LocalLVMDriver) Name() string {
	return "local-lvm"
}

func (d *LocalLVMDriver) Create(ctx context.Context, spec VolumeSpecOnNode) (string, error) {
	// Create LV name from volume ID
	lvName := fmt.Sprintf("kcore-%s", spec.VolumeID)

	// Calculate size in MB (LVM uses MB as base unit)
	sizeMB := spec.SizeBytes / (1024 * 1024)
	if sizeMB == 0 {
		sizeMB = 1 // Minimum 1MB
	}

	// Create logical volume
	cmd := exec.CommandContext(ctx, "lvcreate", "-L", fmt.Sprintf("%dM", sizeMB), "-n", lvName, d.volumeGroup)
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("failed to create logical volume: %w", err)
	}

	// Return device path
	devicePath := fmt.Sprintf("/dev/%s/%s", d.volumeGroup, lvName)
	return devicePath, nil
}

func (d *LocalLVMDriver) Delete(ctx context.Context, backendHandle string) error {
	// Extract LV name from device path
	// /dev/vg0/kcore-123 -> kcore-123
	parts := strings.Split(backendHandle, "/")
	if len(parts) < 4 {
		return fmt.Errorf("invalid device path: %s", backendHandle)
	}
	lvName := parts[len(parts)-1]

	// Remove logical volume
	cmd := exec.CommandContext(ctx, "lvremove", "-f", fmt.Sprintf("%s/%s", d.volumeGroup, lvName))
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("failed to remove logical volume: %w", err)
	}

	return nil
}

func (d *LocalLVMDriver) Attach(ctx context.Context, backendHandle, vmID, targetDev, bus string) error {
	// For block devices, attachment is handled by libvirt XML
	// This is a no-op for local-lvm driver
	return nil
}

func (d *LocalLVMDriver) Detach(ctx context.Context, backendHandle, vmID string) error {
	// For block devices, detachment is handled by libvirt XML
	// This is a no-op for local-lvm driver
	return nil
}

func (d *LocalLVMDriver) Capacity(ctx context.Context) (total uint64, free uint64, err error) {
	// Query VG capacity using vgs
	cmd := exec.CommandContext(ctx, "vgs", d.volumeGroup, "--noheadings", "--units", "b", "--options", "vg_size,vg_free")
	output, err := cmd.Output()
	if err != nil {
		return 0, 0, fmt.Errorf("failed to query volume group: %w", err)
	}

	// Parse output: "1073741824B  536870912B"
	fields := strings.Fields(string(output))
	if len(fields) < 2 {
		return 0, 0, fmt.Errorf("unexpected vgs output format")
	}

	var totalBytes, freeBytes uint64
	if _, err := fmt.Sscanf(fields[0], "%dB", &totalBytes); err != nil {
		return 0, 0, fmt.Errorf("failed to parse total size: %w", err)
	}
	if _, err := fmt.Sscanf(fields[1], "%dB", &freeBytes); err != nil {
		return 0, 0, fmt.Errorf("failed to parse free size: %w", err)
	}

	return totalBytes, freeBytes, nil
}
