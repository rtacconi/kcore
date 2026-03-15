package main

import (
	"fmt"
	"strings"

	"gopkg.in/yaml.v3"
)

// Manifest is the top-level envelope for any kcore YAML resource.
type Manifest struct {
	APIVersion string   `yaml:"apiVersion"`
	Kind       string   `yaml:"kind"`
	Metadata   Metadata `yaml:"metadata"`
}

type Metadata struct {
	Name      string            `yaml:"name"`
	Namespace string            `yaml:"namespace,omitempty"`
	Labels    map[string]string `yaml:"labels,omitempty"`
}

// VMManifest is the full typed manifest for kind: VM / VirtualMachine.
type VMManifest struct {
	APIVersion string   `yaml:"apiVersion"`
	Kind       string   `yaml:"kind"`
	Metadata   Metadata `yaml:"metadata"`
	Spec       VMSpec   `yaml:"spec"`
}

type VMSpec struct {
	CPU              int       `yaml:"cpu"`
	Memory           string    `yaml:"memory"`
	Image            string    `yaml:"image,omitempty"`
	EnableKcoreLogin *bool     `yaml:"enableKcoreLogin,omitempty"`
	CloudInit        string    `yaml:"cloudInit,omitempty"`
	Nics             []NicSpec `yaml:"nics,omitempty"`
}

type NicSpec struct {
	Network string `yaml:"network"`
	Model   string `yaml:"model,omitempty"`
}

// parseManifestKind reads just enough of the YAML to determine the Kind field.
func parseManifestKind(data []byte) (string, error) {
	var m Manifest
	if err := yaml.Unmarshal(data, &m); err != nil {
		return "", fmt.Errorf("invalid YAML: %w", err)
	}
	return m.Kind, nil
}

// parseVMManifest fully parses a VM manifest and validates required fields.
func parseVMManifest(data []byte) (*VMManifest, error) {
	var m VMManifest
	if err := yaml.Unmarshal(data, &m); err != nil {
		return nil, fmt.Errorf("invalid VM YAML: %w", err)
	}
	if m.Metadata.Name == "" {
		return nil, fmt.Errorf("metadata.name is required")
	}
	if m.Spec.CPU <= 0 {
		return nil, fmt.Errorf("spec.cpu must be > 0")
	}
	if m.Spec.Memory == "" {
		return nil, fmt.Errorf("spec.memory is required (e.g. 2G, 4096M)")
	}
	return &m, nil
}

// isVMKind returns true if the kind string represents a VM resource.
func isVMKind(kind string) bool {
	k := strings.ToLower(kind)
	return k == "vm" || k == "virtualmachine"
}
