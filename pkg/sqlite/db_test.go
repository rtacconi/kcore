package sqlite

import (
	"testing"
)

func newTestDB(t *testing.T) *DB {
	t.Helper()
	db, err := New(":memory:")
	if err != nil {
		t.Fatalf("New(:memory:): %v", err)
	}
	t.Cleanup(func() { db.Close() })
	return db
}

func TestNew_InMemory(t *testing.T) {
	db := newTestDB(t)
	if db == nil {
		t.Fatal("db is nil")
	}
}

func TestUpsertAndGetNode(t *testing.T) {
	db := newTestDB(t)

	node := &Node{
		ID:          "node-1",
		Hostname:    "host-1",
		Address:     "10.0.0.1:9091",
		CPUCores:    8,
		MemoryBytes: 16 * 1024 * 1024 * 1024,
		Labels:      []string{},
	}
	if err := db.UpsertNode(node); err != nil {
		t.Fatalf("UpsertNode: %v", err)
	}

	got, err := db.GetNode("node-1")
	if err != nil {
		t.Fatalf("GetNode: %v", err)
	}
	if got.Hostname != "host-1" {
		t.Errorf("Hostname=%q", got.Hostname)
	}
	if got.CPUCores != 8 {
		t.Errorf("CPUCores=%d", got.CPUCores)
	}
}

func TestUpsertNode_Update(t *testing.T) {
	db := newTestDB(t)

	node := &Node{ID: "n1", Hostname: "old", Address: "1.2.3.4:9091", CPUCores: 4, MemoryBytes: 8e9}
	db.UpsertNode(node)

	node.Hostname = "new"
	node.CPUCores = 16
	db.UpsertNode(node)

	got, err := db.GetNode("n1")
	if err != nil {
		t.Fatalf("GetNode: %v", err)
	}
	if got.Hostname != "new" {
		t.Errorf("Hostname=%q after update", got.Hostname)
	}
	if got.CPUCores != 16 {
		t.Errorf("CPUCores=%d after update", got.CPUCores)
	}
}

func TestListNodes(t *testing.T) {
	db := newTestDB(t)

	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "1.1.1.1", CPUCores: 4, MemoryBytes: 8e9})
	db.UpsertNode(&Node{ID: "n2", Hostname: "h2", Address: "2.2.2.2", CPUCores: 8, MemoryBytes: 16e9})

	nodes, err := db.ListNodes()
	if err != nil {
		t.Fatalf("ListNodes: %v", err)
	}
	if len(nodes) != 2 {
		t.Errorf("ListNodes=%d, want 2", len(nodes))
	}
}

func TestGetNode_NotFound(t *testing.T) {
	db := newTestDB(t)

	_, err := db.GetNode("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing node")
	}
}

func TestStorageClass_CRUD(t *testing.T) {
	db := newTestDB(t)

	sc := &StorageClass{
		Name:   "local-dir",
		Driver: "local-dir",
		Shared: false,
	}
	if err := db.CreateStorageClass(sc); err != nil {
		t.Fatalf("CreateStorageClass: %v", err)
	}

	got, err := db.GetStorageClass("local-dir")
	if err != nil {
		t.Fatalf("GetStorageClass: %v", err)
	}
	if got.Driver != "local-dir" {
		t.Errorf("Driver=%q", got.Driver)
	}
	if got.Shared != false {
		t.Error("Shared should be false")
	}
}

func TestVolume_CRUD(t *testing.T) {
	db := newTestDB(t)

	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "1.1.1.1", CPUCores: 4, MemoryBytes: 8e9})
	db.CreateStorageClass(&StorageClass{Name: "local-dir", Driver: "local-dir"})

	nodeID := "n1"
	vol := &Volume{
		ID:           "vol-1",
		Name:         "root-disk",
		Namespace:    "default",
		StorageClass: "local-dir",
		SizeBytes:    40 * 1024 * 1024 * 1024,
		NodeID:       &nodeID,
	}
	if err := db.CreateVolume(vol); err != nil {
		t.Fatalf("CreateVolume: %v", err)
	}

	got, err := db.GetVolume("vol-1")
	if err != nil {
		t.Fatalf("GetVolume: %v", err)
	}
	if got.Name != "root-disk" {
		t.Errorf("Name=%q", got.Name)
	}
	if got.SizeBytes != 40*1024*1024*1024 {
		t.Errorf("SizeBytes=%d", got.SizeBytes)
	}

	if err := db.UpdateVolumeBackendHandle("vol-1", "/dev/vg0/lv-root"); err != nil {
		t.Fatalf("UpdateVolumeBackendHandle: %v", err)
	}

	got, _ = db.GetVolume("vol-1")
	if got.BackendHandle != "/dev/vg0/lv-root" {
		t.Errorf("BackendHandle=%q after update", got.BackendHandle)
	}
}

