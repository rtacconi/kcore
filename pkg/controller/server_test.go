package controller

import (
	"context"
	"testing"

	ctrlpb "github.com/kcore/kcore/api/controller"
	nodepb "github.com/kcore/kcore/api/node"
	"github.com/kcore/kcore/pkg/sqlite"
	"google.golang.org/grpc"
)

// mockNodeClient implements nodepb.NodeComputeClient for testing.
type mockNodeClient struct {
	createResp  *nodepb.CreateVmResponse
	createErr   error
	deleteResp  *nodepb.DeleteVmResponse
	deleteErr   error
	startResp   *nodepb.StartVmResponse
	startErr    error
	stopResp    *nodepb.StopVmResponse
	stopErr     error
	getResp     *nodepb.GetVmResponse
	getErr      error
	listResp    *nodepb.ListVmsResponse
	listErr     error

	lastCreateReq *nodepb.CreateVmRequest
	lastDeleteReq *nodepb.DeleteVmRequest
	lastStartReq  *nodepb.StartVmRequest
	lastStopReq   *nodepb.StopVmRequest
}

func (m *mockNodeClient) CreateVm(_ context.Context, in *nodepb.CreateVmRequest, _ ...grpc.CallOption) (*nodepb.CreateVmResponse, error) {
	m.lastCreateReq = in
	return m.createResp, m.createErr
}
func (m *mockNodeClient) UpdateVm(_ context.Context, _ *nodepb.UpdateVmRequest, _ ...grpc.CallOption) (*nodepb.UpdateVmResponse, error) {
	return nil, nil
}
func (m *mockNodeClient) DeleteVm(_ context.Context, in *nodepb.DeleteVmRequest, _ ...grpc.CallOption) (*nodepb.DeleteVmResponse, error) {
	m.lastDeleteReq = in
	return m.deleteResp, m.deleteErr
}
func (m *mockNodeClient) StartVm(_ context.Context, in *nodepb.StartVmRequest, _ ...grpc.CallOption) (*nodepb.StartVmResponse, error) {
	m.lastStartReq = in
	return m.startResp, m.startErr
}
func (m *mockNodeClient) StopVm(_ context.Context, in *nodepb.StopVmRequest, _ ...grpc.CallOption) (*nodepb.StopVmResponse, error) {
	m.lastStopReq = in
	return m.stopResp, m.stopErr
}
func (m *mockNodeClient) RebootVm(_ context.Context, _ *nodepb.RebootVmRequest, _ ...grpc.CallOption) (*nodepb.RebootVmResponse, error) {
	return nil, nil
}
func (m *mockNodeClient) GetVm(_ context.Context, _ *nodepb.GetVmRequest, _ ...grpc.CallOption) (*nodepb.GetVmResponse, error) {
	return m.getResp, m.getErr
}
func (m *mockNodeClient) ListVms(_ context.Context, _ *nodepb.ListVmsRequest, _ ...grpc.CallOption) (*nodepb.ListVmsResponse, error) {
	return m.listResp, m.listErr
}
func (m *mockNodeClient) PullImage(_ context.Context, _ *nodepb.PullImageRequest, _ ...grpc.CallOption) (*nodepb.PullImageResponse, error) {
	return nil, nil
}
func (m *mockNodeClient) ListImages(_ context.Context, _ *nodepb.ListImagesRequest, _ ...grpc.CallOption) (*nodepb.ListImagesResponse, error) {
	return nil, nil
}
func (m *mockNodeClient) DeleteImage(_ context.Context, _ *nodepb.DeleteImageRequest, _ ...grpc.CallOption) (*nodepb.DeleteImageResponse, error) {
	return nil, nil
}

func newTestServerWithDB(t *testing.T) (*Server, *sqlite.DB) {
	t.Helper()
	db, err := sqlite.New(":memory:")
	if err != nil {
		t.Fatalf("sqlite.New: %v", err)
	}
	t.Cleanup(func() { db.Close() })
	s := NewServerWithDB(db)
	return s, db
}

