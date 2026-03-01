package config

import (
	"fmt"
	"io"
	"os"

	"gopkg.in/yaml.v3"
)

// VM spec from YAML
type VMSpec struct {
	APIVersion string     `yaml:"apiVersion"`
	Kind       string     `yaml:"kind"`
	Metadata   VMMetadata `yaml:"metadata"`
	Spec       VMSpecSpec `yaml:"spec"`
}

type VMMetadata struct {
	Name      string            `yaml:"name"`
	Namespace string            `yaml:"namespace"`
	Labels    map[string]string `yaml:"labels,omitempty"`
}

type VMSpecSpec struct {
	NodeSelector map[string]string `yaml:"nodeSelector,omitempty"`
	CPU          int               `yaml:"cpu"`
	MemoryBytes  string            `yaml:"memoryBytes"` // e.g., "4GiB", "8192MiB"
	Disks        []DiskSpec        `yaml:"disks"`
	NICs         []NICSpec         `yaml:"nics"`
}

type DiskSpec struct {
	Name             string `yaml:"name"`
	SizeBytes        string `yaml:"sizeBytes"` // e.g., "40GiB"
	StorageClassName string `yaml:"storageClassName"`
	Bus              string `yaml:"bus,omitempty"` // virtio, scsi, etc.
}

type NICSpec struct {
	Network string `yaml:"network"`
	Model   string `yaml:"model,omitempty"` // virtio, e1000, etc.
}

// Volume spec from YAML
type VolumeSpec struct {
	APIVersion string         `yaml:"apiVersion"`
	Kind       string         `yaml:"kind"`
	Metadata   VolumeMetadata `yaml:"metadata"`
	Spec       VolumeSpecSpec `yaml:"spec"`
}

type VolumeMetadata struct {
	Name      string            `yaml:"name"`
	Namespace string            `yaml:"namespace"`
	Labels    map[string]string `yaml:"labels,omitempty"`
}

type VolumeSpecSpec struct {
	StorageClassName string            `yaml:"storageClassName"`
	SizeBytes        string            `yaml:"sizeBytes"`
	Parameters       map[string]string `yaml:"parameters,omitempty"`
}

// StorageClass spec from YAML
type StorageClassSpec struct {
	APIVersion string               `yaml:"apiVersion"`
	Kind       string               `yaml:"kind"`
	Metadata   StorageClassMetadata `yaml:"metadata"`
	Spec       StorageClassSpecSpec `yaml:"spec"`
}

type StorageClassMetadata struct {
	Name string `yaml:"name"`
}

type StorageClassSpecSpec struct {
	Driver     string            `yaml:"driver"` // local-dir, local-lvm, linstor-*, san-*
	Shared     bool              `yaml:"shared"`
	Parameters map[string]string `yaml:"parameters,omitempty"`
}

// Network spec from YAML
type NetworkSpec struct {
	APIVersion string          `yaml:"apiVersion"`
	Kind       string          `yaml:"kind"`
	Metadata   NetworkMetadata `yaml:"metadata"`
	Spec       NetworkSpecSpec `yaml:"spec"`
}

type NetworkMetadata struct {
	Name string `yaml:"name"`
}

type NetworkSpecSpec struct {
	BridgeName  string `yaml:"bridgeName"`
	Description string `yaml:"description,omitempty"`
}

// Parse utilities
func ParseVMFromFile(path string) (*VMSpec, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("failed to open file: %w", err)
	}
	defer f.Close()

	return ParseVM(f)
}

func ParseVM(r io.Reader) (*VMSpec, error) {
	var spec VMSpec
	if err := yaml.NewDecoder(r).Decode(&spec); err != nil {
		return nil, fmt.Errorf("failed to parse YAML: %w", err)
	}

	if spec.Kind != "VM" {
		return nil, fmt.Errorf("expected kind=VM, got %s", spec.Kind)
	}

	if spec.Metadata.Namespace == "" {
		spec.Metadata.Namespace = "default"
	}

	return &spec, nil
}

func ParseVolumeFromFile(path string) (*VolumeSpec, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("failed to open file: %w", err)
	}
	defer f.Close()

	var spec VolumeSpec
	if err := yaml.NewDecoder(f).Decode(&spec); err != nil {
		return nil, fmt.Errorf("failed to parse YAML: %w", err)
	}

	if spec.Kind != "Volume" {
		return nil, fmt.Errorf("expected kind=Volume, got %s", spec.Kind)
	}

	if spec.Metadata.Namespace == "" {
		spec.Metadata.Namespace = "default"
	}

	return &spec, nil
}

func ParseStorageClassFromFile(path string) (*StorageClassSpec, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("failed to open file: %w", err)
	}
	defer f.Close()

	var spec StorageClassSpec
	if err := yaml.NewDecoder(f).Decode(&spec); err != nil {
		return nil, fmt.Errorf("failed to parse YAML: %w", err)
	}

	if spec.Kind != "StorageClass" {
		return nil, fmt.Errorf("expected kind=StorageClass, got %s", spec.Kind)
	}

	return &spec, nil
}

// Size parsing utilities
func ParseSizeBytes(sizeStr string) (int64, error) {
	var size int64
	var unit string

	_, err := fmt.Sscanf(sizeStr, "%d%s", &size, &unit)
	if err != nil {
		return 0, fmt.Errorf("invalid size format: %s", sizeStr)
	}

	unit = normalizeUnit(unit)
	switch unit {
	case "b", "B":
		return size, nil
	case "kb", "KB":
		return size * 1024, nil
	case "mb", "MB":
		return size * 1024 * 1024, nil
	case "gb", "GB":
		return size * 1024 * 1024 * 1024, nil
	case "tb", "TB":
		return size * 1024 * 1024 * 1024 * 1024, nil
	case "kib", "KiB":
		return size * 1024, nil
	case "mib", "MiB":
		return size * 1024 * 1024, nil
	case "gib", "GiB":
		return size * 1024 * 1024 * 1024, nil
	case "tib", "TiB":
		return size * 1024 * 1024 * 1024 * 1024, nil
	default:
		return 0, fmt.Errorf("unknown unit: %s", unit)
	}
}

func normalizeUnit(unit string) string {
	unit = toLower(unit)
	// Handle common variations
	if unit == "k" {
		return "kb"
	}
	if unit == "m" {
		return "mb"
	}
	if unit == "g" {
		return "gb"
	}
	if unit == "t" {
		return "tb"
	}
	return unit
}

func toLower(s string) string {
	result := make([]byte, len(s))
	for i := 0; i < len(s); i++ {
		c := s[i]
		if c >= 'A' && c <= 'Z' {
			c += 'a' - 'A'
		}
		result[i] = c
	}
	return string(result)
}
