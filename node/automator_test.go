package node

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"

	nodepb "github.com/kcore/kcore/api/node"
)

// MockSystemExecutor provides canned responses for testing.
type MockSystemExecutor struct {
	CommandLog   []string
	Responses    map[string]struct{ Out, Err string }
	Errors       map[string]error
	Files        map[string][]byte
	BlockDevices map[string]bool
}

func NewMockExecutor() *MockSystemExecutor {
	return &MockSystemExecutor{
		Responses:    make(map[string]struct{ Out, Err string }),
		Errors:       make(map[string]error),
		Files:        make(map[string][]byte),
		BlockDevices: make(map[string]bool),
	}
}

func (m *MockSystemExecutor) RunCommand(_ context.Context, name string, args ...string) (string, string, error) {
	key := name
	m.CommandLog = append(m.CommandLog, fmt.Sprintf("%s %v", name, args))
	if resp, ok := m.Responses[key]; ok {
		if err, ok := m.Errors[key]; ok {
			return resp.Out, resp.Err, err
		}
		return resp.Out, resp.Err, nil
	}
	if err, ok := m.Errors[key]; ok {
		return "", "", err
	}
	return "", "", nil
}

func (m *MockSystemExecutor) ReadFile(path string) ([]byte, error) {
	if data, ok := m.Files[path]; ok {
		return data, nil
	}
	return nil, os.ErrNotExist
}

func (m *MockSystemExecutor) WriteFile(path string, data []byte, _ os.FileMode) error {
	m.Files[path] = data
	return nil
}

func (m *MockSystemExecutor) FileExists(path string) bool {
	_, ok := m.Files[path]
	return ok
}

func (m *MockSystemExecutor) MkdirAll(_ string, _ os.FileMode) error {
	return nil
}

// --- ListDisks tests ---

func TestListDisks_ParsesLsblk(t *testing.T) {
	mock := NewMockExecutor()
	mock.Responses["lsblk"] = struct{ Out, Err string }{
		Out: `{"blockdevices":[{"name":"sda","size":500107862016,"type":"disk","model":"Samsung SSD","serial":"S123","rm":false},{"name":"sdb","size":1000204886016,"type":"disk","model":"WD HDD","serial":"W456","rm":false}]}`,
	}

	cfg := AutomatorConfig{InstalledMarkerPath: filepath.Join(t.TempDir(), "installed")}
	srv := NewAutomatorServer(mock, cfg)

	resp, err := srv.ListDisks(context.Background(), &nodepb.AutomatorListDisksRequest{})
	if err != nil {
		t.Fatalf("ListDisks: %v", err)
	}
	if len(resp.Disks) != 2 {
		t.Fatalf("expected 2 disks, got %d", len(resp.Disks))
	}
	if resp.Disks[0].Name != "sda" {
		t.Errorf("disk[0].Name = %q, want sda", resp.Disks[0].Name)
	}
	if resp.Disks[0].SizeBytes != 500107862016 {
		t.Errorf("disk[0].SizeBytes = %d, want 500107862016", resp.Disks[0].SizeBytes)
	}
	if resp.Disks[0].Model != "Samsung SSD" {
		t.Errorf("disk[0].Model = %q, want Samsung SSD", resp.Disks[0].Model)
	}
	if !resp.BootstrapMode {
		t.Error("expected BootstrapMode=true when marker does not exist")
	}
}

func TestListDisks_InstalledMode(t *testing.T) {
	mock := NewMockExecutor()
	mock.Responses["lsblk"] = struct{ Out, Err string }{
		Out: `{"blockdevices":[{"name":"sda","size":100,"type":"disk","model":"","serial":"","rm":false}]}`,
	}
	markerPath := filepath.Join(t.TempDir(), "installed")
	mock.Files[markerPath] = []byte("yes")

	srv := NewAutomatorServer(mock, AutomatorConfig{InstalledMarkerPath: markerPath})
	resp, err := srv.ListDisks(context.Background(), &nodepb.AutomatorListDisksRequest{})
	if err != nil {
		t.Fatalf("ListDisks: %v", err)
	}
	if resp.BootstrapMode {
		t.Error("expected BootstrapMode=false when marker exists")
	}
}