func addMockNode(s *Server, id, addr string, mock *mockNodeClient) {
	s.nodes[id] = &NodeInfo{
		ID:       id,
		Hostname: id,
		Address:  addr,
		Status:   "ready",
		Client:   mock,
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16e9},
		Usage:    &ctrlpb.NodeUsage{},
	}
}

func TestNewServer(t *testing.T) {
	s := NewServer()
	if s == nil {
		t.Fatal("NewServer returned nil")
	}
	if s.nodes == nil {
		t.Error("nodes map is nil")
	}
	if s.vmToNode == nil {
		t.Error("vmToNode map is nil")
	}
}

func TestRegisterNode(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	resp, err := s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId:   "node-1",
		Hostname: "host-1",
		Address:  "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16 * 1024 * 1024 * 1024},
	})
	if err != nil {
		t.Fatalf("RegisterNode: %v", err)
	}
	if !resp.Success {
		t.Errorf("Success=%v, want true", resp.Success)
	}

	s.mu.RLock()
	node, ok := s.nodes["node-1"]
	s.mu.RUnlock()

	if !ok {
		t.Fatal("node not found after registration")
	}
	if node.Hostname != "host-1" {
		t.Errorf("Hostname=%q", node.Hostname)
	}
	if node.Address != "10.0.0.1:9091" {
		t.Errorf("Address=%q", node.Address)
	}
}

func TestHeartbeat_ExistingNode(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId:   "node-1",
		Hostname: "host-1",
	})

	resp, err := s.Heartbeat(ctx, &ctrlpb.HeartbeatRequest{
		NodeId: "node-1",
		Usage:  &ctrlpb.NodeUsage{CpuCoresUsed: 4},
	})
	if err != nil {
		t.Fatalf("Heartbeat: %v", err)
	}
	if !resp.Success {
		t.Error("Heartbeat should succeed for existing node")
	}
}

func TestHeartbeat_UnknownNode(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	_, err := s.Heartbeat(ctx, &ctrlpb.HeartbeatRequest{
		NodeId: "nonexistent",
	})
	if err == nil {
		t.Fatal("expected error for unknown node")
	}
}

func TestSelectNode_NoNodes(t *testing.T) {
	s := NewServer()
	node := s.selectNode()
	if node != nil {
		t.Error("selectNode should return nil when no nodes")
	}
}

func TestSelectNode_ReturnsReady(t *testing.T) {
	s := NewServer()
	s.nodes["n1"] = &NodeInfo{ID: "n1", Status: "ready"}
	s.nodes["n2"] = &NodeInfo{ID: "n2", Status: "degraded"}

	node := s.selectNode()
	if node == nil {
		t.Fatal("selectNode returned nil")
	}
	if node.ID != "n1" {
		t.Errorf("selected node=%q, want n1", node.ID)
	}
}

func TestGetNodeByAddress(t *testing.T) {
	s := NewServer()
	s.nodes["n1"] = &NodeInfo{ID: "n1", Address: "10.0.0.1:9091"}
	s.nodes["n2"] = &NodeInfo{ID: "n2", Address: "10.0.0.2:9091"}

	node, err := s.getNodeByAddress("10.0.0.2:9091")
	if err != nil {
		t.Fatalf("getNodeByAddress: %v", err)
	}
	if node.ID != "n2" {
		t.Errorf("node.ID=%q, want n2", node.ID)
	}

	_, err = s.getNodeByAddress("10.0.0.99:9091")
	if err == nil {
		t.Error("expected error for unknown address")
	}
}

func TestConvertDisks(t *testing.T) {
	input := []*ctrlpb.Disk{
		{Name: "root", BackendHandle: "/disk/root.qcow2", Bus: "virtio", Device: "vda"},
		{Name: "data", BackendHandle: "/disk/data.qcow2", Bus: "scsi", Device: "sda"},
	}

	result := convertDisks(input)
	if len(result) != 2 {
		t.Fatalf("len=%d, want 2", len(result))
	}
	if result[0].Name != "root" || result[0].Bus != "virtio" {
		t.Errorf("disk[0]=%+v", result[0])
	}
	if result[1].Device != "sda" {
		t.Errorf("disk[1].Device=%q", result[1].Device)
	}
}

