package main

import (
	"os"
	"path/filepath"
	"testing"
)

func TestInitClusterCmd_MissingName(t *testing.T) {
	cmd := newInitCmd()
	cmd.SetArgs([]string{"cluster"})
	err := cmd.Execute()
	if err == nil {
		t.Error("expected error when --name is missing")
	}
}

func TestInitClusterCmd_CreatesFiles(t *testing.T) {
	tmpDir := t.TempDir()
	cmd := newInitCmd()
	cmd.SetArgs([]string{"cluster", "--name", "test-cluster", "--base-path", tmpDir, "--controller-ip", "10.0.0.1"})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("init cluster: %v", err)
	}

	clusterDir := filepath.Join(tmpDir, "test-cluster")
	for _, f := range []string{"ca.key", "ca.crt", "controller.crt", "controller.key", "config.yaml"} {
		path := filepath.Join(clusterDir, f)
		if _, err := os.Stat(path); err != nil {
			t.Errorf("expected file %s to exist: %v", f, err)
		}
	}
}

func TestInitClusterCmd_AlreadyExists(t *testing.T) {
	tmpDir := t.TempDir()

	cmd1 := newInitCmd()
	cmd1.SetArgs([]string{"cluster", "--name", "existing", "--base-path", tmpDir})
	if err := cmd1.Execute(); err != nil {
		t.Fatalf("first init: %v", err)
	}

	cmd2 := newInitCmd()
	cmd2.SetArgs([]string{"cluster", "--name", "existing", "--base-path", tmpDir})
	err := cmd2.Execute()
	if err == nil {
		t.Error("expected error when cluster already exists")
	}
}

func TestInitClusterCmd_NoControllerIP(t *testing.T) {
	tmpDir := t.TempDir()
	cmd := newInitCmd()
	cmd.SetArgs([]string{"cluster", "--name", "no-ip-cluster", "--base-path", tmpDir})
	if err := cmd.Execute(); err != nil {
		t.Fatalf("init cluster without controller-ip: %v", err)
	}

	clusterDir := filepath.Join(tmpDir, "no-ip-cluster")
	if _, err := os.Stat(filepath.Join(clusterDir, "ca.crt")); err != nil {
		t.Error("ca.crt should exist even without controller-ip")
	}
	if _, err := os.Stat(filepath.Join(clusterDir, "controller.crt")); err == nil {
		t.Error("controller.crt should NOT exist when controller-ip is not set")
	}
}
