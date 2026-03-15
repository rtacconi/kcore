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

func TestSchemaVersion(t *testing.T) {
	db := newTestDB(t)
	v := db.SchemaVersion()
	if v != 2 {
		t.Errorf("SchemaVersion()=%d, want 2 (after 001_initial + 002_desired_state)", v)
	}
}

func TestMigrationsIdempotent(t *testing.T) {
	db := newTestDB(t)
	v1 := db.SchemaVersion()

	// Re-running migrate should be a no-op
	if err := db.migrate(); err != nil {
		t.Fatalf("re-running migrate: %v", err)
	}
	v2 := db.SchemaVersion()
	if v1 != v2 {
		t.Errorf("schema version changed after re-migrate: %d -> %d", v1, v2)
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

func TestGetNodeByAddress(t *testing.T) {
	db := newTestDB(t)

	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "10.0.0.1:9091", CPUCores: 4, MemoryBytes: 8e9})
	db.UpsertNode(&Node{ID: "n2", Hostname: "h2", Address: "10.0.0.2:9091", CPUCores: 8, MemoryBytes: 16e9})

	got, err := db.GetNodeByAddress("10.0.0.2:9091")
	if err != nil {
		t.Fatalf("GetNodeByAddress: %v", err)
	}
	if got.ID != "n2" {
		t.Errorf("ID=%q, want n2", got.ID)
	}

	_, err = db.GetNodeByAddress("10.0.0.99:9091")
	if err == nil {
		t.Error("expected error for unknown address")
	}
}

func TestUpdateNodeHeartbeat(t *testing.T) {
	db := newTestDB(t)
	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "1.1.1.1", CPUCores: 4, MemoryBytes: 8e9})

	if err := db.UpdateNodeHeartbeat("n1"); err != nil {
		t.Fatalf("UpdateNodeHeartbeat: %v", err)
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

func TestVM_DesiredState(t *testing.T) {
	db := newTestDB(t)

	vm := &VM{
		ID:           "vm-ds",
		Name:         "desired-state-vm",
		Namespace:    "default",
		CPU:          2,
		MemoryBytes:  4e9,
		State:        "pending",
		DesiredSpec:  `{"cpu":2,"memory_bytes":4000000000}`,
		DesiredState: "running",
		ImageURI:     "https://example.com/image.qcow2",
	}
	if err := db.CreateVM(vm); err != nil {
		t.Fatalf("CreateVM: %v", err)
	}

	got, err := db.GetVM("vm-ds")
	if err != nil {
		t.Fatalf("GetVM: %v", err)
	}
	if got.DesiredState != "running" {
		t.Errorf("DesiredState=%q, want running", got.DesiredState)
	}
	if got.DesiredSpec != `{"cpu":2,"memory_bytes":4000000000}` {
		t.Errorf("DesiredSpec=%q", got.DesiredSpec)
	}
	if got.ImageURI != "https://example.com/image.qcow2" {
		t.Errorf("ImageURI=%q", got.ImageURI)
	}

	if err := db.UpdateVMDesiredState("vm-ds", "stopped"); err != nil {
		t.Fatalf("UpdateVMDesiredState: %v", err)
	}
	got, _ = db.GetVM("vm-ds")
	if got.DesiredState != "stopped" {
		t.Errorf("DesiredState=%q after update", got.DesiredState)
	}
}

func TestVM_GetByName(t *testing.T) {
	db := newTestDB(t)

	db.CreateVM(&VM{ID: "vm-1", Name: "web-server", Namespace: "default", CPU: 2, MemoryBytes: 4e9, State: "running"})

	got, err := db.GetVMByName("web-server")
	if err != nil {
		t.Fatalf("GetVMByName: %v", err)
	}
	if got.ID != "vm-1" {
		t.Errorf("ID=%q, want vm-1", got.ID)
	}
}

func TestVM_ListVMs(t *testing.T) {
	db := newTestDB(t)

	db.CreateVM(&VM{ID: "vm-1", Name: "a", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "running"})
	db.CreateVM(&VM{ID: "vm-2", Name: "b", Namespace: "default", CPU: 2, MemoryBytes: 2e9, State: "stopped"})

	vms, err := db.ListVMs()
	if err != nil {
		t.Fatalf("ListVMs: %v", err)
	}
	if len(vms) != 2 {
		t.Errorf("ListVMs=%d, want 2", len(vms))
	}
}

func TestVM_ListVMs_ExcludesDeleted(t *testing.T) {
	db := newTestDB(t)

	db.CreateVM(&VM{ID: "vm-1", Name: "active", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "running"})
	db.CreateVM(&VM{ID: "vm-2", Name: "deleted-vm", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "stopped", DesiredState: "deleted"})

	vms, err := db.ListVMs()
	if err != nil {
		t.Fatalf("ListVMs: %v", err)
	}
	if len(vms) != 1 {
		t.Errorf("ListVMs=%d, want 1 (deleted VMs excluded)", len(vms))
	}
	if vms[0].Name != "active" {
		t.Errorf("expected 'active' vm, got %q", vms[0].Name)
	}
}

func TestVM_DeleteVM(t *testing.T) {
	db := newTestDB(t)

	db.CreateVM(&VM{ID: "vm-1", Name: "to-delete", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "running"})

	if err := db.DeleteVM("vm-1"); err != nil {
		t.Fatalf("DeleteVM: %v", err)
	}

	_, err := db.GetVM("vm-1")
	if err == nil {
		t.Error("expected error after deletion")
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

func TestVM_UpdateNodeID(t *testing.T) {
	db := newTestDB(t)

	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "1.1.1.1", CPUCores: 4, MemoryBytes: 8e9})
	db.CreateVM(&VM{ID: "vm-1", Name: "test", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	if err := db.UpdateVMNodeID("vm-1", "n1"); err != nil {
		t.Fatalf("UpdateVMNodeID: %v", err)
	}

	got, _ := db.GetVM("vm-1")
	if got.NodeID == nil || *got.NodeID != "n1" {
		t.Errorf("NodeID=%v after update", got.NodeID)
	}
}

func TestGetVM_NotFound(t *testing.T) {
	db := newTestDB(t)
	_, err := db.GetVM("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing VM")
	}
}

func TestGetVMByName_NotFound(t *testing.T) {
	db := newTestDB(t)
	_, err := db.GetVMByName("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing VM name")
	}
}

func TestGetVolume_NotFound(t *testing.T) {
	db := newTestDB(t)
	_, err := db.GetVolume("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing volume")
	}
}