func TestConvertDisks_Nil(t *testing.T) {
	result := convertDisks(nil)
	if len(result) != 0 {
		t.Errorf("convertDisks(nil)=%d items, want 0", len(result))
	}
}

func TestConvertNics(t *testing.T) {
	input := []*ctrlpb.Nic{
		{Network: "default", Model: "virtio", MacAddress: "52:54:00:aa:bb:cc"},
	}

	result := convertNics(input)
	if len(result) != 1 {
		t.Fatalf("len=%d, want 1", len(result))
	}
	if result[0].Network != "default" {
		t.Errorf("Network=%q", result[0].Network)
	}
	if result[0].MacAddress != "52:54:00:aa:bb:cc" {
		t.Errorf("MacAddress=%q", result[0].MacAddress)
	}
}

func TestConvertVmState(t *testing.T) {
	tests := []struct {
		input nodepb.VmState
		want  ctrlpb.VmState
	}{
		{nodepb.VmState_VM_STATE_STOPPED, ctrlpb.VmState_VM_STATE_STOPPED},
		{nodepb.VmState_VM_STATE_RUNNING, ctrlpb.VmState_VM_STATE_RUNNING},
		{nodepb.VmState_VM_STATE_PAUSED, ctrlpb.VmState_VM_STATE_PAUSED},
		{nodepb.VmState_VM_STATE_ERROR, ctrlpb.VmState_VM_STATE_ERROR},
		{nodepb.VmState(99), ctrlpb.VmState_VM_STATE_UNKNOWN},
	}

	for _, tt := range tests {
		got := convertVmState(tt.input)
		if got != tt.want {
			t.Errorf("convertVmState(%v)=%v, want %v", tt.input, got, tt.want)
		}
	}
}

func TestConvertVmStatus_Nil(t *testing.T) {
	result := convertVmStatus(nil)
	if result != nil {
		t.Errorf("convertVmStatus(nil)=%v, want nil", result)
	}
}

func TestConvertVmStatus(t *testing.T) {
	input := &nodepb.VmStatus{
		Id:    "vm-1",
		State: nodepb.VmState_VM_STATE_RUNNING,
	}
	result := convertVmStatus(input)
	if result.Id != "vm-1" {
		t.Errorf("Id=%q", result.Id)
	}
	if result.State != ctrlpb.VmState_VM_STATE_RUNNING {
		t.Errorf("State=%v", result.State)
	}
}

func TestListNodes_Empty(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	resp, err := s.ListNodes(ctx, &ctrlpb.ListNodesRequest{})
	if err != nil {
		t.Fatalf("ListNodes: %v", err)
	}
	if len(resp.Nodes) != 0 {
		t.Errorf("nodes=%d, want 0", len(resp.Nodes))
	}
}

func TestGetNode_NotFound(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	_, err := s.GetNode(ctx, &ctrlpb.GetNodeRequest{NodeId: "missing"})
	if err == nil {
		t.Fatal("expected error for missing node")
	}
}

// --- Tests with DB persistence ---

func TestNewServerWithDB(t *testing.T) {
	s, _ := newTestServerWithDB(t)
	if s.db == nil {
		t.Fatal("db should be set")
	}
}

func TestRegisterNode_WithDB(t *testing.T) {
	s, db := newTestServerWithDB(t)
	ctx := context.Background()

	resp, err := s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId:   "node-db-1",
		Hostname: "host-db-1",
		Address:  "10.0.0.10:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 16, MemoryBytes: 64e9},
	})
	if err != nil {
		t.Fatalf("RegisterNode: %v", err)
	}
	if !resp.Success {
		t.Error("expected success")
	}

	node, err := db.GetNode("node-db-1")
	if err != nil {
		t.Fatalf("DB GetNode: %v", err)
	}
	if node.Hostname != "host-db-1" {
		t.Errorf("DB Hostname=%q", node.Hostname)
	}
	if node.CPUCores != 16 {
		t.Errorf("DB CPUCores=%d", node.CPUCores)
	}
}