func TestVM_CRUD(t *testing.T) {
	db := newTestDB(t)

	vm := &VM{
		ID:          "vm-1",
		Name:        "test-vm",
		Namespace:   "default",
		CPU:         4,
		MemoryBytes: 8 * 1024 * 1024 * 1024,
		State:       "pending",
	}
	if err := db.CreateVM(vm); err != nil {
		t.Fatalf("CreateVM: %v", err)
	}

	got, err := db.GetVM("vm-1")
	if err != nil {
		t.Fatalf("GetVM: %v", err)
	}
	if got.Name != "test-vm" {
		t.Errorf("Name=%q", got.Name)
	}
	if got.State != "pending" {
		t.Errorf("State=%q", got.State)
	}

	if err := db.UpdateVMState("vm-1", "running"); err != nil {
		t.Fatalf("UpdateVMState: %v", err)
	}

	got, _ = db.GetVM("vm-1")
	if got.State != "running" {
		t.Errorf("State=%q after update", got.State)
	}
}

func TestVMDisk_CRUD(t *testing.T) {
	db := newTestDB(t)

	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "1.1.1.1", CPUCores: 4, MemoryBytes: 8e9})
	db.CreateStorageClass(&StorageClass{Name: "local-dir", Driver: "local-dir"})
	nodeID := "n1"
	db.CreateVolume(&Volume{ID: "vol-1", Name: "root", Namespace: "default", StorageClass: "local-dir", SizeBytes: 10e9, NodeID: &nodeID})
	db.CreateVM(&VM{ID: "vm-1", Name: "test", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	disk := &VMDisk{VMID: "vm-1", DiskName: "root", VolumeID: "vol-1", Bus: "virtio", Device: "vda"}
	if err := db.AddVMDisk(disk); err != nil {
		t.Fatalf("AddVMDisk: %v", err)
	}

	disks, err := db.GetVMDisks("vm-1")
	if err != nil {
		t.Fatalf("GetVMDisks: %v", err)
	}
	if len(disks) != 1 {
		t.Fatalf("disks=%d, want 1", len(disks))
	}
	if disks[0].DiskName != "root" {
		t.Errorf("DiskName=%q", disks[0].DiskName)
	}
}

func TestVMNIC_CRUD(t *testing.T) {
	db := newTestDB(t)

	db.CreateVM(&VM{ID: "vm-1", Name: "test", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	mac := "52:54:00:aa:bb:cc"
	nic := &VMNIC{VMID: "vm-1", Network: "default", Model: "virtio", MACAddress: &mac}
	if err := db.AddVMNIC(nic); err != nil {
		t.Fatalf("AddVMNIC: %v", err)
	}

	nics, err := db.GetVMNICs("vm-1")
	if err != nil {
		t.Fatalf("GetVMNICs: %v", err)
	}
	if len(nics) != 1 {
		t.Fatalf("nics=%d, want 1", len(nics))
	}
	if nics[0].Network != "default" {
		t.Errorf("Network=%q", nics[0].Network)
	}
	if nics[0].MACAddress == nil || *nics[0].MACAddress != mac {
		t.Errorf("MACAddress=%v", nics[0].MACAddress)
	}
}

func TestVMPlacement(t *testing.T) {
	db := newTestDB(t)

	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "1.1.1.1", CPUCores: 4, MemoryBytes: 8e9})
	db.CreateVM(&VM{ID: "vm-1", Name: "test", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	placement, err := db.GetVMPlacement("vm-1")
	if err != nil {
		t.Fatalf("GetVMPlacement: %v", err)
	}
	if placement.DesiredState != "stopped" {
		t.Errorf("DesiredState=%q, want stopped", placement.DesiredState)
	}

	nodeID := "n1"
	placement.DesiredNodeID = &nodeID
	placement.DesiredState = "running"
	if err := db.UpdateVMPlacement(placement); err != nil {
		t.Fatalf("UpdateVMPlacement: %v", err)
	}

	got, _ := db.GetVMPlacement("vm-1")
	if got.DesiredState != "running" {
		t.Errorf("DesiredState=%q after update", got.DesiredState)
	}
}

func TestListVMsForReconciliation(t *testing.T) {
	db := newTestDB(t)

	db.CreateVM(&VM{ID: "vm-1", Name: "reconcile-me", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	placements, err := db.ListVMsForReconciliation()
	if err != nil {
		t.Fatalf("ListVMsForReconciliation: %v", err)
	}
	if len(placements) != 1 {
		t.Fatalf("placements=%d, want 1 (desired=stopped, actual=unknown)", len(placements))
	}
}
