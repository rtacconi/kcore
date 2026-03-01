package controller

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/exec"
	"strings"
	"sync"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/types/known/timestamppb"

	ctrlpb "github.com/kcore/kcore/api/controller"
	nodepb "github.com/kcore/kcore/api/node"
)

// Server implements the Controller service
type Server struct {
	ctrlpb.UnimplementedControllerServer
	ctrlpb.UnimplementedControllerAdminServer
	nodes         map[string]*NodeInfo // nodeID -> NodeInfo
	vmToNode      map[string]string    // vmID -> nodeID
	nodeDialCreds credentials.TransportCredentials
	mu            sync.RWMutex
}

type NodeInfo struct {
	ID            string
	Hostname      string
	Address       string
	Capacity      *ctrlpb.NodeCapacity
	Usage         *ctrlpb.NodeUsage
	LastHeartbeat time.Time
	Status        string
	Client        nodepb.NodeComputeClient
	Conn          *grpc.ClientConn
}

func NewServer() *Server {
	return &Server{
		nodes:    make(map[string]*NodeInfo),
		vmToNode: make(map[string]string),
	}
}

func (s *Server) SetNodeDialCredentials(creds credentials.TransportCredentials) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.nodeDialCreds = creds
}

// ControllerAdmin implementation

// ApplyNixConfig writes the provided configuration.nix to /etc/nixos/configuration.nix
// and optionally runs `nixos-rebuild switch`. This mirrors the node-side API and is
// intended for development / lab usage (no auth, no RBAC yet).
func (s *Server) ApplyNixConfig(ctx context.Context, req *ctrlpb.ApplyNixConfigRequest) (*ctrlpb.ApplyNixConfigResponse, error) {
	if req.GetConfigurationNix() == "" {
		return &ctrlpb.ApplyNixConfigResponse{
			Success: false,
			Message: "configuration_nix is empty",
		}, nil
	}

	const cfgPath = "/etc/nixos/configuration.nix"

	if err := os.MkdirAll("/etc/nixos", 0755); err != nil {
		return &ctrlpb.ApplyNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("failed to create /etc/nixos: %v", err),
		}, nil
	}

	if err := os.WriteFile(cfgPath, []byte(req.GetConfigurationNix()), 0644); err != nil {
		return &ctrlpb.ApplyNixConfigResponse{
			Success: false,
			Message: fmt.Sprintf("failed to write %s: %v", cfgPath, err),
		}, nil
	}

	if req.GetRebuild() {
		cmd := exec.CommandContext(ctx, "nixos-rebuild", "switch")
		output, err := cmd.CombinedOutput()
		if err != nil {
			return &ctrlpb.ApplyNixConfigResponse{
				Success: false,
				Message: fmt.Sprintf("nixos-rebuild switch failed: %v (output: %s)", err, strings.TrimSpace(string(output))),
			}, nil
		}
	}

	return &ctrlpb.ApplyNixConfigResponse{
		Success: true,
		Message: "configuration applied",
	}, nil
}

// Node Registration

func (s *Server) RegisterNode(ctx context.Context, req *ctrlpb.RegisterNodeRequest) (*ctrlpb.RegisterNodeResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	var (
		conn         *grpc.ClientConn
		client       nodepb.NodeComputeClient
		status       = "ready"
		responseMsg  = "Node registered successfully"
		dialWarnText string
	)
	if s.nodeDialCreds != nil && req.Address != "" {
		dialConn, err := grpc.Dial(req.Address, grpc.WithTransportCredentials(s.nodeDialCreds))
	if err != nil {
			status = "degraded"
			dialWarnText = fmt.Sprintf(" (initial node dial failed: %v)", err)
			responseMsg = "Node registered, but initial controller-to-node connection failed"
		} else {
			conn = dialConn
			client = nodepb.NewNodeComputeClient(dialConn)
		}
	}

	nodeInfo := &NodeInfo{
		ID:            req.NodeId,
		Hostname:      req.Hostname,
		Address:       req.Address,
		Capacity:      req.Capacity,
		Usage:         &ctrlpb.NodeUsage{},
		LastHeartbeat: time.Now(),
		Status:        status,
		Client:        client,
		Conn:          conn,
	}

	s.nodes[req.NodeId] = nodeInfo

	log.Printf("Node registered: %s (%s) at %s%s", req.NodeId, req.Hostname, req.Address, dialWarnText)

	return &ctrlpb.RegisterNodeResponse{
		Success: true,
		Message: responseMsg,
	}, nil
}

func (s *Server) Heartbeat(ctx context.Context, req *ctrlpb.HeartbeatRequest) (*ctrlpb.HeartbeatResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	node, exists := s.nodes[req.NodeId]
	if !exists {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", req.NodeId)
	}

	node.Usage = req.Usage
	node.LastHeartbeat = time.Now()
	node.Status = "ready"

	return &ctrlpb.HeartbeatResponse{
		Success: true,
	}, nil
}

