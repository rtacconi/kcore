package config

import (
	"fmt"
	"os"
	"strings"

	"gopkg.in/yaml.v3"
)

// ControllerConfig holds configuration for the control plane
type ControllerConfig struct {
	DatabasePath string            `yaml:"databasePath"`
	ListenAddr   string            `yaml:"listenAddr"`
	TLS          TLSConfig         `yaml:"tls"`
	NodeNetworks map[string]string `yaml:"nodeNetworks"` // network name -> bridge name mapping
}

type TLSConfig struct {
	CAFile   string `yaml:"caFile"`
	CertFile string `yaml:"certFile"`
	KeyFile  string `yaml:"keyFile"`
}

// NodeAgentConfig holds configuration for the node agent
type NodeAgentConfig struct {
	NodeID         string            `yaml:"nodeId"`
	ControllerAddr string            `yaml:"controllerAddr"`
	TLS            TLSConfig         `yaml:"tls"`
	Networks       map[string]string `yaml:"networks"` // network name -> bridge name
	Storage        StorageConfig     `yaml:"storage"`
}

type StorageConfig struct {
	Drivers map[string]DriverConfig `yaml:"drivers"`
}

type DriverConfig struct {
	Type       string            `yaml:"type"`
	Parameters map[string]string `yaml:"parameters"`
}

// DefaultControllerConfig returns a default controller configuration
func DefaultControllerConfig() *ControllerConfig {
	return &ControllerConfig{
		DatabasePath: "./kcore.db",
		ListenAddr:   ":9090",
		TLS: TLSConfig{
			CAFile:   "./certs/ca.crt",
			CertFile: "./certs/controller.crt",
			KeyFile:  "./certs/controller.key",
		},
		NodeNetworks: map[string]string{
			"default": "br0",
		},
	}
}

// DefaultNodeAgentConfig returns a default node agent configuration
func DefaultNodeAgentConfig() *NodeAgentConfig {
	return &NodeAgentConfig{
		NodeID:         "",
		ControllerAddr: "localhost:9090",
		TLS: TLSConfig{
			CAFile:   "./certs/ca.crt",
			CertFile: "./certs/node.crt",
			KeyFile:  "./certs/node.key",
		},
		Networks: map[string]string{
			"default": "br0",
		},
		Storage: StorageConfig{
			Drivers: map[string]DriverConfig{
				"local-dir": {
					Type: "local-dir",
					Parameters: map[string]string{
						"path": "/var/lib/kcode/disks",
					},
				},
			},
		},
	}
}

// LoadControllerConfig loads controller config from YAML file
func LoadControllerConfig(path string) (*ControllerConfig, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("failed to open config file: %w", err)
	}
	defer f.Close()

	var cfg ControllerConfig
	if err := yaml.NewDecoder(f).Decode(&cfg); err != nil {
		return nil, fmt.Errorf("failed to parse config: %w", err)
	}

	// Apply defaults
	if cfg.DatabasePath == "" {
		cfg.DatabasePath = DefaultControllerConfig().DatabasePath
	}
	if cfg.ListenAddr == "" {
		cfg.ListenAddr = DefaultControllerConfig().ListenAddr
	}
	if len(cfg.NodeNetworks) == 0 {
		cfg.NodeNetworks = DefaultControllerConfig().NodeNetworks
	}

	return &cfg, nil
}

// LoadNodeAgentConfig loads node agent config from YAML file
func LoadNodeAgentConfig(path string) (*NodeAgentConfig, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("failed to open config file: %w", err)
	}
	defer f.Close()

	var cfg NodeAgentConfig
	if err := yaml.NewDecoder(f).Decode(&cfg); err != nil {
		return nil, fmt.Errorf("failed to parse config: %w", err)
	}

	// Apply defaults
	if cfg.NodeID == "" {
		hostname, _ := os.Hostname()
		cfg.NodeID = strings.ToLower(hostname)
	}
	if cfg.ControllerAddr == "" {
		cfg.ControllerAddr = DefaultNodeAgentConfig().ControllerAddr
	}
	if len(cfg.Networks) == 0 {
		cfg.Networks = DefaultNodeAgentConfig().Networks
	}

	return &cfg, nil
}
