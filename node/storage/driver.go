package storage

import (
	"context"
	"fmt"
)

// VolumeDriver is the interface for storage backends
type VolumeDriver interface {
	Name() string
	Create(ctx context.Context, spec VolumeSpecOnNode) (backendHandle string, err error)
	Delete(ctx context.Context, backendHandle string) error
	Attach(ctx context.Context, backendHandle, vmID, targetDev, bus string) error
	Detach(ctx context.Context, backendHandle, vmID string) error
	Capacity(ctx context.Context) (total, free uint64, err error)
}

// VolumeSpecOnNode contains the information needed to create a volume on a node
type VolumeSpecOnNode struct {
	VolumeID     string
	StorageClass string
	SizeBytes    int64
	Parameters   map[string]string
}

// DriverRegistry manages storage drivers
type DriverRegistry struct {
	drivers map[string]VolumeDriver
}

func NewDriverRegistry() *DriverRegistry {
	return &DriverRegistry{
		drivers: make(map[string]VolumeDriver),
	}
}

func (r *DriverRegistry) Register(driver VolumeDriver) {
	r.drivers[driver.Name()] = driver
}

func (r *DriverRegistry) Get(name string) (VolumeDriver, error) {
	driver, ok := r.drivers[name]
	if !ok {
		return nil, fmt.Errorf("storage driver %s not found", name)
	}
	return driver, nil
}

func (r *DriverRegistry) List() []string {
	names := make([]string, 0, len(r.drivers))
	for name := range r.drivers {
		names = append(names, name)
	}
	return names
}
