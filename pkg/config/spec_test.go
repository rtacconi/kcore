package config

import (
	"strings"
	"testing"
)

func TestParseSizeBytes(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input   string
		want    int64
		wantErr bool
	}{
		{"100B", 100, false},
		{"1KB", 1024, false},
		{"1MB", 1024 * 1024, false},
		{"4GB", 4 * 1024 * 1024 * 1024, false},
		{"1TB", 1024 * 1024 * 1024 * 1024, false},
		{"1KiB", 1024, false},
		{"512MiB", 512 * 1024 * 1024, false},
		{"2GiB", 2 * 1024 * 1024 * 1024, false},
		{"1TiB", 1024 * 1024 * 1024 * 1024, false},
		// Short forms
		{"10G", 10 * 1024 * 1024 * 1024, false},
		{"256M", 256 * 1024 * 1024, false},
		{"1T", 1024 * 1024 * 1024 * 1024, false},
		// Errors
		{"", 0, true},
		{"abc", 0, true},
		{"10XB", 0, true},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.input, func(t *testing.T) {
			t.Parallel()
			got, err := ParseSizeBytes(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error for %q", tt.input)
				}
				return
			}
			if err != nil {
				t.Fatalf("ParseSizeBytes(%q): %v", tt.input, err)
			}
			if got != tt.want {
				t.Fatalf("ParseSizeBytes(%q)=%d, want %d", tt.input, got, tt.want)
			}
		})
	}
}

func TestNormalizeUnit(t *testing.T) {
	t.Parallel()

	tests := []struct {
		input string
		want  string
	}{
		{"G", "gb"},
		{"g", "gb"},
		{"M", "mb"},
		{"K", "kb"},
		{"T", "tb"},
		{"GB", "gb"},
		{"GiB", "gib"},
		{"MiB", "mib"},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.input, func(t *testing.T) {
			t.Parallel()
			got := normalizeUnit(tt.input)
			if got != tt.want {
				t.Fatalf("normalizeUnit(%q)=%q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

func TestParseVM(t *testing.T) {
	yaml := `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test-vm
  namespace: prod
  labels:
    env: staging
spec:
  cpu: 4
  memoryBytes: "8GiB"
  disks:
    - name: root
      sizeBytes: "40GiB"
      storageClassName: local-dir
  nics:
    - network: default
      model: virtio
`
	vm, err := ParseVM(strings.NewReader(yaml))
	if err != nil {
		t.Fatalf("ParseVM: %v", err)
	}
	if vm.Metadata.Name != "test-vm" {
		t.Errorf("name=%q", vm.Metadata.Name)
	}
	if vm.Metadata.Namespace != "prod" {
		t.Errorf("namespace=%q, want prod", vm.Metadata.Namespace)
	}
	if vm.Spec.CPU != 4 {
		t.Errorf("cpu=%d", vm.Spec.CPU)
	}
	if len(vm.Spec.Disks) != 1 {
		t.Fatalf("disks=%d, want 1", len(vm.Spec.Disks))
	}
	if vm.Spec.Disks[0].StorageClassName != "local-dir" {
		t.Errorf("disk storageClass=%q", vm.Spec.Disks[0].StorageClassName)
	}
}

func TestParseVM_DefaultNamespace(t *testing.T) {
	yaml := `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test-vm
spec:
  cpu: 1
  memoryBytes: "1GiB"
`
	vm, err := ParseVM(strings.NewReader(yaml))
	if err != nil {
		t.Fatalf("ParseVM: %v", err)
	}
	if vm.Metadata.Namespace != "default" {
		t.Errorf("namespace=%q, want default", vm.Metadata.Namespace)
	}
}

func TestParseVM_WrongKind(t *testing.T) {
	yaml := `apiVersion: kcore.io/v1
kind: Volume
metadata:
  name: test-vol
`
	_, err := ParseVM(strings.NewReader(yaml))
	if err == nil {
		t.Fatal("expected error for wrong kind")
	}
}