func TestGetStorageClass_NotFound(t *testing.T) {
	db := newTestDB(t)
	_, err := db.GetStorageClass("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing storage class")
	}
}

func TestGetVMPlacement_NotFound(t *testing.T) {
	db := newTestDB(t)
	_, err := db.GetVMPlacement("nonexistent")
	if err == nil {
		t.Fatal("expected error for missing placement")
	}
}

func TestGetVMDisks_Empty(t *testing.T) {
	db := newTestDB(t)
	db.CreateVM(&VM{ID: "vm-1", Name: "test", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	disks, err := db.GetVMDisks("vm-1")
	if err != nil {
		t.Fatalf("GetVMDisks: %v", err)
	}
	if len(disks) != 0 {
		t.Errorf("expected 0 disks, got %d", len(disks))
	}
}

func TestGetVMNICs_Empty(t *testing.T) {
	db := newTestDB(t)
	db.CreateVM(&VM{ID: "vm-1", Name: "test", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	nics, err := db.GetVMNICs("vm-1")
	if err != nil {
		t.Fatalf("GetVMNICs: %v", err)
	}
	if len(nics) != 0 {
		t.Errorf("expected 0 nics, got %d", len(nics))
	}
}

func TestVMNIC_NullMACAddress(t *testing.T) {
	db := newTestDB(t)
	db.CreateVM(&VM{ID: "vm-1", Name: "test", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	nic := &VMNIC{VMID: "vm-1", Network: "default", Model: "virtio"}
	if err := db.AddVMNIC(nic); err != nil {
		t.Fatalf("AddVMNIC: %v", err)
	}

	nics, err := db.GetVMNICs("vm-1")
	if err != nil {
		t.Fatalf("GetVMNICs: %v", err)
	}
	if len(nics) != 1 {
		t.Fatalf("nics=%d", len(nics))
	}
	if nics[0].MACAddress != nil {
		t.Errorf("MACAddress should be nil, got %v", nics[0].MACAddress)
	}
}

func TestListVMs_Empty(t *testing.T) {
	db := newTestDB(t)
	vms, err := db.ListVMs()
	if err != nil {
		t.Fatalf("ListVMs: %v", err)
	}
	if len(vms) != 0 {
		t.Errorf("expected 0 vms, got %d", len(vms))
	}
}

func TestListNodes_Empty(t *testing.T) {
	db := newTestDB(t)
	nodes, err := db.ListNodes()
	if err != nil {
		t.Fatalf("ListNodes: %v", err)
	}
	if len(nodes) != 0 {
		t.Errorf("expected 0 nodes, got %d", len(nodes))
	}
}

func TestListVMsForReconciliation_Empty(t *testing.T) {
	db := newTestDB(t)
	placements, err := db.ListVMsForReconciliation()
	if err != nil {
		t.Fatalf("ListVMsForReconciliation: %v", err)
	}
	if len(placements) != 0 {
		t.Errorf("expected 0 placements, got %d", len(placements))
	}
}

func TestVM_DesiredState_DefaultRunning(t *testing.T) {
	db := newTestDB(t)
	vm := &VM{ID: "vm-def", Name: "default-ds", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"}
	if err := db.CreateVM(vm); err != nil {
		t.Fatalf("CreateVM: %v", err)
	}

	got, _ := db.GetVM("vm-def")
	if got.DesiredState != "running" {
		t.Errorf("DesiredState=%q, want running (default)", got.DesiredState)
	}
}

func TestVM_WithNodeID(t *testing.T) {
	db := newTestDB(t)
	db.UpsertNode(&Node{ID: "n1", Hostname: "h1", Address: "1.1.1.1", CPUCores: 4, MemoryBytes: 8e9})

	nodeID := "n1"
	vm := &VM{ID: "vm-n", Name: "node-vm", Namespace: "default", CPU: 2, MemoryBytes: 4e9, State: "running", NodeID: &nodeID}
	if err := db.CreateVM(vm); err != nil {
		t.Fatalf("CreateVM: %v", err)
	}

	got, _ := db.GetVM("vm-n")
	if got.NodeID == nil || *got.NodeID != "n1" {
		t.Errorf("NodeID=%v", got.NodeID)
	}
}

func TestUpdateVMState(t *testing.T) {
	db := newTestDB(t)
	db.CreateVM(&VM{ID: "vm-st", Name: "state-vm", Namespace: "default", CPU: 1, MemoryBytes: 1e9, State: "pending"})

	db.UpdateVMState("vm-st", "running")
	got, _ := db.GetVM("vm-st")
	if got.State != "running" {
		t.Errorf("State=%q", got.State)
	}

	db.UpdateVMState("vm-st", "stopped")
	got, _ = db.GetVM("vm-st")
	if got.State != "stopped" {
		t.Errorf("State=%q", got.State)
	}
}

func TestNodeLabels(t *testing.T) {
	db := newTestDB(t)
	node := &Node{
		ID:          "n-labels",
		Hostname:    "labeled",
		Address:     "1.1.1.1",
		CPUCores:    4,
		MemoryBytes: 8e9,
		Labels:      []string{"env=prod", "zone=us-east"},
	}
	if err := db.UpsertNode(node); err != nil {
		t.Fatalf("UpsertNode: %v", err)
	}

	got, _ := db.GetNode("n-labels")
	if got.Hostname != "labeled" {
		t.Errorf("Hostname=%q", got.Hostname)
	}
}

func TestVolume_NullNodeID(t *testing.T) {
	db := newTestDB(t)
	db.CreateStorageClass(&StorageClass{Name: "shared", Driver: "nfs", Shared: true})

	vol := &Volume{
		ID: "vol-shared", Name: "shared-vol", Namespace: "default",
		StorageClass: "shared", SizeBytes: 1e9, Shared: true,
	}
	if err := db.CreateVolume(vol); err != nil {
		t.Fatalf("CreateVolume: %v", err)
	}

	got, _ := db.GetVolume("vol-shared")
	if got.NodeID != nil {
		t.Errorf("NodeID should be nil for shared volume, got %v", got.NodeID)
	}
	if !got.Shared {
		t.Error("Shared should be true")
	}
}