func TestListDisks_FiltersNonDisk(t *testing.T) {
	mock := NewMockExecutor()
	mock.Responses["lsblk"] = struct{ Out, Err string }{
		Out: `{"blockdevices":[{"name":"sda","size":100,"type":"disk","model":"","serial":"","rm":false},{"name":"sda1","size":50,"type":"part","model":"","serial":"","rm":false}]}`,
	}

	srv := NewAutomatorServer(mock, AutomatorConfig{InstalledMarkerPath: "/nonexistent"})
	resp, err := srv.ListDisks(context.Background(), &nodepb.AutomatorListDisksRequest{})
	if err != nil {
		t.Fatalf("ListDisks: %v", err)
	}
	if len(resp.Disks) != 1 {
		t.Fatalf("expected 1 disk (filtered partitions), got %d", len(resp.Disks))
	}
}

func TestListDisks_LsblkError(t *testing.T) {
	mock := NewMockExecutor()
	mock.Errors["lsblk"] = fmt.Errorf("command not found")

	srv := NewAutomatorServer(mock, AutomatorConfig{InstalledMarkerPath: "/nonexistent"})
	_, err := srv.ListDisks(context.Background(), &nodepb.AutomatorListDisksRequest{})
	if err == nil {
		t.Fatal("expected error when lsblk fails")
	}
}

// --- ListNetworkInterfaces tests ---

func TestListNetworkInterfaces_ParsesIPOutput(t *testing.T) {
	mock := NewMockExecutor()
	mock.Responses["ip"] = struct{ Out, Err string }{
		Out: `[{"ifname":"lo","address":"00:00:00:00:00:00","flags":["LOOPBACK","UP"],"link_type":"loopback"},{"ifname":"eth0","address":"aa:bb:cc:dd:ee:ff","flags":["UP","BROADCAST"],"link_type":"ether"},{"ifname":"br0","address":"11:22:33:44:55:66","flags":["UP"],"link_type":"bridge"}]`,
	}
	mock.Responses["ethtool"] = struct{ Out, Err string }{Out: "Speed: 1000Mb/s\n"}

	srv := NewAutomatorServer(mock, AutomatorConfig{InstalledMarkerPath: "/nonexistent"})

	resp, err := srv.ListNetworkInterfaces(context.Background(), &nodepb.AutomatorListNICsRequest{})
	if err != nil {
		t.Fatalf("ListNetworkInterfaces: %v", err)
	}

	if len(resp.Interfaces) != 2 {
		t.Fatalf("expected 2 interfaces (lo filtered), got %d", len(resp.Interfaces))
	}

	var eth0 *nodepb.AutomatorNIC
	for _, n := range resp.Interfaces {
		if n.Name == "eth0" {
			eth0 = n
		}
	}
	if eth0 == nil {
		t.Fatal("eth0 not found in response")
	}
	if eth0.MacAddress != "aa:bb:cc:dd:ee:ff" {
		t.Errorf("eth0.MacAddress = %q", eth0.MacAddress)
	}
	if !eth0.IsUp {
		t.Error("eth0 should be UP")
	}

	var br0Found bool
	for _, n := range resp.Interfaces {
		if n.Name == "br0" {
			br0Found = true
			if !n.IsVirtual {
				t.Error("br0 should be virtual")
			}
		}
	}
	if !br0Found {
		t.Error("br0 not found")
	}
}

// --- InstallToDisk tests ---

