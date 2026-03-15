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

// --- NodeInstall manifest tests ---

func TestIsNodeInstallKind(t *testing.T) {
	t.Parallel()
	if !isNodeInstallKind("NodeInstall") {
		t.Error("should match NodeInstall")
	}
	if !isNodeInstallKind("nodeinstall") {
		t.Error("should match nodeinstall (case-insensitive)")
	}
	if isNodeInstallKind("VM") {
		t.Error("should not match VM")
	}
}

func TestParseNodeInstallManifest_Valid(t *testing.T) {
	t.Parallel()
	yaml := `apiVersion: kcore.io/v1
kind: NodeInstall
metadata:
  name: new-node-01
spec:
  address: "192.168.40.108:9090"
  disks:
    - device: sda
      role: os
    - device: sdb
      role: storage
  hostname: kvm-node-02
  rootPassword: kcore
  sshKeys:
    - "ssh-ed25519 AAAA... user@host"
  runController: true
  controllerAddress: "192.168.40.100:9090"
`
	m, err := parseNodeInstallManifest([]byte(yaml))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if m.Metadata.Name != "new-node-01" {
		t.Errorf("name = %q", m.Metadata.Name)
	}
	if len(m.Spec.Disks) != 2 {
		t.Fatalf("disks count = %d, want 2", len(m.Spec.Disks))
	}
	if m.Spec.Disks[0].Role != "os" {
		t.Errorf("disk[0].Role = %q", m.Spec.Disks[0].Role)
	}
	if !m.Spec.RunController {
		t.Error("runController should be true")
	}
	if m.Spec.ControllerAddress != "192.168.40.100:9090" {
		t.Errorf("controllerAddress = %q", m.Spec.ControllerAddress)
	}
}

func TestParseNodeInstallManifest_NoOSDisk(t *testing.T) {
	t.Parallel()
	yaml := `apiVersion: kcore.io/v1
kind: NodeInstall
metadata:
  name: bad-node
spec:
  disks:
    - device: sdb
      role: storage
`
	_, err := parseNodeInstallManifest([]byte(yaml))
	if err == nil {
		t.Error("expected error for missing os disk")
	}
}

func TestParseNodeInstallManifest_NoDisks(t *testing.T) {
	t.Parallel()
	yaml := `apiVersion: kcore.io/v1
kind: NodeInstall
metadata:
  name: bad-node
spec: {}
`
	_, err := parseNodeInstallManifest([]byte(yaml))
	if err == nil {
		t.Error("expected error for empty disks")
	}
}

// --- NodeNetwork manifest tests ---

func TestIsNodeNetworkKind(t *testing.T) {
	t.Parallel()
	if !isNodeNetworkKind("NodeNetwork") {
		t.Error("should match")
	}
	if isNodeNetworkKind("VM") {
		t.Error("should not match")
	}
}

func TestParseNodeNetworkManifest_Valid(t *testing.T) {
	t.Parallel()
	yaml := `apiVersion: kcore.io/v1
kind: NodeNetwork
metadata:
  name: node-01-network
spec:
  nodeId: kvm-node-01
  bridges:
    - name: br0
      memberPorts: [enp0s31f6]
      dhcp: true
  vlans:
    - parentInterface: enp0s31f6
      vlanId: 100
      ipAddress: 10.0.100.10
      subnetMask: "255.255.255.0"
  dnsServers: "8.8.8.8,8.8.4.4"
  applyNow: true
`
	m, err := parseNodeNetworkManifest([]byte(yaml))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if m.Spec.NodeID != "kvm-node-01" {
		t.Errorf("nodeId = %q", m.Spec.NodeID)
	}
	if len(m.Spec.Bridges) != 1 {
		t.Fatalf("bridges count = %d", len(m.Spec.Bridges))
	}
	if !m.Spec.Bridges[0].DHCP {
		t.Error("bridge should have dhcp=true")
	}
	if len(m.Spec.Vlans) != 1 {
		t.Fatalf("vlans count = %d", len(m.Spec.Vlans))
	}
	if m.Spec.Vlans[0].VlanID != 100 {
		t.Errorf("vlanId = %d", m.Spec.Vlans[0].VlanID)
	}
}

func TestParseNodeNetworkManifest_MissingNodeID(t *testing.T) {
	t.Parallel()
	yaml := `apiVersion: kcore.io/v1
kind: NodeNetwork
metadata:
  name: bad
spec: {}
`
	_, err := parseNodeNetworkManifest([]byte(yaml))
	if err == nil {
		t.Error("expected error for missing nodeId")
	}
}

// --- NodeConfig manifest tests ---

func TestIsNodeConfigKind(t *testing.T) {
	t.Parallel()
	if !isNodeConfigKind("NodeConfig") {
		t.Error("should match")
	}
	if isNodeConfigKind("VM") {
		t.Error("should not match")
	}
}

func TestParseNodeConfigManifest_Valid(t *testing.T) {
	t.Parallel()
	yaml := `apiVersion: kcore.io/v1
kind: NodeConfig
metadata:
  name: config-01
spec:
  nodeId: kvm-node-01
  configurationNix: "{ ... }"
  rebuild: true
`
	m, err := parseNodeConfigManifest([]byte(yaml))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if m.Spec.NodeID != "kvm-node-01" {
		t.Errorf("nodeId = %q", m.Spec.NodeID)
	}
	if !m.Spec.Rebuild {
		t.Error("rebuild should be true")
	}
}

func TestParseNodeConfigManifest_MissingNodeID(t *testing.T) {
	t.Parallel()
	yaml := `apiVersion: kcore.io/v1
kind: NodeConfig
metadata:
  name: bad
spec: {}
`
	_, err := parseNodeConfigManifest([]byte(yaml))
	if err == nil {
		t.Error("expected error for missing nodeId")
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