func TestHeartbeat_WithDB(t *testing.T) {
	s, _ := newTestServerWithDB(t)
	ctx := context.Background()

	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId:   "node-hb",
		Hostname: "host-hb",
		Address:  "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 4, MemoryBytes: 8e9},
	})

	resp, err := s.Heartbeat(ctx, &ctrlpb.HeartbeatRequest{
		NodeId: "node-hb",
		Usage:  &ctrlpb.NodeUsage{CpuCoresUsed: 2, MemoryBytesUsed: 4e9},
	})
	if err != nil {
		t.Fatalf("Heartbeat: %v", err)
	}
	if !resp.Success {
		t.Error("expected success")
	}
}

func TestCreateVm_ForwardsFields(t *testing.T) {
	s, _ := newTestServerWithDB(t)
	ctx := context.Background()

	mock := &mockNodeClient{
		createResp: &nodepb.CreateVmResponse{
			Status: &nodepb.VmStatus{
				Id:    "vm-test-1",
				State: nodepb.VmState_VM_STATE_STOPPED,
			},
		},
	}
	addMockNode(s, "n1", "10.0.0.1:9091", mock)

	resp, err := s.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec: &ctrlpb.VmSpec{
			Id:               "vm-test-1",
			Name:             "test-vm",
			Cpu:              2,
			MemoryBytes:      4e9,
			EnableKcoreLogin: true,
			Nics: []*ctrlpb.Nic{
				{Network: "default", Model: "virtio"},
			},
		},
		ImageUri:          "https://example.com/image.qcow2",
		CloudInitUserData: "#cloud-config\nusers: []",
	})
	if err != nil {
		t.Fatalf("CreateVm: %v", err)
	}
	if resp.VmId != "vm-test-1" {
		t.Errorf("VmId=%q", resp.VmId)
	}
	if resp.NodeId != "n1" {
		t.Errorf("NodeId=%q", resp.NodeId)
	}

	// Verify forwarded to node
	req := mock.lastCreateReq
	if req == nil {
		t.Fatal("node CreateVm was not called")
	}
	if req.ImageUri != "https://example.com/image.qcow2" {
		t.Errorf("forwarded ImageUri=%q", req.ImageUri)
	}
	if req.CloudInitUserData != "#cloud-config\nusers: []" {
		t.Errorf("forwarded CloudInitUserData=%q", req.CloudInitUserData)
	}
	if !req.Spec.EnableKcoreLogin {
		t.Error("forwarded EnableKcoreLogin should be true")
	}
	if req.Spec.Cpu != 2 {
		t.Errorf("forwarded Cpu=%d", req.Spec.Cpu)
	}
	if len(req.Spec.Nics) != 1 {
		t.Fatalf("forwarded Nics=%d", len(req.Spec.Nics))
	}
}

func TestCreateVm_PersistsToDB(t *testing.T) {
	s, db := newTestServerWithDB(t)
	ctx := context.Background()

	mock := &mockNodeClient{
		createResp: &nodepb.CreateVmResponse{
			Status: &nodepb.VmStatus{
				Id:    "vm-persist-1",
				State: nodepb.VmState_VM_STATE_STOPPED,
			},
		},
	}

	// Register node in DB first, then set mock client
	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId: "n1", Hostname: "h1", Address: "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16e9},
	})
	s.nodes["n1"].Client = mock

	_, err := s.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec: &ctrlpb.VmSpec{
			Id:          "vm-persist-1",
			Name:        "persist-vm",
			Cpu:         4,
			MemoryBytes: 8e9,
		},
		ImageUri: "https://example.com/debian.qcow2",
	})
	if err != nil {
		t.Fatalf("CreateVm: %v", err)
	}

	vm, err := db.GetVM("vm-persist-1")
	if err != nil {
		t.Fatalf("DB GetVM: %v", err)
	}
	if vm.Name != "persist-vm" {
		t.Errorf("DB Name=%q", vm.Name)
	}
	if vm.DesiredState != "running" {
		t.Errorf("DB DesiredState=%q", vm.DesiredState)
	}
	if vm.ImageURI != "https://example.com/debian.qcow2" {
		t.Errorf("DB ImageURI=%q", vm.ImageURI)
	}
}