func TestInstallToDisk_RejectsInstalledNode(t *testing.T) {
	mock := NewMockExecutor()
	markerPath := filepath.Join(t.TempDir(), "installed")
	mock.Files[markerPath] = []byte("yes")

	srv := NewAutomatorServer(mock, AutomatorConfig{
		InstalledMarkerPath: markerPath,
		InstallScriptPath:   "fake-install",
	})

	resp, err := srv.InstallToDisk(context.Background(), &nodepb.InstallToDiskRequest{
		Disks:    []*nodepb.AutomatorDiskConfig{{Device: "sda", Role: "os"}},
		Hostname: "test",
	})
	if err != nil {
		t.Fatalf("InstallToDisk: %v", err)
	}
	if resp.Status != "FAILED" {
		t.Errorf("status = %q, want FAILED", resp.Status)
	}
}

func TestInstallToDisk_RequiresOSDisk(t *testing.T) {
	mock := NewMockExecutor()

	srv := NewAutomatorServer(mock, AutomatorConfig{
		InstalledMarkerPath: "/nonexistent",
		InstallScriptPath:   "fake-install",
	})

	resp, err := srv.InstallToDisk(context.Background(), &nodepb.InstallToDiskRequest{
		Disks:    []*nodepb.AutomatorDiskConfig{{Device: "sdb", Role: "storage"}},
		Hostname: "test",
	})
	if err != nil {
		t.Fatalf("InstallToDisk: %v", err)
	}
	if resp.Status != "FAILED" {
		t.Errorf("status = %q, want FAILED (no os disk)", resp.Status)
	}
}

func TestInstallToDisk_InvalidRole(t *testing.T) {
	mock := NewMockExecutor()

	srv := NewAutomatorServer(mock, AutomatorConfig{
		InstalledMarkerPath: "/nonexistent",
		InstallScriptPath:   "fake-install",
	})

	resp, err := srv.InstallToDisk(context.Background(), &nodepb.InstallToDiskRequest{
		Disks:    []*nodepb.AutomatorDiskConfig{{Device: "sda", Role: "unknown"}},
		Hostname: "test",
	})
	if err != nil {
		t.Fatalf("InstallToDisk: %v", err)
	}
	if resp.Status != "FAILED" {
		t.Errorf("status = %q, want FAILED (invalid role)", resp.Status)
	}
}

func TestInstallToDisk_StartsInstall(t *testing.T) {
	mock := NewMockExecutor()

	srv := NewAutomatorServer(mock, AutomatorConfig{
		InstalledMarkerPath: "/nonexistent",
		InstallScriptPath:   "echo",
		StatusFilePath:      filepath.Join(t.TempDir(), "status"),
	})

	resp, err := srv.InstallToDisk(context.Background(), &nodepb.InstallToDiskRequest{
		Disks: []*nodepb.AutomatorDiskConfig{
			{Device: "sda", Role: "os"},
			{Device: "sdb", Role: "storage"},
		},
		Hostname:          "test-node",
		RunController:     true,
		ControllerAddress: "192.168.1.1:9090",
	})
	if err != nil {
		t.Fatalf("InstallToDisk: %v", err)
	}
	if resp.Status != "STARTED" {
		t.Errorf("status = %q, want STARTED", resp.Status)
	}
	if resp.InstallId == "" {
		t.Error("expected non-empty install_id")
	}
}

