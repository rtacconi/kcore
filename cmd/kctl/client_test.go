package main

import (
	"os"
	"testing"
)

func TestParseMemorySize(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input   string
		want    int64
		wantErr bool
	}{
		{"1G", 1 * 1024 * 1024 * 1024, false},
		{"2G", 2 * 1024 * 1024 * 1024, false},
		{"4g", 4 * 1024 * 1024 * 1024, false},
		{"512M", 512 * 1024 * 1024, false},
		{"1024m", 1024 * 1024 * 1024, false},
		{"64K", 64 * 1024, false},
		{"1k", 1024, false},
		{"", 0, true},
		{"G", 0, true},
		{"abc", 0, true},
		{"10X", 0, true},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.input, func(t *testing.T) {
			t.Parallel()
			got, err := parseMemorySize(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error for %q", tt.input)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("parseMemorySize(%q)=%d, want %d", tt.input, got, tt.want)
			}
		})
	}
}

func TestParseDiskSize(t *testing.T) {
	t.Parallel()

	got, err := parseDiskSize("10G")
	if err != nil {
		t.Fatalf("parseDiskSize: %v", err)
	}
	if got != 10*1024*1024*1024 {
		t.Errorf("got=%d", got)
	}
}

func TestFormatBytes(t *testing.T) {
	t.Parallel()

	tests := []struct {
		bytes int64
		want  string
	}{
		{0, "0 B"},
		{500, "500 B"},
		{1024, "1.0 KB"},
		{1536, "1.5 KB"},
		{1024 * 1024, "1.0 MB"},
		{1024 * 1024 * 1024, "1.0 GB"},
		{2 * 1024 * 1024 * 1024, "2.0 GB"},
		{int64(1024) * 1024 * 1024 * 1024, "1.0 TB"},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.want, func(t *testing.T) {
			t.Parallel()
			got := formatBytes(tt.bytes)
			if got != tt.want {
				t.Errorf("formatBytes(%d)=%q, want %q", tt.bytes, got, tt.want)
			}
		})
	}
}

func TestGetConfigPath(t *testing.T) {
	t.Parallel()
	path := GetConfigPath()
	if path == "" {
		t.Error("GetConfigPath returned empty")
	}
}

func TestGetCurrentContext_NoContexts(t *testing.T) {
	t.Parallel()
	cfg := &Config{Contexts: map[string]Context{}}
	_, err := cfg.GetCurrentContext()
	if err == nil {
		t.Fatal("expected error with no contexts")
	}
}

func TestGetCurrentContext_CurrentSet(t *testing.T) {
	t.Parallel()
	cfg := &Config{
		CurrentContext: "prod",
		Contexts: map[string]Context{
			"prod": {Controller: "10.0.0.1:9090", Insecure: false},
		},
	}
	ctx, err := cfg.GetCurrentContext()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ctx.Controller != "10.0.0.1:9090" {
		t.Errorf("Controller=%q", ctx.Controller)
	}
}

func TestGetCurrentContext_CurrentNotFound(t *testing.T) {
	t.Parallel()
	cfg := &Config{
		CurrentContext: "nonexistent",
		Contexts: map[string]Context{
			"prod": {Controller: "10.0.0.1:9090"},
		},
	}
	_, err := cfg.GetCurrentContext()
	if err == nil {
		t.Fatal("expected error for missing current context")
	}
}

func TestGetCurrentContext_FallbackToFirst(t *testing.T) {
	t.Parallel()
	cfg := &Config{
		Contexts: map[string]Context{
			"dev": {Controller: "localhost:9090", Insecure: true},
		},
	}
	ctx, err := cfg.GetCurrentContext()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ctx.Controller != "localhost:9090" {
		t.Errorf("Controller=%q", ctx.Controller)
	}
}

func TestGetConnectionInfo_FromConfig(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	cfgPath := dir + "/config.yaml"
	data := []byte(`current-context: test
contexts:
  test:
    controller: "10.0.0.5:9090"
    insecure: true
`)
	if err := writeTestFile(cfgPath, data); err != nil {
		t.Fatalf("write config: %v", err)
	}

	addr, insecure, _, _, _, err := GetConnectionInfo(cfgPath, "", false)
	if err != nil {
		t.Fatalf("GetConnectionInfo: %v", err)
	}
	if addr != "10.0.0.5:9090" {
		t.Errorf("addr=%q", addr)
	}
	if !insecure {
		t.Error("insecure should be true from config")
	}
}

func TestGetConnectionInfo_FlagOverridesConfig(t *testing.T) {
	t.Parallel()

	addr, insecure, _, _, _, err := GetConnectionInfo("", "192.168.1.1:8080", true)
	if err != nil {
		t.Fatalf("GetConnectionInfo: %v", err)
	}
	if addr != "192.168.1.1:8080" {
		t.Errorf("addr=%q", addr)
	}
	if !insecure {
		t.Error("insecure should be true")
	}
}

func writeTestFile(path string, data []byte) error {
	return os.WriteFile(path, data, 0600)
}
