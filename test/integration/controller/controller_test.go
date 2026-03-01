package controller_test

import (
	"context"
	"testing"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	ctrlpb "github.com/kcore/kcore/api/controller"
	"github.com/kcore/kcore/pkg/controller"
)

const (
	testControllerAddr = "localhost:8080"
	testTimeout        = 10 * time.Second
)

// TestControllerBasicOperations tests basic controller functionality
func TestControllerBasicOperations(t *testing.T) {
	// Connect to controller
	conn, err := grpc.Dial(testControllerAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		t.Skipf("Controller not available at %s: %v", testControllerAddr, err)
		return
	}
	defer conn.Close()

	client := ctrlpb.NewControllerClient(conn)
	ctx, cancel := context.WithTimeout(context.Background(), testTimeout)
	defer cancel()

	t.Run("ListNodes", func(t *testing.T) {
		resp, err := client.ListNodes(ctx, &ctrlpb.ListNodesRequest{})
		if err != nil {
			t.Fatalf("ListNodes failed: %v", err)
		}

		t.Logf("Found %d nodes", len(resp.Nodes))
		for _, node := range resp.Nodes {
			t.Logf("  - Node: %s (%s) at %s", node.NodeId, node.Hostname, node.Address)
		}
	})
}

// TestNodeRegistration tests node registration flow
func TestNodeRegistration(t *testing.T) {
	// Create a test server
	srv := controller.NewServer()

	ctx := context.Background()

	// Test node registration
	req := &ctrlpb.RegisterNodeRequest{
		NodeId:   "test-node-1",
		Hostname: "test-host",
		Address:  "127.0.0.1:9999",
		Capacity: &ctrlpb.NodeCapacity{
			CpuCores:    16,
			MemoryBytes: 68719476736, // 64GB
		},
	}

	// Note: gRPC Dial doesn't fail immediately for unreachable addresses
	// The connection is established lazily on first RPC call
	resp, err := srv.RegisterNode(ctx, req)
	if err != nil {
		t.Fatalf("RegisterNode returned error: %v", err)
	}

	// Registration should respond
	if resp == nil {
		t.Fatal("Expected response from registration")
	}

	// For now, registration may succeed even if node is unreachable
	// The connection will fail on actual RPC calls to the node
	t.Logf("Registration response: success=%v, message=%s", resp.Success, resp.Message)
}

// TestNodeListing tests node listing functionality
func TestNodeListing(t *testing.T) {
	srv := controller.NewServer()
	ctx := context.Background()

	// Initially empty
	resp, err := srv.ListNodes(ctx, &ctrlpb.ListNodesRequest{})
	if err != nil {
		t.Fatalf("ListNodes failed: %v", err)
	}

	if len(resp.Nodes) != 0 {
		t.Errorf("Expected 0 nodes, got %d", len(resp.Nodes))
	}
}

// TestVmToNodeTracking tests VM-to-node mapping
func TestVmToNodeTracking(t *testing.T) {
	// This test verifies the controller tracks which VMs are on which nodes
	t.Run("VmLocationTracking", func(t *testing.T) {
		srv := controller.NewServer()

		// Test that VM lookup fails for unknown VM
		ctx := context.Background()
		_, err := srv.GetVm(ctx, &ctrlpb.GetVmRequest{
			VmId: "nonexistent-vm",
		})

		if err == nil {
			t.Error("Expected error for nonexistent VM")
		}
	})
}

// TestControllerScheduling tests node selection logic
func TestControllerScheduling(t *testing.T) {
	t.Run("NoNodesAvailable", func(t *testing.T) {
		srv := controller.NewServer()
		ctx := context.Background()

		// Attempt to create VM with no nodes
		_, err := srv.CreateVm(ctx, &ctrlpb.CreateVmRequest{
			Spec: &ctrlpb.VmSpec{
				Id:          "test-vm",
				Name:        "test-vm",
				Cpu:         2,
				MemoryBytes: 4294967296, // 4GB
			},
		})

		if err == nil {
			t.Error("Expected error when no nodes available")
		}
	})
}

// TestControllerHeartbeat tests heartbeat mechanism
func TestControllerHeartbeat(t *testing.T) {
	srv := controller.NewServer()
	ctx := context.Background()

	// Heartbeat for non-existent node should fail
	_, err := srv.Heartbeat(ctx, &ctrlpb.HeartbeatRequest{
		NodeId: "nonexistent-node",
		Usage: &ctrlpb.NodeUsage{
			CpuCoresUsed:    2,
			MemoryBytesUsed: 1073741824, // 1GB
		},
	})

	if err == nil {
		t.Error("Expected error for heartbeat from unknown node")
	}
}