func TestCreateVm_NoNodes(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	_, err := s.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec: &ctrlpb.VmSpec{
			Id:   "vm-1",
			Name: "test",
			Cpu:  1,
		},
	})
	if err == nil {
		t.Fatal("expected error when no nodes available")
	}
}

func TestCreateVm_TargetNodeNotFound(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	_, err := s.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec:       &ctrlpb.VmSpec{Id: "vm-1", Name: "test", Cpu: 1},
		TargetNode: "nonexistent:9091",
	})
	if err == nil {
		t.Fatal("expected error for missing target node")
	}
}

func TestDeleteVm_WithDB(t *testing.T) {
	s, db := newTestServerWithDB(t)
	ctx := context.Background()

	mock := &mockNodeClient{
		createResp: &nodepb.CreateVmResponse{
			Status: &nodepb.VmStatus{Id: "vm-del-1", State: nodepb.VmState_VM_STATE_STOPPED},
		},
		deleteResp: &nodepb.DeleteVmResponse{},
	}
	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId: "n1", Hostname: "h1", Address: "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16e9},
	})
	s.nodes["n1"].Client = mock

	s.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec: &ctrlpb.VmSpec{Id: "vm-del-1", Name: "del-vm", Cpu: 1, MemoryBytes: 1e9},
	})

	resp, err := s.DeleteVm(ctx, &ctrlpb.DeleteVmRequest{VmId: "vm-del-1"})
	if err != nil {
		t.Fatalf("DeleteVm: %v", err)
	}
	if !resp.Success {
		t.Error("expected success")
	}

	vm, err := db.GetVM("vm-del-1")
	if err != nil {
		t.Fatalf("DB GetVM: %v", err)
	}
	if vm.DesiredState != "deleted" {
		t.Errorf("DB DesiredState=%q, want deleted", vm.DesiredState)
	}
}

func TestDeleteVm_UnknownVM(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	_, err := s.DeleteVm(ctx, &ctrlpb.DeleteVmRequest{VmId: "nonexistent"})
	if err == nil {
		t.Fatal("expected error for unknown VM")
	}
}

func TestStartVm(t *testing.T) {
	s, db := newTestServerWithDB(t)
	ctx := context.Background()

	mock := &mockNodeClient{
		createResp: &nodepb.CreateVmResponse{
			Status: &nodepb.VmStatus{Id: "vm-start-1", State: nodepb.VmState_VM_STATE_STOPPED},
		},
		startResp: &nodepb.StartVmResponse{
			Status: &nodepb.VmStatus{Id: "vm-start-1", State: nodepb.VmState_VM_STATE_RUNNING},
		},
	}
	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId: "n1", Hostname: "h1", Address: "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16e9},
	})
	s.nodes["n1"].Client = mock

	s.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec: &ctrlpb.VmSpec{Id: "vm-start-1", Name: "start-vm", Cpu: 1, MemoryBytes: 1e9},
	})

	resp, err := s.StartVm(ctx, &ctrlpb.StartVmRequest{VmId: "vm-start-1"})
	if err != nil {
		t.Fatalf("StartVm: %v", err)
	}
	if resp.State != ctrlpb.VmState_VM_STATE_RUNNING {
		t.Errorf("State=%v, want RUNNING", resp.State)
	}

	vm, _ := db.GetVM("vm-start-1")
	if vm.DesiredState != "running" {
		t.Errorf("DB DesiredState=%q", vm.DesiredState)
	}
}