func (s *Server) SyncVmState(ctx context.Context, req *ctrlpb.SyncVmStateRequest) (*ctrlpb.SyncVmStateResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	_, exists := s.nodes[req.NodeId]
	if !exists {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", req.NodeId)
	}

	// Get current VMs for this node
	currentVMIDs := make(map[string]bool)
	for _, vm := range req.Vms {
		currentVMIDs[vm.Id] = true
	}

	// Remove VMs that no longer exist on the node
	for vmID, nodeID := range s.vmToNode {
		if nodeID == req.NodeId && !currentVMIDs[vmID] {
			log.Printf("VM %s removed from node %s (no longer exists in libvirt)", vmID, req.NodeId)
			delete(s.vmToNode, vmID)
		}
	}

	// Add/update VMs that exist on the node
	for _, vm := range req.Vms {
		existingNode, exists := s.vmToNode[vm.Id]
		if !exists {
			log.Printf("VM %s discovered on node %s", vm.Id, req.NodeId)
			s.vmToNode[vm.Id] = req.NodeId
		} else if existingNode != req.NodeId {
			log.Printf("VM %s moved from node %s to %s", vm.Id, existingNode, req.NodeId)
			s.vmToNode[vm.Id] = req.NodeId
		}
	}

	log.Printf("State sync from node %s: %d VMs", req.NodeId, len(req.Vms))
	return &ctrlpb.SyncVmStateResponse{
		Success: true,
	}, nil
}

// VM Operations

