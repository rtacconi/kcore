package storage

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
)

// statfs wrapper
func statfs(path string, stat *syscall.Statfs_t) error {
	return syscall.Statfs(path, stat)
}

// LocalDirDriver implements volume storage using qcow2 files in a directory
type LocalDirDriver struct {
	basePath string
}

func NewLocalDirDriver(basePath string) (*LocalDirDriver, error) {
	if err := os.MkdirAll(basePath, 0755); err != nil {
		return nil, fmt.Errorf("failed to create base directory: %w", err)
	}

	return &LocalDirDriver{basePath: basePath}, nil
}

func (d *LocalDirDriver) Name() string {
	return "local-dir"
}

func (d *LocalDirDriver) Create(ctx context.Context, spec VolumeSpecOnNode) (string, error) {
	// Create qcow2 file
	filename := fmt.Sprintf("%s.qcow2", spec.VolumeID)
	filePath := filepath.Join(d.basePath, filename)

	// Use qemu-img to create the qcow2 file
	cmd := exec.CommandContext(ctx, "qemu-img", "create", "-f", "qcow2", filePath, fmt.Sprintf("%d", spec.SizeBytes))
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("failed to create qcow2 file: %w", err)
	}

	return filePath, nil
}

func (d *LocalDirDriver) Delete(ctx context.Context, backendHandle string) error {
	if err := os.Remove(backendHandle); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to delete volume file: %w", err)
	}
	return nil
}

func (d *LocalDirDriver) Attach(ctx context.Context, backendHandle, vmID, targetDev, bus string) error {
	// For file-based volumes, attachment is handled by libvirt XML
	// This is a no-op for local-dir driver
	return nil
}

func (d *LocalDirDriver) Detach(ctx context.Context, backendHandle, vmID string) error {
	// For file-based volumes, detachment is handled by libvirt XML
	// This is a no-op for local-dir driver
	return nil
}

func (d *LocalDirDriver) Capacity(ctx context.Context) (total, free uint64, err error) {
	var stat syscall.Statfs_t
	if err := statfs(d.basePath, &stat); err != nil {
		return 0, 0, fmt.Errorf("failed to stat filesystem: %w", err)
	}

	total = uint64(stat.Blocks) * uint64(stat.Bsize)
	free = uint64(stat.Bavail) * uint64(stat.Bsize)

	return total, free, nil
}