func TestStopVm(t *testing.T) {
	s, db := newTestServerWithDB(t)
	ctx := context.Background()

	mock := &mockNodeClient{
		createResp: &nodepb.CreateVmResponse{
			Status: &nodepb.VmStatus{Id: "vm-stop-1", State: nodepb.VmState_VM_STATE_STOPPED},
		},
		stopResp: &nodepb.StopVmResponse{
			Status: &nodepb.VmStatus{Id: "vm-stop-1", State: nodepb.VmState_VM_STATE_STOPPED},
		},
	}
	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId: "n1", Hostname: "h1", Address: "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16e9},
	})
	s.nodes["n1"].Client = mock

	s.CreateVm(ctx, &ctrlpb.CreateVmRequest{
		Spec: &ctrlpb.VmSpec{Id: "vm-stop-1", Name: "stop-vm", Cpu: 1, MemoryBytes: 1e9},
	})

	resp, err := s.StopVm(ctx, &ctrlpb.StopVmRequest{VmId: "vm-stop-1", Force: true})
	if err != nil {
		t.Fatalf("StopVm: %v", err)
	}
	if resp.State != ctrlpb.VmState_VM_STATE_STOPPED {
		t.Errorf("State=%v", resp.State)
	}

	vm, _ := db.GetVM("vm-stop-1")
	if vm.DesiredState != "stopped" {
		t.Errorf("DB DesiredState=%q", vm.DesiredState)
	}
}

func TestGetVm(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	mock := &mockNodeClient{
		getResp: &nodepb.GetVmResponse{
			Spec: &nodepb.VmSpec{
				Id:          "vm-get-1",
				Name:        "get-vm",
				Cpu:         4,
				MemoryBytes: 8e9,
				Disks: []*nodepb.Disk{
					{Name: "root", BackendHandle: "/disk.qcow2", Bus: "virtio", Device: "vda"},
				},
				Nics: []*nodepb.Nic{
					{Network: "default", Model: "virtio", MacAddress: "52:54:00:aa:bb:cc"},
				},
			},
			Status: &nodepb.VmStatus{
				Id:    "vm-get-1",
				State: nodepb.VmState_VM_STATE_RUNNING,
			},
		},
	}
	addMockNode(s, "n1", "10.0.0.1:9091", mock)
	s.vmToNode["vm-get-1"] = "n1"

	resp, err := s.GetVm(ctx, &ctrlpb.GetVmRequest{VmId: "vm-get-1"})
	if err != nil {
		t.Fatalf("GetVm: %v", err)
	}
	if resp.Spec.Name != "get-vm" {
		t.Errorf("Name=%q", resp.Spec.Name)
	}
	if resp.Spec.Cpu != 4 {
		t.Errorf("Cpu=%d", resp.Spec.Cpu)
	}
	if resp.NodeId != "n1" {
		t.Errorf("NodeId=%q", resp.NodeId)
	}
	if len(resp.Spec.Disks) != 1 {
		t.Fatalf("Disks=%d", len(resp.Spec.Disks))
	}
	if resp.Spec.Disks[0].Name != "root" {
		t.Errorf("Disks[0].Name=%q", resp.Spec.Disks[0].Name)
	}
	if len(resp.Spec.Nics) != 1 {
		t.Fatalf("Nics=%d", len(resp.Spec.Nics))
	}
	if resp.Spec.Nics[0].MacAddress != "52:54:00:aa:bb:cc" {
		t.Errorf("Nics[0].MacAddress=%q", resp.Spec.Nics[0].MacAddress)
	}
}

func TestGetVm_UnknownVM(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	_, err := s.GetVm(ctx, &ctrlpb.GetVmRequest{VmId: "nonexistent"})
	if err == nil {
		t.Fatal("expected error for unknown VM")
	}
}

func TestListVms_AllNodes(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	mock1 := &mockNodeClient{
		listResp: &nodepb.ListVmsResponse{
			Vms: []*nodepb.VmInfo{
				{Id: "vm-1", Name: "vm-a", State: nodepb.VmState_VM_STATE_RUNNING, Cpu: 2, MemoryBytes: 4e9},
			},
		},
	}
	mock2 := &mockNodeClient{
		listResp: &nodepb.ListVmsResponse{
			Vms: []*nodepb.VmInfo{
				{Id: "vm-2", Name: "vm-b", State: nodepb.VmState_VM_STATE_STOPPED, Cpu: 1, MemoryBytes: 2e9},
				{Id: "vm-3", Name: "vm-c", State: nodepb.VmState_VM_STATE_RUNNING, Cpu: 4, MemoryBytes: 8e9},
			},
		},
	}
	addMockNode(s, "n1", "10.0.0.1:9091", mock1)
	addMockNode(s, "n2", "10.0.0.2:9091", mock2)

	resp, err := s.ListVms(ctx, &ctrlpb.ListVmsRequest{})
	if err != nil {
		t.Fatalf("ListVms: %v", err)
	}
	if len(resp.Vms) != 3 {
		t.Fatalf("Vms=%d, want 3", len(resp.Vms))
	}
}

