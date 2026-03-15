package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

// Config represents kctl configuration
type Config struct {
	CurrentContext string             `yaml:"current-context,omitempty"`
	Contexts       map[string]Context `yaml:"contexts,omitempty"`
}

// Context represents a kcore cluster context
type Context struct {
	Controller string `yaml:"controller"`         // Controller/node address (host:port)
	Insecure   bool   `yaml:"insecure,omitempty"` // Skip TLS verification
	CertFile   string `yaml:"cert,omitempty"`     // Client certificate
	KeyFile    string `yaml:"key,omitempty"`      // Client key
	CAFile     string `yaml:"ca,omitempty"`       // CA certificate
}

// LoadConfig loads configuration from file
func LoadConfig(path string) (*Config, error) {
	// Expand ~ to home directory
	if strings.HasPrefix(path, "~/") {
		home, err := os.UserHomeDir()
		if err != nil {
			return nil, fmt.Errorf("failed to get home directory: %w", err)
		}
		path = filepath.Join(home, path[2:])
	}

	// If file doesn't exist, return empty config
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return &Config{
			Contexts: make(map[string]Context),
		}, nil
	}

	// Read file
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("failed to read config file: %w", err)
	}

	// Parse YAML
	var config Config
	if err := yaml.Unmarshal(data, &config); err != nil {
		return nil, fmt.Errorf("failed to parse config file: %w", err)
	}

	// Initialize contexts map if nil
	if config.Contexts == nil {
		config.Contexts = make(map[string]Context)
	}

	return &config, nil
}

// SaveConfig saves configuration to file
func SaveConfig(path string, config *Config) error {
	// Expand ~ to home directory
	if strings.HasPrefix(path, "~/") {
		home, err := os.UserHomeDir()
		if err != nil {
			return fmt.Errorf("failed to get home directory: %w", err)
		}
		path = filepath.Join(home, path[2:])
	}

	// Create directory if it doesn't exist
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create config directory: %w", err)
	}

	// Marshal to YAML
	data, err := yaml.Marshal(config)
	if err != nil {
		return fmt.Errorf("failed to marshal config: %w", err)
	}

	// Write file
	if err := os.WriteFile(path, data, 0600); err != nil {
		return fmt.Errorf("failed to write config file: %w", err)
	}

	return nil
}

// GetCurrentContext returns the current context or the first available one
func (c *Config) GetCurrentContext() (*Context, error) {
	if c.CurrentContext != "" {
		ctx, ok := c.Contexts[c.CurrentContext]
		if !ok {
			return nil, fmt.Errorf("current context '%s' not found in config", c.CurrentContext)
		}
		return &ctx, nil
	}

	// If no current context, try to use the first one
	if len(c.Contexts) > 0 {
		for _, ctx := range c.Contexts {
			return &ctx, nil
		}
	}

	return nil, fmt.Errorf("no contexts configured")
}

// NormalizeAddress ensures the address has a port
func NormalizeAddress(addr string) string {
	if addr == "" {
		return ""
	}
	
	// If address doesn't contain a colon, add default controller port
	if !strings.Contains(addr, ":") {
		return addr + ":9090"
	}
	
	return addr
}

// GetConfigPath returns the default config path
func GetConfigPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ".kcore/config"
	}
	return filepath.Join(home, ".kcore", "config")
}

// GetConnectionInfo extracts connection info from flags and config
// Priority: flag > config > error
func GetConnectionInfo(configPath, controllerFlag string, insecureFlag bool) (controller string, insecure bool, certFile, keyFile, caFile string, err error) {
	if controllerFlag != "" {
		if insecureFlag {
			return NormalizeAddress(controllerFlag), true, "", "", "", nil
		}
		return NormalizeAddress(controllerFlag), false, "certs/controller.crt", "certs/controller.key", "certs/ca.crt", nil
	}

	// Try to load from config
	config, err := LoadConfig(configPath)
	if err != nil {
		return "", false, "", "", "", fmt.Errorf("failed to load config: %w", err)
	}

	ctx, err := config.GetCurrentContext()
	if err != nil {
		return "", false, "", "", "", fmt.Errorf("no controller configured: use --controller flag or create config file at %s", configPath)
	}

	// Use context values
	controller = NormalizeAddress(ctx.Controller)
	insecure = ctx.Insecure || insecureFlag // Flag can override config
	certFile = ctx.CertFile
	keyFile = ctx.KeyFile
	caFile = ctx.CAFile

	// Default certs if not specified
	if certFile == "" {
		certFile = "certs/controller.crt"
	}
	if keyFile == "" {
		keyFile = "certs/controller.key"
	}
	if caFile == "" {
		caFile = "certs/ca.crt"
	}

	return controller, insecure, certFile, keyFile, caFile, nil
}

