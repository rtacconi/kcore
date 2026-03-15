package controlplaneclient

import (
	"testing"
)

func TestDial_EmptyAddress(t *testing.T) {
	_, _, err := Dial(Config{})
	if err == nil {
		t.Fatal("expected error for empty address")
	}
}

func TestDial_MissingCAFile(t *testing.T) {
	_, _, err := Dial(Config{
		Address: "localhost:9090",
		CAFile:  "/nonexistent/ca.crt",
	})
	if err == nil {
		t.Fatal("expected error for missing CA file")
	}
}

func TestConfig_Fields(t *testing.T) {
	cfg := Config{
		Address:    "10.0.0.1:9090",
		CertFile:   "/certs/client.crt",
		KeyFile:    "/certs/client.key",
		CAFile:     "/certs/ca.crt",
		Insecure:   true,
		ServerName: "controller",
	}

	if cfg.Address != "10.0.0.1:9090" {
		t.Errorf("Address=%q", cfg.Address)
	}
	if !cfg.Insecure {
		t.Error("Insecure should be true")
	}
	if cfg.ServerName != "controller" {
		t.Errorf("ServerName=%q", cfg.ServerName)
	}
}