func (s *Server) CreateVm(ctx context.Context, req *ctrlpb.CreateVmRequest) (*ctrlpb.CreateVmResponse, error) {
	// Find target node
	var targetNode *NodeInfo
	var err error

	if req.TargetNode != "" {
		// Explicit node specified
		targetNode, err = s.getNodeByAddress(req.TargetNode)
		if err != nil {
			return nil, status.Errorf(codes.NotFound, "target node not found: %s", req.TargetNode)
		}
	} else {
		// Auto-select node (future: smart scheduling)
		targetNode = s.selectNode()
		if targetNode == nil {
			return nil, status.Error(codes.Unavailable, "no available nodes")
		}
	}

	// Convert controller VmSpec to node VmSpec
	nodeSpec := &nodepb.VmSpec{
		Id:          req.Spec.Id,
		Name:        req.Spec.Name,
		Cpu:         req.Spec.Cpu,
		MemoryBytes: req.Spec.MemoryBytes,
		Disks:       convertDisks(req.Spec.Disks),
		Nics:        convertNics(req.Spec.Nics),
	}

	// Forward to node
	resp, err := targetNode.Client.CreateVm(ctx, &nodepb.CreateVmRequest{
		Spec: nodeSpec,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to create VM on node: %v", err)
	}

	// Track VM location
	s.mu.Lock()
	s.vmToNode[req.Spec.Id] = targetNode.ID
	s.mu.Unlock()

	log.Printf("VM created: %s on node %s", req.Spec.Name, targetNode.ID)

	return &ctrlpb.CreateVmResponse{
		VmId:   resp.Status.Id,
		NodeId: targetNode.ID,
		State:  convertVmState(resp.Status.State),
	}, nil
}

func (s *Server) DeleteVm(ctx context.Context, req *ctrlpb.DeleteVmRequest) (*ctrlpb.DeleteVmResponse, error) {
	// Find node with this VM
	var targetNode *NodeInfo
	var err error

	if req.TargetNode != "" {
		targetNode, err = s.getNodeByAddress(req.TargetNode)
		if err != nil {
			return nil, status.Errorf(codes.NotFound, "target node not found: %s", req.TargetNode)
		}
	} else {
		s.mu.RLock()
		nodeID, ok := s.vmToNode[req.VmId]
		s.mu.RUnlock()

		if !ok {
			return nil, status.Errorf(codes.NotFound, "VM location unknown: %s", req.VmId)
		}

		s.mu.RLock()
		targetNode = s.nodes[nodeID]
		s.mu.RUnlock()

		if targetNode == nil {
			return nil, status.Errorf(codes.NotFound, "node not found: %s", nodeID)
		}
	}

	// Forward to node
	_, err = targetNode.Client.DeleteVm(ctx, &nodepb.DeleteVmRequest{
		VmId: req.VmId,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to delete VM: %v", err)
	}

	// Remove tracking
	s.mu.Lock()
	delete(s.vmToNode, req.VmId)
	s.mu.Unlock()

	log.Printf("VM deleted: %s from node %s", req.VmId, targetNode.ID)

	return &ctrlpb.DeleteVmResponse{
		Success: true,
	}, nil
}

func (s *Server) StartVm(ctx context.Context, req *ctrlpb.StartVmRequest) (*ctrlpb.StartVmResponse, error) {
	targetNode, err := s.findNodeForVm(req.VmId, req.TargetNode)
	if err != nil {
		return nil, err
	}

	resp, err := targetNode.Client.StartVm(ctx, &nodepb.StartVmRequest{
		VmId: req.VmId,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to start VM: %v", err)
	}

	return &ctrlpb.StartVmResponse{
		State: convertVmState(resp.Status.State),
	}, nil
}

func (s *Server) StopVm(ctx context.Context, req *ctrlpb.StopVmRequest) (*ctrlpb.StopVmResponse, error) {
	targetNode, err := s.findNodeForVm(req.VmId, req.TargetNode)
	if err != nil {
		return nil, err
	}

	resp, err := targetNode.Client.StopVm(ctx, &nodepb.StopVmRequest{
		VmId:  req.VmId,
		Force: req.Force,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to stop VM: %v", err)
	}

	return &ctrlpb.StopVmResponse{
		State: convertVmState(resp.Status.State),
	}, nil
}

func (s *Server) GetVm(ctx context.Context, req *ctrlpb.GetVmRequest) (*ctrlpb.GetVmResponse, error) {
	targetNode, err := s.findNodeForVm(req.VmId, req.TargetNode)
	if err != nil {
		return nil, err
	}

	resp, err := targetNode.Client.GetVm(ctx, &nodepb.GetVmRequest{
		VmId: req.VmId,
	})
	if err != nil {
		return nil, status.Errorf(codes.Internal, "failed to get VM: %v", err)
	}

	var spec *ctrlpb.VmSpec
	if resp.Spec != nil {
		spec = &ctrlpb.VmSpec{
			Id:          resp.Spec.Id,
			Name:        resp.Spec.Name,
			Cpu:         resp.Spec.Cpu,
			MemoryBytes: resp.Spec.MemoryBytes,
			Disks:       convertDisksFromNode(resp.Spec.Disks),
			Nics:        convertNicsFromNode(resp.Spec.Nics),
		}
	}

	return &ctrlpb.GetVmResponse{
		Spec:   spec,
		Status: convertVmStatus(resp.Status),
		NodeId: targetNode.ID,
	}, nil
}

func (s *Server) ListVms(ctx context.Context, req *ctrlpb.ListVmsRequest) (*ctrlpb.ListVmsResponse, error) {
	var vms []*ctrlpb.VmInfo

	if req.TargetNode != "" {
		// List from specific node
		targetNode, err := s.getNodeByAddress(req.TargetNode)
		if err != nil {
			return nil, status.Errorf(codes.NotFound, "target node not found: %s", req.TargetNode)
		}

		resp, err := targetNode.Client.ListVms(ctx, &nodepb.ListVmsRequest{})
		if err != nil {
			return nil, status.Errorf(codes.Internal, "failed to list VMs from node: %v", err)
		}

		for _, vm := range resp.Vms {
			vms = append(vms, &ctrlpb.VmInfo{
				Id:          vm.Id,
				Name:        vm.Name,
				State:       convertVmState(vm.State),
				Cpu:         vm.Cpu,
				MemoryBytes: vm.MemoryBytes,
				NodeId:      targetNode.ID,
				CreatedAt:   vm.CreatedAt,
			})
		}
	} else {
		// List from all nodes
		s.mu.RLock()
		nodes := make([]*NodeInfo, 0, len(s.nodes))
		for _, node := range s.nodes {
			nodes = append(nodes, node)
		}
		s.mu.RUnlock()

		for _, node := range nodes {
			resp, err := node.Client.ListVms(ctx, &nodepb.ListVmsRequest{})
			if err != nil {
				log.Printf("Warning: failed to list VMs from node %s: %v", node.ID, err)
				continue
			}

			for _, vm := range resp.Vms {
				vms = append(vms, &ctrlpb.VmInfo{
					Id:          vm.Id,
					Name:        vm.Name,
					State:       convertVmState(vm.State),
					Cpu:         vm.Cpu,
					MemoryBytes: vm.MemoryBytes,
					NodeId:      node.ID,
					CreatedAt:   vm.CreatedAt,
				})
			}
		}
	}

	return &ctrlpb.ListVmsResponse{
		Vms: vms,
	}, nil
}

// Node Operations

func (s *Server) ListNodes(ctx context.Context, req *ctrlpb.ListNodesRequest) (*ctrlpb.ListNodesResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	nodes := make([]*ctrlpb.NodeInfo, 0, len(s.nodes))
	for _, node := range s.nodes {
		nodes = append(nodes, &ctrlpb.NodeInfo{
			NodeId:        node.ID,
			Hostname:      node.Hostname,
			Address:       node.Address,
			Capacity:      node.Capacity,
			Usage:         node.Usage,
			Status:        node.Status,
			LastHeartbeat: timestamppb.New(node.LastHeartbeat),
		})
	}

	return &ctrlpb.ListNodesResponse{
		Nodes: nodes,
	}, nil
}

func (s *Server) GetNode(ctx context.Context, req *ctrlpb.GetNodeRequest) (*ctrlpb.GetNodeResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	node, exists := s.nodes[req.NodeId]
	if !exists {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", req.NodeId)
	}

	return &ctrlpb.GetNodeResponse{
		Node: &ctrlpb.NodeInfo{
			NodeId:        node.ID,
			Hostname:      node.Hostname,
			Address:       node.Address,
			Capacity:      node.Capacity,
			Usage:         node.Usage,
			Status:        node.Status,
			LastHeartbeat: timestamppb.New(node.LastHeartbeat),
		},
	}, nil
}

// Helper functions

func (s *Server) getNodeByAddress(address string) (*NodeInfo, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	for _, node := range s.nodes {
		if node.Address == address {
			return node, nil
		}
	}
	return nil, fmt.Errorf("node not found with address: %s", address)
}

func (s *Server) findNodeForVm(vmID, targetNodeAddr string) (*NodeInfo, error) {
	if targetNodeAddr != "" {
		return s.getNodeByAddress(targetNodeAddr)
	}

	s.mu.RLock()
	nodeID, ok := s.vmToNode[vmID]
	s.mu.RUnlock()

	if !ok {
		return nil, status.Errorf(codes.NotFound, "VM location unknown: %s", vmID)
	}

	s.mu.RLock()
	node := s.nodes[nodeID]
	s.mu.RUnlock()

	if node == nil {
		return nil, status.Errorf(codes.NotFound, "node not found: %s", nodeID)
	}

	return node, nil
}

func (s *Server) selectNode() *NodeInfo {
	s.mu.RLock()
	defer s.mu.RUnlock()

	// Simple: return first available node
	// Future: implement smart scheduling based on capacity
	for _, node := range s.nodes {
		if node.Status == "ready" {
			return node
		}
	}
	return nil
}

// Conversion functions

func convertDisks(disks []*ctrlpb.Disk) []*nodepb.Disk {
	result := make([]*nodepb.Disk, len(disks))
	for i, d := range disks {
		result[i] = &nodepb.Disk{
			Name:          d.Name,
			BackendHandle: d.BackendHandle,
			Bus:           d.Bus,
			Device:        d.Device,
		}
	}
	return result
}

func convertNics(nics []*ctrlpb.Nic) []*nodepb.Nic {
	result := make([]*nodepb.Nic, len(nics))
	for i, n := range nics {
		result[i] = &nodepb.Nic{
			Network:    n.Network,
			Model:      n.Model,
			MacAddress: n.MacAddress,
		}
	}
	return result
}

func convertDisksFromNode(disks []*nodepb.Disk) []*ctrlpb.Disk {
	result := make([]*ctrlpb.Disk, len(disks))
	for i, d := range disks {
		result[i] = &ctrlpb.Disk{
			Name:          d.Name,
			BackendHandle: d.BackendHandle,
			Bus:           d.Bus,
			Device:        d.Device,
		}
	}
	return result
}

func convertNicsFromNode(nics []*nodepb.Nic) []*ctrlpb.Nic {
	result := make([]*ctrlpb.Nic, len(nics))
	for i, n := range nics {
		result[i] = &ctrlpb.Nic{
			Network:    n.Network,
			Model:      n.Model,
			MacAddress: n.MacAddress,
		}
	}
	return result
}

func convertVmState(state nodepb.VmState) ctrlpb.VmState {
	switch state {
	case nodepb.VmState_VM_STATE_STOPPED:
		return ctrlpb.VmState_VM_STATE_STOPPED
	case nodepb.VmState_VM_STATE_RUNNING:
		return ctrlpb.VmState_VM_STATE_RUNNING
	case nodepb.VmState_VM_STATE_PAUSED:
		return ctrlpb.VmState_VM_STATE_PAUSED
	case nodepb.VmState_VM_STATE_ERROR:
		return ctrlpb.VmState_VM_STATE_ERROR
	default:
		return ctrlpb.VmState_VM_STATE_UNKNOWN
	}
}

func convertVmStatus(status *nodepb.VmStatus) *ctrlpb.VmStatus {
	if status == nil {
		return nil
	}
	return &ctrlpb.VmStatus{
		Id:        status.Id,
		State:     convertVmState(status.State),
		CreatedAt: status.CreatedAt,
		UpdatedAt: status.UpdatedAt,
	}
}
