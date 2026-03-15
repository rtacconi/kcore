package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDefaultControllerConfig(t *testing.T) {
	cfg := DefaultControllerConfig()
	if cfg.ListenAddr != ":9090" {
		t.Errorf("ListenAddr=%q, want :9090", cfg.ListenAddr)
	}
	if cfg.DatabasePath != "./kcore.db" {
		t.Errorf("DatabasePath=%q, want ./kcore.db", cfg.DatabasePath)
	}
	if cfg.NodeNetworks["default"] != "br0" {
		t.Errorf("NodeNetworks[default]=%q, want br0", cfg.NodeNetworks["default"])
	}
}

func TestDefaultNodeAgentConfig(t *testing.T) {
	cfg := DefaultNodeAgentConfig()
	if cfg.ListenAddr != ":9091" {
		t.Errorf("ListenAddr=%q, want :9091", cfg.ListenAddr)
	}
	if cfg.ControllerAddr != "" {
		t.Errorf("ControllerAddr=%q, want empty", cfg.ControllerAddr)
	}
	if cfg.Networks["default"] != "br0" {
		t.Errorf("Networks[default]=%q, want br0", cfg.Networks["default"])
	}
	drv, ok := cfg.Storage.Drivers["local-dir"]
	if !ok {
		t.Fatal("missing local-dir driver in defaults")
	}
	if drv.Type != "local-dir" {
		t.Errorf("driver type=%q, want local-dir", drv.Type)
	}
}

func TestLoadControllerConfig(t *testing.T) {
	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "controller.yaml")
	data := []byte(`listenAddr: ":8080"
databasePath: "/tmp/test.db"
tls:
  caFile: /etc/certs/ca.crt
  certFile: /etc/certs/ctrl.crt
  keyFile: /etc/certs/ctrl.key
`)
	os.WriteFile(cfgPath, data, 0600)

	cfg, err := LoadControllerConfig(cfgPath)
	if err != nil {
		t.Fatalf("LoadControllerConfig: %v", err)
	}
	if cfg.ListenAddr != ":8080" {
		t.Errorf("ListenAddr=%q, want :8080", cfg.ListenAddr)
	}
	if cfg.DatabasePath != "/tmp/test.db" {
		t.Errorf("DatabasePath=%q", cfg.DatabasePath)
	}
	if cfg.TLS.CAFile != "/etc/certs/ca.crt" {
		t.Errorf("TLS.CAFile=%q", cfg.TLS.CAFile)
	}
}

func TestLoadControllerConfig_Defaults(t *testing.T) {
	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "controller.yaml")
	os.WriteFile(cfgPath, []byte("tls:\n  caFile: /ca.crt\n"), 0600)

	cfg, err := LoadControllerConfig(cfgPath)
	if err != nil {
		t.Fatalf("LoadControllerConfig: %v", err)
	}
	if cfg.ListenAddr != ":9090" {
		t.Errorf("default ListenAddr=%q, want :9090", cfg.ListenAddr)
	}
	if cfg.DatabasePath != "./kcore.db" {
		t.Errorf("default DatabasePath=%q, want ./kcore.db", cfg.DatabasePath)
	}
}

func TestLoadNodeAgentConfig(t *testing.T) {
	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "node-agent.yaml")
	data := []byte(`nodeId: my-node
listenAddr: ":9999"
controllerAddr: "10.0.0.1:9090"
networks:
  default: virbr0
`)
	os.WriteFile(cfgPath, data, 0600)

	cfg, err := LoadNodeAgentConfig(cfgPath)
	if err != nil {
		t.Fatalf("LoadNodeAgentConfig: %v", err)
	}
	if cfg.NodeID != "my-node" {
		t.Errorf("NodeID=%q, want my-node", cfg.NodeID)
	}
	if cfg.ListenAddr != ":9999" {
		t.Errorf("ListenAddr=%q", cfg.ListenAddr)
	}
	if cfg.Networks["default"] != "virbr0" {
		t.Errorf("Networks[default]=%q, want virbr0", cfg.Networks["default"])
	}
}

func TestLoadNodeAgentConfig_DefaultNodeID(t *testing.T) {
	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "node-agent.yaml")
	os.WriteFile(cfgPath, []byte("listenAddr: \":9091\"\n"), 0600)

	cfg, err := LoadNodeAgentConfig(cfgPath)
	if err != nil {
		t.Fatalf("LoadNodeAgentConfig: %v", err)
	}
	if cfg.NodeID == "" {
		t.Error("NodeID should default to hostname, got empty")
	}
}

func TestLoadControllerConfig_MissingFile(t *testing.T) {
	_, err := LoadControllerConfig("/nonexistent/controller.yaml")
	if err == nil {
		t.Fatal("expected error for missing file")
	}
}
