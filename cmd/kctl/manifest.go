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

func isNodeInstallKind(kind string) bool {
	return strings.EqualFold(kind, "nodeinstall")
}

func isNodeNetworkKind(kind string) bool {
	return strings.EqualFold(kind, "nodenetwork")
}

func isNodeConfigKind(kind string) bool {
	return strings.EqualFold(kind, "nodeconfig")
}

// NodeInstallManifest for kind: NodeInstall
type NodeInstallManifest struct {
	APIVersion string          `yaml:"apiVersion"`
	Kind       string          `yaml:"kind"`
	Metadata   Metadata        `yaml:"metadata"`
	Spec       NodeInstallSpec `yaml:"spec"`
}

type NodeInstallSpec struct {
	Address           string             `yaml:"address"`
	Disks             []NodeInstallDisk  `yaml:"disks"`
	Hostname          string             `yaml:"hostname"`
	RootPassword      string             `yaml:"rootPassword,omitempty"`
	SSHKeys           []string           `yaml:"sshKeys,omitempty"`
	RunController     bool               `yaml:"runController,omitempty"`
	ControllerAddress string             `yaml:"controllerAddress,omitempty"`
}

type NodeInstallDisk struct {
	Device string `yaml:"device"`
	Role   string `yaml:"role"`
}

func parseNodeInstallManifest(data []byte) (*NodeInstallManifest, error) {
	var m NodeInstallManifest
	if err := yaml.Unmarshal(data, &m); err != nil {
		return nil, fmt.Errorf("invalid NodeInstall YAML: %w", err)
	}
	if m.Metadata.Name == "" {
		return nil, fmt.Errorf("metadata.name is required")
	}
	if len(m.Spec.Disks) == 0 {
		return nil, fmt.Errorf("spec.disks is required (at least one 'os' disk)")
	}
	hasOS := false
	for _, d := range m.Spec.Disks {
		if d.Role == "os" {
			hasOS = true
		}
	}
	if !hasOS {
		return nil, fmt.Errorf("at least one disk with role 'os' is required")
	}
	return &m, nil
}

// NodeNetworkManifest for kind: NodeNetwork
type NodeNetworkManifest struct {
	APIVersion string          `yaml:"apiVersion"`
	Kind       string          `yaml:"kind"`
	Metadata   Metadata        `yaml:"metadata"`
	Spec       NodeNetworkSpec `yaml:"spec"`
}

type NodeNetworkSpec struct {
	NodeID     string              `yaml:"nodeId"`
	Bridges    []BridgeSpec        `yaml:"bridges,omitempty"`
	Bonds      []BondSpec          `yaml:"bonds,omitempty"`
	Vlans      []VlanSpec          `yaml:"vlans,omitempty"`
	DNSServers string              `yaml:"dnsServers,omitempty"`
	ApplyNow   bool                `yaml:"applyNow,omitempty"`
}

type BridgeSpec struct {
	Name        string   `yaml:"name"`
	MemberPorts []string `yaml:"memberPorts,omitempty"`
	IPAddress   string   `yaml:"ipAddress,omitempty"`
	SubnetMask  string   `yaml:"subnetMask,omitempty"`
	Gateway     string   `yaml:"gateway,omitempty"`
	DHCP        bool     `yaml:"dhcp,omitempty"`
}

type BondSpec struct {
	Name        string   `yaml:"name"`
	MemberPorts []string `yaml:"memberPorts,omitempty"`
	Mode        string   `yaml:"mode,omitempty"`
	IPAddress   string   `yaml:"ipAddress,omitempty"`
	SubnetMask  string   `yaml:"subnetMask,omitempty"`
	DHCP        bool     `yaml:"dhcp,omitempty"`
}

type VlanSpec struct {
	ParentInterface string `yaml:"parentInterface"`
	VlanID          int    `yaml:"vlanId"`
	IPAddress       string `yaml:"ipAddress,omitempty"`
	SubnetMask      string `yaml:"subnetMask,omitempty"`
	DHCP            bool   `yaml:"dhcp,omitempty"`
}

func parseNodeNetworkManifest(data []byte) (*NodeNetworkManifest, error) {
	var m NodeNetworkManifest
	if err := yaml.Unmarshal(data, &m); err != nil {
		return nil, fmt.Errorf("invalid NodeNetwork YAML: %w", err)
	}
	if m.Metadata.Name == "" {
		return nil, fmt.Errorf("metadata.name is required")
	}
	if m.Spec.NodeID == "" {
		return nil, fmt.Errorf("spec.nodeId is required")
	}
	return &m, nil
}

// NodeConfigManifest for kind: NodeConfig
type NodeConfigManifest struct {
	APIVersion string         `yaml:"apiVersion"`
	Kind       string         `yaml:"kind"`
	Metadata   Metadata       `yaml:"metadata"`
	Spec       NodeConfigSpec `yaml:"spec"`
}

type NodeConfigSpec struct {
	NodeID           string `yaml:"nodeId"`
	ConfigurationNix string `yaml:"configurationNix,omitempty"`
	ConfigFile       string `yaml:"configFile,omitempty"`
	Rebuild          bool   `yaml:"rebuild,omitempty"`
}

func parseNodeConfigManifest(data []byte) (*NodeConfigManifest, error) {
	var m NodeConfigManifest
	if err := yaml.Unmarshal(data, &m); err != nil {
		return nil, fmt.Errorf("invalid NodeConfig YAML: %w", err)
	}
	if m.Metadata.Name == "" {
		return nil, fmt.Errorf("metadata.name is required")
	}
	if m.Spec.NodeID == "" {
		return nil, fmt.Errorf("spec.nodeId is required")
	}
	return &m, nil
}