func TestListVms_TargetNode(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	mock := &mockNodeClient{
		listResp: &nodepb.ListVmsResponse{
			Vms: []*nodepb.VmInfo{
				{Id: "vm-1", Name: "vm-a", State: nodepb.VmState_VM_STATE_RUNNING},
			},
		},
	}
	addMockNode(s, "n1", "10.0.0.1:9091", mock)
	addMockNode(s, "n2", "10.0.0.2:9091", &mockNodeClient{
		listResp: &nodepb.ListVmsResponse{
			Vms: []*nodepb.VmInfo{
				{Id: "vm-2", Name: "vm-b"},
				{Id: "vm-3", Name: "vm-c"},
			},
		},
	})

	resp, err := s.ListVms(ctx, &ctrlpb.ListVmsRequest{TargetNode: "10.0.0.1:9091"})
	if err != nil {
		t.Fatalf("ListVms: %v", err)
	}
	if len(resp.Vms) != 1 {
		t.Fatalf("Vms=%d, want 1", len(resp.Vms))
	}
	if resp.Vms[0].NodeId != "n1" {
		t.Errorf("NodeId=%q, want n1", resp.Vms[0].NodeId)
	}
}

func TestListNodes_WithData(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId: "n1", Hostname: "host-1", Address: "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16e9},
	})
	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId: "n2", Hostname: "host-2", Address: "10.0.0.2:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 16, MemoryBytes: 64e9},
	})

	resp, err := s.ListNodes(ctx, &ctrlpb.ListNodesRequest{})
	if err != nil {
		t.Fatalf("ListNodes: %v", err)
	}
	if len(resp.Nodes) != 2 {
		t.Errorf("Nodes=%d, want 2", len(resp.Nodes))
	}
}

func TestGetNode_Found(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	s.RegisterNode(ctx, &ctrlpb.RegisterNodeRequest{
		NodeId: "n1", Hostname: "host-1", Address: "10.0.0.1:9091",
		Capacity: &ctrlpb.NodeCapacity{CpuCores: 8, MemoryBytes: 16e9},
	})

	resp, err := s.GetNode(ctx, &ctrlpb.GetNodeRequest{NodeId: "n1"})
	if err != nil {
		t.Fatalf("GetNode: %v", err)
	}
	if resp.Node.Hostname != "host-1" {
		t.Errorf("Hostname=%q", resp.Node.Hostname)
	}
	if resp.Node.Status != "ready" {
		t.Errorf("Status=%q", resp.Node.Status)
	}
}

func TestSyncVmState(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	addMockNode(s, "n1", "10.0.0.1:9091", &mockNodeClient{})

	resp, err := s.SyncVmState(ctx, &ctrlpb.SyncVmStateRequest{
		NodeId: "n1",
		Vms: []*ctrlpb.VmInfo{
			{Id: "vm-1", State: ctrlpb.VmState_VM_STATE_RUNNING},
			{Id: "vm-2", State: ctrlpb.VmState_VM_STATE_STOPPED},
		},
	})
	if err != nil {
		t.Fatalf("SyncVmState: %v", err)
	}
	if !resp.Success {
		t.Error("expected success")
	}

	// VMs should now be tracked
	if s.vmToNode["vm-1"] != "n1" {
		t.Errorf("vmToNode[vm-1]=%q", s.vmToNode["vm-1"])
	}
	if s.vmToNode["vm-2"] != "n1" {
		t.Errorf("vmToNode[vm-2]=%q", s.vmToNode["vm-2"])
	}
}

