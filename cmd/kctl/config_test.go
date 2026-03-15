package main

import (
	"os"
	"path/filepath"
	"testing"
)

func TestNormalizeAddress(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input string
		want  string
	}{
		{"192.168.1.1", "192.168.1.1:9091"},
		{"192.168.1.1:9091", "192.168.1.1:9091"},
		{"myhost:8080", "myhost:8080"},
		{"myhost", "myhost:9091"},
		{"", ""},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.input, func(t *testing.T) {
			t.Parallel()
			got := NormalizeAddress(tt.input)
			if got != tt.want {
				t.Fatalf("NormalizeAddress(%q)=%q, want=%q", tt.input, got, tt.want)
			}
		})
	}
}

func TestGetConnectionInfo_InsecureSkipsCerts(t *testing.T) {
	t.Parallel()

	addr, insecure, cert, key, ca, err := GetConnectionInfo("", "192.168.40.107", true)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if addr != "192.168.40.107:9091" {
		t.Errorf("addr=%q, want 192.168.40.107:9091", addr)
	}
	if !insecure {
		t.Error("insecure should be true")
	}
	if cert != "" || key != "" || ca != "" {
		t.Errorf("certs should be empty in insecure mode, got cert=%q key=%q ca=%q", cert, key, ca)
	}
}

func TestGetConnectionInfo_SecureReturnsCerts(t *testing.T) {
	t.Parallel()

	addr, insecure, cert, key, ca, err := GetConnectionInfo("", "192.168.40.107", false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if addr != "192.168.40.107:9091" {
		t.Errorf("addr=%q, want 192.168.40.107:9091", addr)
	}
	if insecure {
		t.Error("insecure should be false")
	}
	if cert != "certs/controller.crt" {
		t.Errorf("cert=%q, want certs/controller.crt", cert)
	}
	if key != "certs/controller.key" {
		t.Errorf("key=%q, want certs/controller.key", key)
	}
	if ca != "certs/ca.crt" {
		t.Errorf("ca=%q, want certs/ca.crt", ca)
	}
}

func TestLoadConfig_MissingFile(t *testing.T) {
	t.Parallel()

	cfg, err := LoadConfig("/nonexistent/path/config.yaml")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if cfg == nil {
		t.Fatal("config should not be nil")
	}
	if len(cfg.Contexts) != 0 {
		t.Errorf("contexts should be empty, got %d", len(cfg.Contexts))
	}
}

func TestLoadConfig_ValidFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "config.yaml")
	data := []byte(`current-context: dev
contexts:
  dev:
    controller: "192.168.1.10:9091"
    insecure: true
  prod:
    controller: "10.0.0.1:9091"
    cert: "/etc/kcore/certs/node.crt"
    key: "/etc/kcore/certs/node.key"
    ca: "/etc/kcore/certs/ca.crt"
`)
	if err := os.WriteFile(cfgPath, data, 0600); err != nil {
		t.Fatalf("write config: %v", err)
	}

	cfg, err := LoadConfig(cfgPath)
	if err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}

	if cfg.CurrentContext != "dev" {
		t.Errorf("current-context=%q, want dev", cfg.CurrentContext)
	}
	if len(cfg.Contexts) != 2 {
		t.Fatalf("contexts count=%d, want 2", len(cfg.Contexts))
	}

	dev := cfg.Contexts["dev"]
	if dev.Controller != "192.168.1.10:9091" {
		t.Errorf("dev controller=%q", dev.Controller)
	}
	if !dev.Insecure {
		t.Error("dev should be insecure")
	}
}

func TestSaveAndLoadConfig(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "subdir", "config.yaml")

	cfg := &Config{
		CurrentContext: "test",
		Contexts: map[string]Context{
			"test": {
				Controller: "localhost:9091",
				Insecure:   true,
			},
		},
	}

	if err := SaveConfig(cfgPath, cfg); err != nil {
		t.Fatalf("SaveConfig: %v", err)
	}

	loaded, err := LoadConfig(cfgPath)
	if err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}

	if loaded.CurrentContext != "test" {
		t.Errorf("current-context=%q, want test", loaded.CurrentContext)
	}

	ctx, ok := loaded.Contexts["test"]
	if !ok {
		t.Fatal("context 'test' not found")
	}
	if ctx.Controller != "localhost:9091" {
		t.Errorf("controller=%q, want localhost:9091", ctx.Controller)
	}
	if !ctx.Insecure {
		t.Error("insecure should be true")
	}
}
