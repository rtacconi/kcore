package libvirt

import (
	"testing"

	libvirtgo "github.com/libvirt/libvirt-go"
)

func TestDomainToVMState(t *testing.T) {
	t.Parallel()

	tests := []struct {
		state libvirtgo.DomainState
		want  string
	}{
		{libvirtgo.DOMAIN_RUNNING, "running"},
		{libvirtgo.DOMAIN_BLOCKED, "running"},
		{libvirtgo.DOMAIN_PAUSED, "paused"},
		{libvirtgo.DOMAIN_SHUTDOWN, "stopped"},
		{libvirtgo.DOMAIN_SHUTOFF, "stopped"},
		{libvirtgo.DOMAIN_CRASHED, "stopped"},
		{libvirtgo.DOMAIN_PMSUSPENDED, "unknown"},
		{libvirtgo.DomainState(99), "unknown"},
	}

	for _, tt := range tests {
		got := DomainToVMState(tt.state)
		if got != tt.want {
			t.Errorf("DomainToVMState(%d)=%q, want %q", tt.state, got, tt.want)
		}
	}
}

func TestBuildDomainXML_Basic(t *testing.T) {
	t.Parallel()

	spec := &VMSpec{
		ID:          "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
		Name:        "test-vm",
		CPU:         2,
		MemoryBytes: 2 * 1024 * 1024 * 1024,
		Disks: []DiskSpec{
			{Name: "root", BackendHandle: "/var/lib/kcore/disks/root.qcow2", Bus: "virtio", Device: "vda"},
		},
		NICs: []NICSpec{
			{Network: "default", Model: "virtio"},
		},
	}

	domain, err := BuildDomainXML(spec)
	if err != nil {
		t.Fatalf("BuildDomainXML: %v", err)
	}

	if domain.Type != "kvm" {
		t.Errorf("Type=%q, want kvm", domain.Type)
	}
	if domain.Name != "test-vm" {
		t.Errorf("Name=%q", domain.Name)
	}
	if domain.UUID != "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" {
		t.Errorf("UUID=%q", domain.UUID)
	}
	if domain.VCPU.Value != 2 {
		t.Errorf("VCPU=%d, want 2", domain.VCPU.Value)
	}

	expectedMemKiB := uint(2 * 1024 * 1024) // 2 GiB in KiB
	if domain.Memory.Value != expectedMemKiB {
		t.Errorf("Memory=%d KiB, want %d KiB", domain.Memory.Value, expectedMemKiB)
	}

	if len(domain.Devices.Disks) != 1 {
		t.Fatalf("disks=%d, want 1", len(domain.Devices.Disks))
	}
	disk := domain.Devices.Disks[0]
	if disk.Device != "disk" {
		t.Errorf("disk.Device=%q, want disk", disk.Device)
	}
	if disk.Target.Dev != "vda" {
		t.Errorf("disk.Target.Dev=%q, want vda", disk.Target.Dev)
	}

	if len(domain.Devices.Interfaces) != 1 {
		t.Fatalf("interfaces=%d, want 1", len(domain.Devices.Interfaces))
	}
	iface := domain.Devices.Interfaces[0]
	if iface.Source.Network == nil {
		t.Fatal("expected network source for 'default' network")
	}
	if iface.Source.Network.Network != "default" {
		t.Errorf("network=%q, want default", iface.Source.Network.Network)
	}
}

func TestBuildDomainXML_CloudInitDisk(t *testing.T) {
	t.Parallel()

	spec := &VMSpec{
		ID:          "11111111-2222-3333-4444-555555555555",
		Name:        "ci-test",
		CPU:         1,
		MemoryBytes: 1024 * 1024 * 1024,
		Disks: []DiskSpec{
			{Name: "root", BackendHandle: "/disks/root.qcow2", Bus: "virtio", Device: "vda"},
			{Name: "cloud-init", BackendHandle: "/disks/seed.iso", Bus: "virtio", Device: "vdb"},
		},
	}

	domain, err := BuildDomainXML(spec)
	if err != nil {
		t.Fatalf("BuildDomainXML: %v", err)
	}

	if len(domain.Devices.Disks) != 2 {
		t.Fatalf("disks=%d, want 2", len(domain.Devices.Disks))
	}

	ciDisk := domain.Devices.Disks[1]
	if ciDisk.Device != "cdrom" {
		t.Errorf("cloud-init disk device=%q, want cdrom", ciDisk.Device)
	}
	if ciDisk.Target.Bus != "sata" {
		t.Errorf("cloud-init bus=%q, want sata", ciDisk.Target.Bus)
	}
	if ciDisk.Driver.Type != "raw" {
		t.Errorf("cloud-init driver type=%q, want raw", ciDisk.Driver.Type)
	}
	if ciDisk.ReadOnly == nil {
		t.Error("cloud-init disk should be read-only")
	}
}

func TestBuildDomainXML_BlockDevice(t *testing.T) {
	t.Parallel()

	spec := &VMSpec{
		ID:          "22222222-3333-4444-5555-666666666666",
		Name:        "block-test",
		CPU:         1,
		MemoryBytes: 512 * 1024 * 1024,
		Disks: []DiskSpec{
			{Name: "root", BackendHandle: "/dev/vg0/lv-root", Bus: "virtio", Device: "vda"},
		},
	}

	domain, err := BuildDomainXML(spec)
	if err != nil {
		t.Fatalf("BuildDomainXML: %v", err)
	}

	disk := domain.Devices.Disks[0]
	if disk.Source.Block == nil {
		t.Fatal("expected block source for /dev/ path")
	}
	if disk.Source.Block.Dev != "/dev/vg0/lv-root" {
		t.Errorf("block dev=%q", disk.Source.Block.Dev)
	}
}

func TestBuildDomainXML_BridgeNetwork(t *testing.T) {
	t.Parallel()

	spec := &VMSpec{
		ID:          "33333333-4444-5555-6666-777777777777",
		Name:        "bridge-test",
		CPU:         1,
		MemoryBytes: 512 * 1024 * 1024,
		NICs: []NICSpec{
			{Network: "br0", Model: "virtio"},
		},
	}

	domain, err := BuildDomainXML(spec)
	if err != nil {
		t.Fatalf("BuildDomainXML: %v", err)
	}

	iface := domain.Devices.Interfaces[0]
	if iface.Source.Bridge == nil {
		t.Fatal("expected bridge source for 'br0' network")
	}
	if iface.Source.Bridge.Bridge != "br0" {
		t.Errorf("bridge=%q, want br0", iface.Source.Bridge.Bridge)
	}
}

func TestBuildDomainXML_MACAddress(t *testing.T) {
	t.Parallel()

	spec := &VMSpec{
		ID:          "44444444-5555-6666-7777-888888888888",
		Name:        "mac-test",
		CPU:         1,
		MemoryBytes: 512 * 1024 * 1024,
		NICs: []NICSpec{
			{Network: "default", Model: "virtio", MACAddress: "52:54:00:aa:bb:cc"},
		},
	}

	domain, err := BuildDomainXML(spec)
	if err != nil {
		t.Fatalf("BuildDomainXML: %v", err)
	}

	iface := domain.Devices.Interfaces[0]
	if iface.MAC == nil {
		t.Fatal("expected MAC address to be set")
	}
	if iface.MAC.Address != "52:54:00:aa:bb:cc" {
		t.Errorf("MAC=%q", iface.MAC.Address)
	}
}