func TestSyncVmState_UnknownNode(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	_, err := s.SyncVmState(ctx, &ctrlpb.SyncVmStateRequest{
		NodeId: "nonexistent",
	})
	if err == nil {
		t.Fatal("expected error for unknown node")
	}
}

func TestSyncVmState_RemovesStaleVMs(t *testing.T) {
	s := NewServer()
	ctx := context.Background()

	addMockNode(s, "n1", "10.0.0.1:9091", &mockNodeClient{})
	s.vmToNode["vm-stale"] = "n1"

	s.SyncVmState(ctx, &ctrlpb.SyncVmStateRequest{
		NodeId: "n1",
		Vms:    []*ctrlpb.VmInfo{},
	})

	if _, ok := s.vmToNode["vm-stale"]; ok {
		t.Error("stale VM should have been removed")
	}
}

func TestVmStateString(t *testing.T) {
	tests := []struct {
		state ctrlpb.VmState
		want  string
	}{
		{ctrlpb.VmState_VM_STATE_RUNNING, "running"},
		{ctrlpb.VmState_VM_STATE_STOPPED, "stopped"},
		{ctrlpb.VmState_VM_STATE_PAUSED, "paused"},
		{ctrlpb.VmState_VM_STATE_ERROR, "error"},
		{ctrlpb.VmState_VM_STATE_UNKNOWN, "unknown"},
		{ctrlpb.VmState(99), "unknown"},
	}
	for _, tt := range tests {
		if got := vmStateString(tt.state); got != tt.want {
			t.Errorf("vmStateString(%v)=%q, want %q", tt.state, got, tt.want)
		}
	}
}

func TestConvertDisksFromNode(t *testing.T) {
	input := []*nodepb.Disk{
		{Name: "root", BackendHandle: "/disk.qcow2", Bus: "virtio", Device: "vda"},
	}
	result := convertDisksFromNode(input)
	if len(result) != 1 {
		t.Fatalf("len=%d", len(result))
	}
	if result[0].Name != "root" {
		t.Errorf("Name=%q", result[0].Name)
	}
	if result[0].BackendHandle != "/disk.qcow2" {
		t.Errorf("BackendHandle=%q", result[0].BackendHandle)
	}
}

func TestConvertNicsFromNode(t *testing.T) {
	input := []*nodepb.Nic{
		{Network: "default", Model: "virtio", MacAddress: "52:54:00:aa:bb:cc"},
	}
	result := convertNicsFromNode(input)
	if len(result) != 1 {
		t.Fatalf("len=%d", len(result))
	}
	if result[0].Network != "default" {
		t.Errorf("Network=%q", result[0].Network)
	}
}

func TestFindNodeForVm_ByTargetAddr(t *testing.T) {
	s := NewServer()
	addMockNode(s, "n1", "10.0.0.1:9091", &mockNodeClient{})

	node, err := s.findNodeForVm("any-vm", "10.0.0.1:9091")
	if err != nil {
		t.Fatalf("findNodeForVm: %v", err)
	}
	if node.ID != "n1" {
		t.Errorf("ID=%q", node.ID)
	}
}

func TestFindNodeForVm_ByVmToNode(t *testing.T) {
	s := NewServer()
	addMockNode(s, "n1", "10.0.0.1:9091", &mockNodeClient{})
	s.vmToNode["vm-1"] = "n1"

	node, err := s.findNodeForVm("vm-1", "")
	if err != nil {
		t.Fatalf("findNodeForVm: %v", err)
	}
	if node.ID != "n1" {
		t.Errorf("ID=%q", node.ID)
	}
}

func TestFindNodeForVm_UnknownVM(t *testing.T) {
	s := NewServer()
	_, err := s.findNodeForVm("unknown-vm", "")
	if err == nil {
		t.Fatal("expected error")
	}
}

func TestConvertNics_Nil(t *testing.T) {
	result := convertNics(nil)
	if len(result) != 0 {
		t.Errorf("len=%d", len(result))
	}
}
