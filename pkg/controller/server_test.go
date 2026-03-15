package controller

import (
	"context"
	"testing"

	ctrlpb "github.com/kcore/kcore/api/controller"
	nodepb "github.com/kcore/kcore/api/node"
)

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
