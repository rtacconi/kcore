package main

import (
	"testing"
)

func TestParseManifestKind(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		yaml    string
		want    string
		wantErr bool
	}{
		{
			name: "VM kind",
			yaml: `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test-vm`,
			want: "VM",
		},
		{
			name: "VirtualMachine kind",
			yaml: `apiVersion: kcore.io/v1
kind: VirtualMachine
metadata:
  name: test-vm`,
			want: "VirtualMachine",
		},
		{
			name:    "invalid yaml",
			yaml:    `{{{not yaml`,
			wantErr: true,
		},
		{
			name: "empty kind",
			yaml: `apiVersion: kcore.io/v1
metadata:
  name: test-vm`,
			want: "",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got, err := parseManifestKind([]byte(tt.yaml))
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error, got nil")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("got kind=%q, want=%q", got, tt.want)
			}
		})
	}
}

func TestIsVMKind(t *testing.T) {
	t.Parallel()

	tests := []struct {
		kind string
		want bool
	}{
		{"VM", true},
		{"vm", true},
		{"Vm", true},
		{"VirtualMachine", true},
		{"virtualmachine", true},
		{"VIRTUALMACHINE", true},
		{"Network", false},
		{"", false},
		{"VMachine", false},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.kind, func(t *testing.T) {
			t.Parallel()
			if got := isVMKind(tt.kind); got != tt.want {
				t.Fatalf("isVMKind(%q)=%v, want=%v", tt.kind, got, tt.want)
			}
		})
	}
}

func TestParseVMManifest_Valid(t *testing.T) {
	t.Parallel()

	yaml := `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: debian12-test
  labels:
    env: production
spec:
  cpu: 1
  memory: 2G
  image: https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2
  enableKcoreLogin: true
  nics:
    - network: default
      model: virtio
`
	m, err := parseVMManifest([]byte(yaml))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if m.APIVersion != "kcore.io/v1" {
		t.Errorf("apiVersion=%q, want kcore.io/v1", m.APIVersion)
	}
	if m.Kind != "VM" {
		t.Errorf("kind=%q, want VM", m.Kind)
	}
	if m.Metadata.Name != "debian12-test" {
		t.Errorf("name=%q, want debian12-test", m.Metadata.Name)
	}
	if m.Metadata.Labels["env"] != "production" {
		t.Errorf("labels[env]=%q, want production", m.Metadata.Labels["env"])
	}
	if m.Spec.CPU != 1 {
		t.Errorf("cpu=%d, want 1", m.Spec.CPU)
	}
	if m.Spec.Memory != "2G" {
		t.Errorf("memory=%q, want 2G", m.Spec.Memory)
	}
	if m.Spec.Image != "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2" {
		t.Errorf("image=%q, want debian URL", m.Spec.Image)
	}
	if m.Spec.EnableKcoreLogin == nil || !*m.Spec.EnableKcoreLogin {
		t.Error("enableKcoreLogin should be true")
	}
	if len(m.Spec.Nics) != 1 {
		t.Fatalf("nics count=%d, want 1", len(m.Spec.Nics))
	}
	if m.Spec.Nics[0].Network != "default" {
		t.Errorf("nic network=%q, want default", m.Spec.Nics[0].Network)
	}
	if m.Spec.Nics[0].Model != "virtio" {
		t.Errorf("nic model=%q, want virtio", m.Spec.Nics[0].Model)
	}
}

func TestParseVMManifest_EnableKcoreLoginDefault(t *testing.T) {
	t.Parallel()

	yaml := `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test-vm
spec:
  cpu: 2
  memory: 4G
`
	m, err := parseVMManifest([]byte(yaml))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if m.Spec.EnableKcoreLogin != nil {
		t.Errorf("enableKcoreLogin should be nil when not specified, got %v", *m.Spec.EnableKcoreLogin)
	}
}

func TestParseVMManifest_CustomCloudInit(t *testing.T) {
	t.Parallel()

	yaml := `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: custom-ci-vm
spec:
  cpu: 1
  memory: 1G
  image: https://example.com/image.qcow2
  cloudInit: |
    #cloud-config
    users:
      - name: myuser
        sudo: ALL=(ALL) NOPASSWD:ALL
`
	m, err := parseVMManifest([]byte(yaml))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if m.Spec.CloudInit == "" {
		t.Fatal("cloudInit should not be empty")
	}
	if m.Spec.CloudInit[:13] != "#cloud-config" {
		t.Errorf("cloudInit should start with #cloud-config, got: %q", m.Spec.CloudInit[:20])
	}
}

func TestParseVMManifest_ValidationErrors(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		yaml string
	}{
		{
			name: "missing name",
			yaml: `apiVersion: kcore.io/v1
kind: VM
metadata: {}
spec:
  cpu: 1
  memory: 2G`,
		},
		{
			name: "zero cpu",
			yaml: `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test
spec:
  cpu: 0
  memory: 2G`,
		},
		{
			name: "negative cpu",
			yaml: `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test
spec:
  cpu: -1
  memory: 2G`,
		},
		{
			name: "missing memory",
			yaml: `apiVersion: kcore.io/v1
kind: VM
metadata:
  name: test
spec:
  cpu: 1`,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			_, err := parseVMManifest([]byte(tt.yaml))
			if err == nil {
				t.Fatal("expected validation error, got nil")
			}
		})
	}
}