func TestInstallToDisk_RejectsSecondInstall(t *testing.T) {
	mock := NewMockExecutor()

	srv := NewAutomatorServer(mock, AutomatorConfig{
		InstalledMarkerPath: "/nonexistent",
		InstallScriptPath:   "sleep",
		StatusFilePath:      filepath.Join(t.TempDir(), "status"),
	})

	_, err := srv.InstallToDisk(context.Background(), &nodepb.InstallToDiskRequest{
		Disks: []*nodepb.AutomatorDiskConfig{{Device: "sda", Role: "os"}},
	})
	if err != nil {
		t.Fatal(err)
	}

	resp2, err := srv.InstallToDisk(context.Background(), &nodepb.InstallToDiskRequest{
		Disks: []*nodepb.AutomatorDiskConfig{{Device: "sda", Role: "os"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	if resp2.Status != "FAILED" {
		t.Errorf("second install status = %q, want FAILED", resp2.Status)
	}
}

// --- GetInstallStatus tests ---

func TestGetInstallStatus_NoInstall(t *testing.T) {
	mock := NewMockExecutor()
	srv := NewAutomatorServer(mock, AutomatorConfig{InstalledMarkerPath: "/nonexistent"})

	resp, err := srv.GetInstallStatus(context.Background(), &nodepb.AutomatorGetInstallStatusRequest{})
	if err != nil {
		t.Fatalf("GetInstallStatus: %v", err)
	}
	if resp.Phase != "NONE" {
		t.Errorf("phase = %q, want NONE", resp.Phase)
	}
}

// --- UpdateNixConfig tests ---

func TestUpdateNixConfig_WritesFile(t *testing.T) {
	mock := NewMockExecutor()
	cfgPath := filepath.Join(t.TempDir(), "configuration.nix")

	srv := NewAutomatorServer(mock, AutomatorConfig{
		InstalledMarkerPath: "/nonexistent",
		ConfigNixPath:       cfgPath,
	})

	nixContent := "{ config, pkgs, ... }: { environment.systemPackages = []; }"
	resp, err := srv.UpdateNixConfig(context.Background(), &nodepb.AutomatorUpdateNixConfigRequest{
		ConfigurationNix: nixContent,
	})
	if err != nil {
		t.Fatalf("UpdateNixConfig: %v", err)
	}
	if !resp.Success {
		t.Errorf("success = false, message: %s", resp.Message)
	}

	written, ok := mock.Files[cfgPath]
	if !ok {
		t.Fatal("config file not written")
	}
	if string(written) != nixContent {
		t.Errorf("written content = %q, want %q", string(written), nixContent)
	}
}

func TestUpdateNixConfig_EmptyReject(t *testing.T) {
	mock := NewMockExecutor()
	srv := NewAutomatorServer(mock, AutomatorConfig{ConfigNixPath: "/tmp/nix.nix"})

	resp, err := srv.UpdateNixConfig(context.Background(), &nodepb.AutomatorUpdateNixConfigRequest{
		ConfigurationNix: "",
	})
	if err != nil {
		t.Fatalf("UpdateNixConfig: %v", err)
	}
	if resp.Success {
		t.Error("expected failure for empty config")
	}
}

// --- RebuildNix tests ---

func TestRebuildNix_ValidStrategies(t *testing.T) {
	for _, strategy := range []string{"switch", "boot", "test", "dry-build"} {
		t.Run(strategy, func(t *testing.T) {
			mock := NewMockExecutor()
			mock.Responses["nixos-rebuild"] = struct{ Out, Err string }{Out: "build ok"}

			srv := NewAutomatorServer(mock, AutomatorConfig{})
			resp, err := srv.RebuildNix(context.Background(), &nodepb.AutomatorRebuildNixRequest{
				Strategy: strategy,
			})
			if err != nil {
				t.Fatalf("RebuildNix: %v", err)
			}
			if !resp.Success {
				t.Errorf("expected success for strategy %s", strategy)
			}
		})
	}
}

func TestRebuildNix_InvalidStrategy(t *testing.T) {
	mock := NewMockExecutor()
	srv := NewAutomatorServer(mock, AutomatorConfig{})

	resp, err := srv.RebuildNix(context.Background(), &nodepb.AutomatorRebuildNixRequest{
		Strategy: "invalid",
	})
	if err != nil {
		t.Fatalf("RebuildNix: %v", err)
	}
	if resp.Success {
		t.Error("expected failure for invalid strategy")
	}
}

func TestRebuildNix_DefaultStrategy(t *testing.T) {
	mock := NewMockExecutor()
	mock.Responses["nixos-rebuild"] = struct{ Out, Err string }{Out: "ok"}

	srv := NewAutomatorServer(mock, AutomatorConfig{})
	resp, err := srv.RebuildNix(context.Background(), &nodepb.AutomatorRebuildNixRequest{})
	if err != nil {
		t.Fatalf("RebuildNix: %v", err)
	}
	if !resp.Success {
		t.Error("expected success with default strategy")
	}
}

func TestRebuildNix_CommandFailure(t *testing.T) {
	mock := NewMockExecutor()
	mock.Errors["nixos-rebuild"] = fmt.Errorf("exit 1")
	mock.Responses["nixos-rebuild"] = struct{ Out, Err string }{Err: "some error"}

	srv := NewAutomatorServer(mock, AutomatorConfig{})
	resp, err := srv.RebuildNix(context.Background(), &nodepb.AutomatorRebuildNixRequest{Strategy: "switch"})
	if err != nil {
		t.Fatalf("RebuildNix: %v", err)
	}
	if resp.Success {
		t.Error("expected failure when command fails")
	}
}

// --- UpdateSystem tests ---

func TestUpdateSystem_ChannelsAndRebuild(t *testing.T) {
	mock := NewMockExecutor()
	mock.Responses["nix-channel"] = struct{ Out, Err string }{Out: "ok"}
	mock.Responses["nixos-rebuild"] = struct{ Out, Err string }{Out: "ok"}

	srv := NewAutomatorServer(mock, AutomatorConfig{})
	resp, err := srv.UpdateSystem(context.Background(), &nodepb.AutomatorUpdateSystemRequest{
		UpdateChannels: true,
		Rebuild:        true,
		UpdateAgent:    true,
	})
	if err != nil {
		t.Fatalf("UpdateSystem: %v", err)
	}
	if !resp.Success {
		t.Errorf("expected success, got: %s", resp.Message)
	}
	if resp.Message == "" {
		t.Error("expected non-empty message")
	}
}

func TestUpdateSystem_ChannelFailure(t *testing.T) {
	mock := NewMockExecutor()
	mock.Errors["nix-channel"] = fmt.Errorf("network error")

	srv := NewAutomatorServer(mock, AutomatorConfig{})
	resp, err := srv.UpdateSystem(context.Background(), &nodepb.AutomatorUpdateSystemRequest{
		UpdateChannels: true,
	})
	if err != nil {
		t.Fatalf("UpdateSystem: %v", err)
	}
	if resp.Success {
		t.Error("expected failure when channels update fails")
	}
}

// --- ConfigureNetwork tests ---

func TestConfigureNetwork_GeneratesSnippet(t *testing.T) {
	mock := NewMockExecutor()
	srv := NewAutomatorServer(mock, AutomatorConfig{ConfigNixPath: "/tmp/nix.nix"})

	resp, err := srv.ConfigureNetwork(context.Background(), &nodepb.AutomatorConfigureNetworkRequest{
		Bridges: []*nodepb.AutomatorBridgeConfig{
			{Name: "br0", MemberPorts: []string{"enp0s31f6"}, Dhcp: true},
		},
		DnsServers: "8.8.8.8,1.1.1.1",
		ApplyNow:   false,
	})
	if err != nil {
		t.Fatalf("ConfigureNetwork: %v", err)
	}
	if !resp.Success {
		t.Errorf("expected success, got: %s", resp.Message)
	}
	if resp.GeneratedNixSnippet == "" {
		t.Error("expected non-empty NixOS snippet")
	}
	if !contains(resp.GeneratedNixSnippet, "br0") {
		t.Error("snippet should contain br0")
	}
	if !contains(resp.GeneratedNixSnippet, "8.8.8.8") {
		t.Error("snippet should contain DNS servers")
	}
}

func TestConfigureNetwork_WithApply(t *testing.T) {
	mock := NewMockExecutor()
	cfgPath := filepath.Join(t.TempDir(), "configuration.nix")
	mock.Files[cfgPath] = []byte("{ config, ... }: {\n  networking = {\n    useDHCP = true;\n  };\n}")
	mock.Responses["nixos-rebuild"] = struct{ Out, Err string }{Out: "ok"}

	srv := NewAutomatorServer(mock, AutomatorConfig{ConfigNixPath: cfgPath})

	resp, err := srv.ConfigureNetwork(context.Background(), &nodepb.AutomatorConfigureNetworkRequest{
		Bridges:  []*nodepb.AutomatorBridgeConfig{{Name: "br0", MemberPorts: []string{"eth0"}, Dhcp: true}},
		ApplyNow: true,
	})
	if err != nil {
		t.Fatalf("ConfigureNetwork: %v", err)
	}
	if !resp.Success {
		t.Errorf("expected success, got: %s", resp.Message)
	}
}

// --- Helper function tests ---

func TestPrefixToMask(t *testing.T) {
	tests := []struct {
		prefix int
		want   string
	}{
		{24, "255.255.255.0"},
		{16, "255.255.0.0"},
		{8, "255.0.0.0"},
		{32, "255.255.255.255"},
		{0, ""},
	}
	for _, tt := range tests {
		got := prefixToMask(tt.prefix)
		if got != tt.want {
			t.Errorf("prefixToMask(%d) = %q, want %q", tt.prefix, got, tt.want)
		}
	}
}

func TestMaskToPrefix(t *testing.T) {
	tests := []struct {
		mask string
		want int
	}{
		{"255.255.255.0", 24},
		{"255.255.0.0", 16},
		{"255.0.0.0", 8},
		{"", 24},
	}
	for _, tt := range tests {
		got := maskToPrefix(tt.mask)
		if got != tt.want {
			t.Errorf("maskToPrefix(%q) = %d, want %d", tt.mask, got, tt.want)
		}
	}
}

func TestGenerateNixNetworkSnippet_Bond(t *testing.T) {
	snippet := generateNixNetworkSnippet(&nodepb.AutomatorConfigureNetworkRequest{
		Bonds: []*nodepb.AutomatorBondConfig{
			{Name: "bond0", MemberPorts: []string{"eth0", "eth1"}, Mode: "802.3ad"},
		},
	})
	if !contains(snippet, "bond0") {
		t.Error("snippet should contain bond0")
	}
	if !contains(snippet, "802.3ad") {
		t.Error("snippet should contain bond mode")
	}
}

func TestGenerateNixNetworkSnippet_Vlan(t *testing.T) {
	snippet := generateNixNetworkSnippet(&nodepb.AutomatorConfigureNetworkRequest{
		Vlans: []*nodepb.AutomatorVlanConfig{
			{ParentInterface: "eth0", VlanId: 100, IpAddress: "10.0.100.10", SubnetMask: "255.255.255.0"},
		},
	})
	if !contains(snippet, "100") {
		t.Error("snippet should contain vlan id")
	}
}

func TestInjectNetworkBlock_Existing(t *testing.T) {
	existing := `{ config, ... }: {
  networking = {
    useDHCP = true;
  };
  services.openssh.enable = true;
}`
	snippet := "  networking = {\n    bridges.br0 = {};\n  };\n"
	result := injectNetworkBlock(existing, snippet)

	if contains(result, "useDHCP") {
		t.Error("old networking block should be replaced")
	}
	if !contains(result, "br0") {
		t.Error("new networking block should be present")
	}
	if !contains(result, "openssh") {
		t.Error("other config should be preserved")
	}
}

func TestInjectNetworkBlock_NoExisting(t *testing.T) {
	existing := `{ config, ... }: {
  services.openssh.enable = true;
}`
	snippet := "  networking = {\n    bridges.br0 = {};\n  };\n"
	result := injectNetworkBlock(existing, snippet)

	if !contains(result, "br0") {
		t.Error("networking block should be added")
	}
	if !contains(result, "openssh") {
		t.Error("existing config should be preserved")
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && containsHelper(s, substr))
}

func containsHelper(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
